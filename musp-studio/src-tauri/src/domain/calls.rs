use super::*;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Direction {
    Rx,
    Tx,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CallState {
    Requested,
    Paging,
    Permitted,
    Allocating,
    Ready,
    Active,
    Ending,
    Releasing,
    Ended,
    Failed,
    Cancelled,
    CleanupPending,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ResourceState {
    Reserved,
    Configuring,
    Applied,
    Active,
    Releasing,
    Released,
    Unknown,
    Failed,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StartMode {
    LocalReady,
    ControlNotify,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Call {
    pub id: String,
    pub project_id: String,
    pub scenario_run_id: String,
    pub wire_call_id: u16,
    pub epoch: String,
    pub kind: String,
    pub participants: Vec<String>,
    pub required: BTreeMap<String, Vec<Direction>>,
    pub start_mode: StartMode,
    pub state: CallState,
    pub active_devices: BTreeSet<String>,
    pub end_started: bool,
    pub created_ms: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ResourceKey {
    pub project_id: String,
    pub scenario_run_id: String,
    pub call_id: String,
    pub call_epoch: String,
    pub device_id: String,
    pub resource_id: u16,
    pub direction: Direction,
    pub version: u16,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    pub id: String,
    pub key: ResourceKey,
    pub pool_id: String,
    pub requested_bw: u64,
    pub reserved_bw: u64,
    pub assigned_bw: u64,
    pub released_bw: u64,
    pub unit: String,
    pub state: ResourceState,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourcePool {
    pub id: String,
    pub name: String,
    pub capacity: u64,
    pub available: u64,
    pub unit: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CleanupAction {
    NotifyEnd { call_id: String, device_id: String },
    ReleaseResource { resource: ResourceKey },
    UnknownResource { resource: ResourceKey },
}

impl DomainStore {
    #[allow(clippy::too_many_arguments)] // Explicit domain identity and lifetime inputs.
    pub fn create_call(
        &mut self,
        project_id: &str,
        scenario_run_id: &str,
        wire_call_id: u16,
        kind: &str,
        participants: Vec<String>,
        start_mode: StartMode,
        now_ms: u64,
    ) -> DomainResult<String> {
        if participants.is_empty() {
            return Err("通话必须有参与设备".into());
        }
        if participants.iter().collect::<BTreeSet<_>>().len() != participants.len() {
            return Err("重复通话参与设备".into());
        }
        for participant in &participants {
            let d = self.devices.get(participant).ok_or("参与设备不存在")?;
            if !d.registration_valid {
                return Err("离线设备不能发起新通话".into());
            }
            let count = self
                .calls
                .values()
                .filter(|c| {
                    c.participants.contains(participant)
                        && !c.end_started
                        && !matches!(
                            c.state,
                            CallState::Ended | CallState::Failed | CallState::Cancelled
                        )
                })
                .count();
            if count >= d.max_concurrent_calls {
                return Err("设备并发通话上限".into());
            }
        }
        // Never reuse a wire ID in the same run. New runs have distinct epochs.
        if self.calls.values().any(|c| {
            c.project_id == project_id
                && c.scenario_run_id == scenario_run_id
                && c.wire_call_id == wire_call_id
        }) {
            return Err("该运行中的 wire callId 已使用；请使用新编号或场景运行".into());
        }
        let call_id = id();
        let required = participants
            .iter()
            .map(|p| (p.clone(), vec![Direction::Rx, Direction::Tx]))
            .collect();
        self.calls.insert(
            call_id.clone(),
            Call {
                id: call_id.clone(),
                project_id: project_id.into(),
                scenario_run_id: scenario_run_id.into(),
                wire_call_id,
                epoch: id(),
                kind: kind.into(),
                participants,
                required,
                start_mode,
                state: CallState::Requested,
                active_devices: BTreeSet::new(),
                end_started: false,
                created_ms: now_ms,
            },
        );
        Ok(call_id)
    }
    pub fn permit_call(&mut self, call_id: &str) -> DomainResult<()> {
        let c = self.calls.get_mut(call_id).ok_or("通话不存在")?;
        if c.end_started || !matches!(c.state, CallState::Requested | CallState::Paging) {
            return Err("通话不能再次许可".into());
        }
        c.state = CallState::Permitted;
        Ok(())
    }
    pub fn add_pool(
        &mut self,
        pool_id: &str,
        name: &str,
        capacity: u64,
        unit: &str,
    ) -> DomainResult<()> {
        if self.pools.contains_key(pool_id) {
            return Err("资源池已存在".into());
        }
        self.pools.insert(
            pool_id.into(),
            ResourcePool {
                id: pool_id.into(),
                name: name.into(),
                capacity,
                available: capacity,
                unit: unit.into(),
            },
        );
        Ok(())
    }
    #[allow(clippy::too_many_arguments)] // Reservation requires the complete resource identity.
    pub fn reserve_resource(
        &mut self,
        call_id: &str,
        device_id: &str,
        resource_id: u16,
        direction: Direction,
        version: u16,
        pool_id: &str,
        requested_bw: u64,
        assigned_bw: u64,
    ) -> DomainResult<String> {
        let c = self.calls.get(call_id).ok_or("通话不存在")?;
        if c.end_started
            || !matches!(c.state, CallState::Permitted | CallState::Allocating)
            || !c.participants.iter().any(|p| p == device_id)
        {
            return Err("当前通话不允许分配资源".into());
        }
        let key = ResourceKey {
            project_id: c.project_id.clone(),
            scenario_run_id: c.scenario_run_id.clone(),
            call_id: call_id.into(),
            call_epoch: c.epoch.clone(),
            device_id: device_id.into(),
            resource_id,
            direction,
            version,
        };
        if let Some(r) = self.resources.values().find(|r| r.key == key) {
            if r.pool_id == pool_id
                && r.assigned_bw == assigned_bw
                && r.requested_bw == requested_bw
            {
                return Ok(r.id.clone());
            }
            return Err("重传资源参数与已预留值冲突".into());
        }
        if self.resources.values().any(|r| {
            r.key.call_id == call_id
                && r.key.device_id == device_id
                && r.key.direction == direction
                && r.state != ResourceState::Released
        }) {
            return Err("该通话设备方向已有资源".into());
        }
        let pool = self.pools.get_mut(pool_id).ok_or("资源池不存在")?;
        if assigned_bw == 0 || pool.available < assigned_bw {
            return Err("资源池带宽不足或分配为零".into());
        }
        pool.available -= assigned_bw;
        let resource_uuid = id();
        self.resources.insert(
            resource_uuid.clone(),
            Resource {
                id: resource_uuid.clone(),
                key,
                pool_id: pool_id.into(),
                requested_bw,
                reserved_bw: assigned_bw,
                assigned_bw,
                released_bw: 0,
                unit: pool.unit.clone(),
                state: ResourceState::Reserved,
            },
        );
        self.calls.get_mut(call_id).unwrap().state = CallState::Allocating;
        Ok(resource_uuid)
    }
    pub fn mark_configuring(&mut self, resource_id: &str) -> DomainResult<()> {
        let r = self.resources.get_mut(resource_id).ok_or("资源不存在")?;
        if !matches!(
            r.state,
            ResourceState::Reserved | ResourceState::Configuring
        ) {
            return Err("资源不处于可下发状态".into());
        }
        r.state = ResourceState::Configuring;
        Ok(())
    }
    pub fn acknowledge_resource(
        &mut self,
        key: &ResourceKey,
        result_code: u8,
        now_ms: u64,
    ) -> DomainResult<Vec<CleanupAction>> {
        let r = self
            .resources
            .values_mut()
            .find(|r| &r.key == key)
            .ok_or("资源关联字段不匹配")?;
        if matches!(r.state, ResourceState::Applied | ResourceState::Active) {
            return Ok(vec![]);
        }
        if r.state != ResourceState::Configuring {
            return Err("资源等待已关闭，响应迟到".into());
        }
        if result_code != 0 {
            r.state = ResourceState::Failed;
            return self.begin_end_call(&key.call_id, now_ms);
        }
        r.state = ResourceState::Applied;
        self.refresh_call_ready(&key.call_id)?;
        Ok(vec![])
    }
    fn refresh_call_ready(&mut self, call_id: &str) -> DomainResult<()> {
        let c = self.calls.get(call_id).ok_or("通话不存在")?.clone();
        if c.end_started {
            return Ok(());
        }
        let ready: Vec<String> = c
            .required
            .iter()
            .filter(|(device, directions)| {
                !directions.is_empty()
                    && directions.iter().all(|direction| {
                        self.resources.values().any(|r| {
                            r.key.call_id == call_id
                                && &r.key.device_id == *device
                                && r.key.direction == *direction
                                && matches!(r.state, ResourceState::Applied | ResourceState::Active)
                        })
                    })
            })
            .map(|(d, _)| d.clone())
            .collect();
        if ready.len() == c.required.len() {
            self.calls.get_mut(call_id).unwrap().state = CallState::Ready;
        }
        if c.start_mode == StartMode::LocalReady {
            for device_id in ready {
                self.activate_participant(call_id, &device_id)?;
            }
        }
        Ok(())
    }
    /// Invoke on a verified 0x75 event or acknowledged delivery, not UDP send success.
    pub fn control_ready(&mut self, call_id: &str, device_id: &str) -> DomainResult<()> {
        let c = self.calls.get(call_id).ok_or("通话不存在")?;
        if c.start_mode != StartMode::ControlNotify
            || !matches!(c.state, CallState::Ready | CallState::Active)
            || c.end_started
        {
            return Err("通话资源尚未整体就绪".into());
        }
        self.activate_participant(call_id, device_id)
    }
    fn activate_participant(&mut self, call_id: &str, device_id: &str) -> DomainResult<()> {
        let c = self.calls.get_mut(call_id).ok_or("通话不存在")?;
        if c.end_started || !c.participants.iter().any(|p| p == device_id) {
            Err("通话参与者不匹配")?;
        }
        let d = self.devices.get_mut(device_id).ok_or("设备不存在")?;
        if !d.registration_valid {
            return Err("离线设备不能被迟到通知激活".into());
        }
        c.active_devices.insert(device_id.into());
        d.active_calls.insert(call_id.into());
        if c.active_devices.len() == c.participants.len() {
            c.state = CallState::Active;
        }
        for r in self.resources.values_mut().filter(|r| {
            r.key.call_id == call_id
                && r.key.device_id == device_id
                && r.state == ResourceState::Applied
        }) {
            r.state = ResourceState::Active;
        }
        self.recompute_device(device_id);
        Ok(())
    }
    /// Repeated hangups return no duplicate release work. Each 0x73 can still
    /// receive its own acceptance response from the caller.
    pub fn begin_end_call(
        &mut self,
        call_id: &str,
        now_ms: u64,
    ) -> DomainResult<Vec<CleanupAction>> {
        let c = self.calls.get_mut(call_id).ok_or("通话不存在")?;
        if c.end_started {
            return Ok(vec![]);
        }
        c.end_started = true;
        c.state = CallState::Releasing;
        let participants = c.participants.clone();
        let mut actions = vec![];
        for device_id in participants {
            let d = self.devices.get(&device_id).ok_or("参与设备不存在")?;
            let reachable = d.registration_valid
                || d.cleanup_until_ms.is_some_and(|deadline| now_ms < deadline);
            if reachable {
                actions.push(CleanupAction::NotifyEnd {
                    call_id: call_id.into(),
                    device_id: device_id.clone(),
                });
            } else {
                self.stop_participant(call_id, &device_id)?;
            }
            for r in self.resources.values_mut().filter(|r| {
                r.key.call_id == call_id
                    && r.key.device_id == device_id
                    && r.state != ResourceState::Released
            }) {
                if reachable {
                    r.state = ResourceState::Releasing;
                    actions.push(CleanupAction::ReleaseResource {
                        resource: r.key.clone(),
                    });
                } else {
                    r.state = ResourceState::Unknown;
                    actions.push(CleanupAction::UnknownResource {
                        resource: r.key.clone(),
                    });
                }
            }
        }
        self.refresh_cleanup(call_id);
        Ok(actions)
    }
    /// Only apply once the end event has executed or been acknowledged.
    pub fn stop_participant(&mut self, call_id: &str, device_id: &str) -> DomainResult<()> {
        let c = self.calls.get_mut(call_id).ok_or("通话不存在")?;
        if !c.participants.iter().any(|p| p == device_id) {
            return Err("设备不属于通话".into());
        }
        c.active_devices.remove(device_id);
        if let Some(d) = self.devices.get_mut(device_id) {
            d.active_calls.remove(call_id);
        }
        self.recompute_device(device_id);
        self.refresh_cleanup(call_id);
        Ok(())
    }
    pub fn ack_release(&mut self, key: &ResourceKey, result_code: u8) -> DomainResult<bool> {
        let r = self
            .resources
            .values_mut()
            .find(|r| &r.key == key)
            .ok_or("释放确认资源/方向/版本/纪元不匹配")?;
        if r.state == ResourceState::Released {
            return Ok(false);
        }
        if r.state != ResourceState::Releasing {
            return Err("释放等待已关闭；迟到响应需独立状态核对".into());
        }
        if result_code == 0 {
            let pool = self.pools.get_mut(&r.pool_id).ok_or("资源池不存在")?;
            let available = pool
                .available
                .checked_add(r.reserved_bw)
                .filter(|v| *v <= pool.capacity)
                .ok_or("资源账务不一致")?;
            pool.available = available;
            r.state = ResourceState::Released;
            r.released_bw = r.reserved_bw;
        } else {
            r.state = ResourceState::Unknown;
        }
        self.refresh_cleanup(&key.call_id);
        Ok(result_code == 0)
    }
    pub fn resource_timeout(
        &mut self,
        key: &ResourceKey,
        now_ms: u64,
    ) -> DomainResult<Vec<CleanupAction>> {
        let r = self
            .resources
            .values_mut()
            .find(|r| &r.key == key)
            .ok_or("资源不存在")?;
        if r.state == ResourceState::Released {
            return Ok(vec![]);
        }
        let configuring = matches!(
            r.state,
            ResourceState::Reserved | ResourceState::Configuring
        );
        r.state = ResourceState::Unknown;
        if configuring {
            return self.begin_end_call(&key.call_id, now_ms);
        }
        self.refresh_cleanup(&key.call_id);
        Ok(vec![])
    }
    fn refresh_cleanup(&mut self, call_id: &str) {
        let resources: Vec<_> = self
            .resources
            .values()
            .filter(|r| r.key.call_id == call_id)
            .collect();
        if let Some(c) = self.calls.get_mut(call_id) {
            if !c.end_started {
                return;
            }
            c.state = if resources.iter().all(|r| r.state == ResourceState::Released)
                && c.active_devices.is_empty()
            {
                CallState::Ended
            } else if resources
                .iter()
                .any(|r| matches!(r.state, ResourceState::Unknown | ResourceState::Failed))
            {
                CallState::CleanupPending
            } else {
                CallState::Releasing
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::devices::tests::registered;
    fn active_call(s: &mut DomainStore, devices: Vec<String>, wire: u16) -> String {
        let c = s
            .create_call(
                "p",
                "run",
                wire,
                "video",
                devices.clone(),
                StartMode::LocalReady,
                0,
            )
            .unwrap();
        s.permit_call(&c).unwrap();
        for d in devices {
            for direction in [Direction::Rx, Direction::Tx] {
                let r = s
                    .reserve_resource(&c, &d, wire, direction, 1, "pool", 100, 60)
                    .unwrap();
                s.mark_configuring(&r).unwrap();
                let key = s.resources[&r].key.clone();
                s.acknowledge_resource(&key, 0, 1).unwrap();
            }
        }
        c
    }
    #[test]
    fn resource_timeout_never_returns_capacity() {
        let mut s = DomainStore::default();
        s.add_pool("pool", "Pool", 1000, "kbps").unwrap();
        let a = registered(&mut s, 59);
        let b = registered(&mut s, 60);
        let video = active_call(&mut s, vec![a.clone(), b.clone()], 1);
        let voice = active_call(&mut s, vec![a.clone()], 2);
        assert_eq!(s.pools["pool"].available, 640);
        let actions = s.begin_end_call(&video, 2).unwrap();
        assert_eq!(actions.len(), 6);
        assert!(s.begin_end_call(&video, 2).unwrap().is_empty());
        s.stop_participant(&video, &a).unwrap();
        s.stop_participant(&video, &b).unwrap();
        assert_eq!(s.devices[&a].state, DeviceState::Communicating);
        assert_eq!(s.devices[&b].state, DeviceState::Online);
        let keys: Vec<_> = s
            .resources
            .values()
            .filter(|r| r.key.call_id == video)
            .map(|r| r.key.clone())
            .collect();
        s.resource_timeout(&keys[0], 3).unwrap();
        assert!(s.ack_release(&keys[0], 0).is_err());
        for key in &keys[1..] {
            assert!(s.ack_release(key, 0).unwrap());
            assert!(!s.ack_release(key, 0).unwrap());
        }
        assert_eq!(s.pools["pool"].available, 820);
        assert_eq!(s.calls[&video].state, CallState::CleanupPending);
        assert_eq!(s.calls[&voice].state, CallState::Active);
    }
    #[test]
    fn offline_cleanup_only_releases_reachable_peer_resources() {
        let mut s = DomainStore::default();
        s.add_pool("pool", "Pool", 1000, "kbps").unwrap();
        let a = registered(&mut s, 59);
        let b = registered(&mut s, 60);
        let c = active_call(&mut s, vec![a.clone(), b.clone()], 1);
        let actions = s.offline_device(&a, 10, None).unwrap();
        assert_eq!(
            actions
                .iter()
                .filter(|a| matches!(a, CleanupAction::UnknownResource { .. }))
                .count(),
            2
        );
        assert_eq!(s.devices[&a].state, DeviceState::Offline);
        s.stop_participant(&c, &b).unwrap();
        assert_eq!(s.devices[&b].state, DeviceState::Online);
        assert_eq!(s.pools["pool"].available, 760);
        assert!(s.offline_device(&a, 11, None).unwrap().is_empty());
    }
    #[test]
    fn control_mode_waits_for_all_resources_and_explicit_ready() {
        let mut s = DomainStore::default();
        s.add_pool("pool", "Pool", 1000, "kbps").unwrap();
        let a = registered(&mut s, 59);
        let c = s
            .create_call(
                "p",
                "r",
                1,
                "voice",
                vec![a.clone()],
                StartMode::ControlNotify,
                0,
            )
            .unwrap();
        s.permit_call(&c).unwrap();
        for direction in [Direction::Rx, Direction::Tx] {
            assert!(s.control_ready(&c, &a).is_err());
            let r = s
                .reserve_resource(&c, &a, 1, direction, 1, "pool", 80, 60)
                .unwrap();
            s.mark_configuring(&r).unwrap();
            let key = s.resources[&r].key.clone();
            s.acknowledge_resource(&key, 0, 1).unwrap();
        }
        assert_eq!(s.devices[&a].state, DeviceState::Online);
        s.control_ready(&c, &a).unwrap();
        assert_eq!(s.devices[&a].state, DeviceState::Communicating);
    }
}
