use super::*;
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeviceState {
    Offline,
    Online,
    Communicating,
}
impl DeviceState {
    pub fn wire_value(self) -> u8 {
        match self {
            Self::Offline => 0,
            Self::Online => 1,
            Self::Communicating => 2,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolIdentity {
    pub protocol_id: String,
    pub address: Vec<u8>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Device {
    pub id: String,
    pub system_id: String,
    pub name: String,
    pub identities: Vec<ProtocolIdentity>,
    pub state: DeviceState,
    pub origin: String,
    pub config_version: u64,
    pub config: Value,
    pub registration_valid: bool,
    pub active_calls: BTreeSet<String>,
    pub max_concurrent_calls: usize,
    pub last_seen_ms: Option<u64>,
    pub cleanup_until_ms: Option<u64>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: String,
    pub device_id: String,
    pub peer_system_id: String,
    pub wire_session_id: u16,
    pub epoch: String,
    pub valid_until_ms: u64,
    pub auth_required: bool,
    pub auth_expires_ms: Option<u64>,
    pub registered: bool,
    pub active: bool,
    /// Runtime-only secret, excluded from snapshots, Debug and exports.
    #[serde(skip)]
    token: Vec<u8>,
}
impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("id", &self.id)
            .field("device_id", &self.device_id)
            .field("wire_session_id", &self.wire_session_id)
            .field("token", &"[REDACTED]")
            .finish()
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConfigMode {
    Devices,
    Parameters,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConfigOperation {
    Replace,
    Merge,
    Delete,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApplyTrigger {
    Received,
    ResponseSent,
    #[default]
    SuccessAck,
    Event,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigItem {
    pub address: Option<Vec<u8>>,
    pub key: String,
    pub values: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigRequest {
    /// Scope by endpoint, protocol, session, transaction and source.
    pub request_key: String,
    pub system_id: String,
    pub protocol_id: String,
    pub mode: ConfigMode,
    pub operation: ConfigOperation,
    pub target_address: Option<Vec<u8>>,
    pub version: u64,
    pub items: Vec<ConfigItem>,
    #[serde(default)]
    pub effective_at: ApplyTrigger,
    #[serde(default)]
    pub atomic: bool,
    /// False while a candidate segmented batch is incomplete.
    #[serde(default = "true_value")]
    pub batch_complete: bool,
}
fn true_value() -> bool {
    true
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigItemResult {
    pub index: usize,
    pub code: u8,
    pub confirmed: bool,
    pub applied: bool,
    pub device_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigOutcome {
    pub request_key: String,
    pub committed: bool,
    pub deferred: bool,
    pub items: Vec<ConfigItemResult>,
}

impl DomainStore {
    pub fn create_device(
        &mut self,
        system_id: &str,
        name: &str,
        protocol_id: &str,
        address: Vec<u8>,
        origin: &str,
    ) -> DomainResult<String> {
        if address.is_empty() {
            return Err("设备协议地址不能为空".into());
        }
        if self.find_device(system_id, protocol_id, &address).is_some() {
            return Err("同系统协议地址已存在".into());
        }
        let device_id = id();
        self.devices.insert(
            device_id.clone(),
            Device {
                id: device_id.clone(),
                system_id: system_id.into(),
                name: name.into(),
                identities: vec![ProtocolIdentity {
                    protocol_id: protocol_id.into(),
                    address,
                }],
                state: DeviceState::Offline,
                origin: origin.into(),
                config_version: 0,
                config: json!({}),
                registration_valid: false,
                active_calls: BTreeSet::new(),
                max_concurrent_calls: 8,
                last_seen_ms: None,
                cleanup_until_ms: None,
            },
        );
        Ok(device_id)
    }
    pub fn find_device(&self, system: &str, protocol: &str, address: &[u8]) -> Option<&Device> {
        self.devices.values().find(|d| {
            d.system_id == system
                && d.identities
                    .iter()
                    .any(|i| i.protocol_id == protocol && i.address == address)
        })
    }
    pub fn update_config(
        &mut self,
        device_id: &str,
        config: Value,
        version: u64,
    ) -> DomainResult<()> {
        let d = self.devices.get_mut(device_id).ok_or("设备不存在")?;
        d.config = config;
        d.config_version = version;
        Ok(())
    }
    pub fn apply_config_result(
        &mut self,
        request: &ConfigRequest,
        results: &[u8],
    ) -> DomainResult<ConfigOutcome> {
        self.apply_config_at(request, results, ApplyTrigger::SuccessAck)
    }
    pub fn apply_config_at(
        &mut self,
        request: &ConfigRequest,
        results: &[u8],
        trigger: ApplyTrigger,
    ) -> DomainResult<ConfigOutcome> {
        if let Some(previous) = self.applied_requests.get(&request.request_key) {
            return Ok(previous.clone());
        }
        if request.items.len() != results.len() {
            return Err("0x22 条目数与请求不一致，配置未提交".into());
        }
        if request.items.len() > 255 {
            return Err("配置条目数超出一字节范围".into());
        }
        let mut outcome = ConfigOutcome {
            request_key: request.request_key.clone(),
            committed: false,
            deferred: trigger != request.effective_at || !request.batch_complete,
            items: results
                .iter()
                .enumerate()
                .map(|(index, code)| ConfigItemResult {
                    index,
                    code: *code,
                    confirmed: *code == 0xAA,
                    applied: false,
                    device_id: None,
                })
                .collect(),
        };
        if outcome.deferred
            || ((request.atomic || request.operation == ConfigOperation::Replace)
                && results.iter().any(|r| *r != 0xAA))
        {
            self.config_history.push(outcome.clone());
            return Ok(outcome);
        }
        // Commit an entire validated snapshot, never half of a malformed mapping.
        let mut candidate = self.clone();
        let target = if request.mode == ConfigMode::Parameters {
            let address = request
                .target_address
                .as_ref()
                .ok_or("单设备模式缺少目标地址")?;
            let existing = candidate
                .find_device(&request.system_id, &request.protocol_id, address)
                .map(|d| d.id.clone());
            if let Some(existing) = existing {
                Some(existing)
            } else if results.contains(&0xAA) && request.operation != ConfigOperation::Delete {
                Some(candidate.create_device(
                    &request.system_id,
                    "报文配置设备",
                    &request.protocol_id,
                    address.clone(),
                    "PACKET",
                )?)
            } else {
                None
            }
        } else {
            None
        };
        if let Some(target_id) = &target
            && request.operation == ConfigOperation::Replace
        {
            candidate.devices.get_mut(target_id).unwrap().config = json!({"parameters": {}});
        }
        for (index, item) in request.items.iter().enumerate() {
            if results[index] != 0xAA {
                continue;
            }
            let device_id = match request.mode {
                ConfigMode::Devices => {
                    let address = item.address.as_ref().ok_or("多设备条目缺少协议地址")?;
                    let existing = candidate
                        .find_device(&request.system_id, &request.protocol_id, address)
                        .map(|d| d.id.clone());
                    match existing {
                        Some(d) => d,
                        None if request.operation == ConfigOperation::Delete => continue,
                        None => candidate.create_device(
                            &request.system_id,
                            &format!("设备 {}", item.key),
                            &request.protocol_id,
                            address.clone(),
                            "PACKET",
                        )?,
                    }
                }
                ConfigMode::Parameters => match &target {
                    Some(t) => t.clone(),
                    None => continue,
                },
            };
            let d = candidate.devices.get_mut(&device_id).unwrap();
            if request.mode == ConfigMode::Parameters {
                if !d.config.is_object() {
                    d.config = json!({});
                }
                if !d.config.get("parameters").is_some_and(Value::is_object) {
                    d.config["parameters"] = json!({});
                }
                let params = d.config["parameters"].as_object_mut().unwrap();
                if request.operation == ConfigOperation::Delete {
                    params.remove(&item.key);
                } else {
                    params.insert(item.key.clone(), item.values.clone());
                }
            } else if request.operation == ConfigOperation::Delete {
                d.config = json!({});
            } else if request.operation == ConfigOperation::Merge {
                merge_json(&mut d.config, &item.values);
            } else {
                d.config = item.values.clone();
            }
            d.config_version = request.version;
            outcome.items[index].applied = true;
            outcome.items[index].device_id = Some(device_id);
        }
        outcome.committed = outcome.items.iter().any(|i| i.applied) || request.items.is_empty();
        candidate.config_history.push(outcome.clone());
        candidate
            .applied_requests
            .insert(request.request_key.clone(), outcome.clone());
        *self = candidate;
        Ok(outcome)
    }
    pub fn permit_session(
        &mut self,
        device_id: &str,
        peer_system_id: &str,
        wire_session_id: u16,
        valid_until_ms: u64,
        auth_required: bool,
    ) -> DomainResult<String> {
        if !self.devices.contains_key(device_id) {
            return Err("设备不存在".into());
        }
        if wire_session_id == 0 {
            return Err("许可会话号不能为零".into());
        }
        for s in self
            .sessions
            .values_mut()
            .filter(|s| s.device_id == device_id && s.peer_system_id == peer_system_id)
        {
            s.active = false;
            s.token.clear();
        }
        let session_id = id();
        self.sessions.insert(
            session_id.clone(),
            Session {
                id: session_id.clone(),
                device_id: device_id.into(),
                peer_system_id: peer_system_id.into(),
                wire_session_id,
                epoch: id(),
                valid_until_ms,
                auth_required,
                auth_expires_ms: None,
                registered: false,
                active: true,
                token: vec![],
            },
        );
        Ok(session_id)
    }
    pub fn issue_token(
        &mut self,
        session_id: &str,
        token: Vec<u8>,
        expires_ms: u64,
        now_ms: u64,
    ) -> DomainResult<()> {
        let s = self.sessions.get_mut(session_id).ok_or("会话不存在")?;
        if !s.active || now_ms >= s.valid_until_ms || expires_ms <= now_ms || token.is_empty() {
            return Err("REAUTH_REQUIRED".into());
        }
        s.token = token;
        s.auth_expires_ms = Some(expires_ms);
        Ok(())
    }
    pub fn session_token(&self, session_id: &str, now_ms: u64) -> DomainResult<&[u8]> {
        let s = self.sessions.get(session_id).ok_or("会话不存在")?;
        if !s.active
            || now_ms >= s.valid_until_ms
            || s.token.is_empty()
            || s.auth_expires_ms.is_none_or(|exp| now_ms >= exp)
        {
            return Err("REAUTH_REQUIRED".into());
        }
        Ok(&s.token)
    }
    pub fn complete_registration(
        &mut self,
        session_id: &str,
        result_code: u8,
        now_ms: u64,
    ) -> DomainResult<()> {
        let s = self.sessions.get(session_id).ok_or("会话不存在")?;
        if result_code != 0 {
            return Err(format!("注册未完成，结果 {result_code:02X}"));
        }
        if !s.active || now_ms >= s.valid_until_ms {
            return Err("注册会话已过期".into());
        }
        if s.auth_required {
            self.session_token(session_id, now_ms)?;
        }
        let device_id = s.device_id.clone();
        self.sessions.get_mut(session_id).unwrap().registered = true;
        let d = self.devices.get_mut(&device_id).ok_or("设备不存在")?;
        d.registration_valid = true;
        d.cleanup_until_ms = None;
        d.last_seen_ms = Some(now_ms);
        self.recompute_device(&device_id);
        Ok(())
    }
    pub fn record_heartbeat(&mut self, device_id: &str, now_ms: u64) -> DomainResult<()> {
        let d = self.devices.get_mut(device_id).ok_or("设备不存在")?;
        if d.registration_valid {
            d.last_seen_ms = Some(now_ms);
        } // Old heartbeats never restore ONLINE.
        Ok(())
    }
    pub fn expire_heartbeats(&mut self, now_ms: u64, timeout_ms: u64) -> Vec<CleanupAction> {
        let ids: Vec<_> = self
            .devices
            .values()
            .filter(|d| {
                d.registration_valid
                    && d.last_seen_ms
                        .is_some_and(|t| now_ms.saturating_sub(t) >= timeout_ms)
            })
            .map(|d| d.id.clone())
            .collect();
        ids.into_iter()
            .flat_map(|d| self.offline_device(&d, now_ms, None).unwrap_or_default())
            .collect()
    }
    pub(crate) fn recompute_device(&mut self, device_id: &str) {
        if let Some(d) = self.devices.get_mut(device_id) {
            d.state = if !d.registration_valid {
                DeviceState::Offline
            } else if !d.active_calls.is_empty() {
                DeviceState::Communicating
            } else {
                DeviceState::Online
            };
        }
    }
    pub fn offline_device(
        &mut self,
        device_id: &str,
        now_ms: u64,
        cleanup_window_ms: Option<u64>,
    ) -> DomainResult<Vec<CleanupAction>> {
        let d = self.devices.get_mut(device_id).ok_or("设备不存在")?;
        if !d.registration_valid && d.state == DeviceState::Offline {
            return Ok(vec![]);
        }
        d.registration_valid = false;
        d.state = DeviceState::Offline;
        d.cleanup_until_ms = cleanup_window_ms.map(|duration| now_ms.saturating_add(duration));
        for s in self
            .sessions
            .values_mut()
            .filter(|s| s.device_id == device_id)
        {
            s.active = false;
            s.token.clear();
        }
        let calls: Vec<_> = self
            .calls
            .values()
            .filter(|c| {
                c.participants.iter().any(|p| p == device_id) && c.state != CallState::Ended
            })
            .map(|c| c.id.clone())
            .collect();
        let mut actions = vec![];
        for call in calls {
            self.stop_participant(&call, device_id)?;
            actions.extend(self.begin_end_call(&call, now_ms)?);
        }
        Ok(actions)
    }
}
fn merge_json(target: &mut Value, source: &Value) {
    match (target, source) {
        (Value::Object(to), Value::Object(from)) => {
            for (key, value) in from {
                merge_json(to.entry(key.clone()).or_insert(Value::Null), value);
            }
        }
        (target, source) => *target = source.clone(),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub fn device(store: &mut DomainStore, addr: u8) -> String {
        store
            .create_device("s", "A", "musp", vec![0, 0, addr], "MANUAL")
            .unwrap()
    }
    pub fn registered(store: &mut DomainStore, addr: u8) -> String {
        let d = device(store, addr);
        let s = store
            .permit_session(&d, "control", addr as u16, 100_000, false)
            .unwrap();
        store.complete_registration(&s, 0, 1).unwrap();
        d
    }
    fn config(mode: ConfigMode, operation: ConfigOperation) -> ConfigRequest {
        ConfigRequest {
            request_key: "request-1".into(),
            system_id: "s".into(),
            protocol_id: "musp".into(),
            mode,
            operation,
            target_address: Some(vec![0, 0, 59]),
            version: 2,
            items: (0..5)
                .map(|i| ConfigItem {
                    address: Some(vec![0, 0, 59 + i]),
                    key: i.to_string(),
                    values: json!({"value":i}),
                })
                .collect(),
            effective_at: ApplyTrigger::SuccessAck,
            atomic: false,
            batch_complete: true,
        }
    }
    #[test]
    fn device_configuration_never_resets_registered_state() {
        let mut s = DomainStore::default();
        let d = registered(&mut s, 59);
        let r = s
            .apply_config_result(
                &config(ConfigMode::Devices, ConfigOperation::Replace),
                &[0xAA; 5],
            )
            .unwrap();
        assert_eq!(s.devices.len(), 5);
        assert!(r.committed);
        assert_eq!(s.devices[&d].state, DeviceState::Online);
        assert_eq!(
            s.devices
                .values()
                .filter(|d| d.state == DeviceState::Offline)
                .count(),
            4
        );
    }
    #[test]
    fn parameter_mode_partial_and_count_mismatch_do_not_create_five_devices() {
        let mut s = DomainStore::default();
        let req = config(ConfigMode::Parameters, ConfigOperation::Replace);
        assert!(s.apply_config_result(&req, &[0xAA; 4]).is_err());
        assert!(s.devices.is_empty());
        assert!(
            !s.apply_config_result(&req, &[0xAA, 0xAA, 0x01, 0xAA, 0xAA])
                .unwrap()
                .committed
        );
        assert!(s.devices.is_empty());
        let mut merge = req.clone();
        merge.operation = ConfigOperation::Merge;
        let out = s
            .apply_config_result(&merge, &[0xAA, 0xAA, 0x01, 0xAA, 0xAA])
            .unwrap();
        assert_eq!(out.items[2].code, 1);
        assert!(!out.items[2].applied);
        assert_eq!(s.devices.len(), 1);
        assert_eq!(
            s.devices.values().next().unwrap().config["parameters"]
                .as_object()
                .unwrap()
                .len(),
            4
        );
        s.apply_config_result(&merge, &[0xAA; 5]).unwrap();
        assert_eq!(s.devices.len(), 1);
    }
    #[test]
    fn token_is_scoped_expires_and_is_never_serialized() {
        let mut s = DomainStore::default();
        let d = device(&mut s, 59);
        let session = s.permit_session(&d, "control", 123, 10_000, true).unwrap();
        assert!(s.complete_registration(&session, 0, 1).is_err());
        s.issue_token(&session, vec![0xDE, 0xAD], 100, 1).unwrap();
        s.complete_registration(&session, 0, 2).unwrap();
        assert_eq!(s.session_token(&session, 99).unwrap(), &[0xDE, 0xAD]);
        assert!(s.session_token(&session, 100).is_err());
        assert!(!serde_json::to_string(&s).unwrap().contains("token"));
        s.offline_device(&d, 101, None).unwrap();
        s.record_heartbeat(&d, 102).unwrap();
        assert_eq!(s.devices[&d].state, DeviceState::Offline);
    }
}
