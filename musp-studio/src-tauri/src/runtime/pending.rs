//! A serialized registry. Register before send_to, then feed validated packets.
//! Holding one application mutex around register/match/tick arbitrates races.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "value", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SessionMatcher {
    #[default]
    Any,
    Exact(u16),
    Assigned,
}
impl SessionMatcher {
    fn matches(&self, session: Option<u16>) -> bool {
        match self {
            Self::Any => true,
            Self::Exact(s) => session == Some(*s),
            Self::Assigned => session.is_some_and(|s| s != 0),
        }
    }
    fn overlaps(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Exact(a), Self::Exact(b)) => a == b,
            (Self::Assigned, Self::Exact(s)) | (Self::Exact(s), Self::Assigned) => *s != 0,
            _ => true,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PendingStatus {
    #[default]
    Waiting,
    RetryScheduled,
    Matched,
    TimedOut,
    Cancelled,
    SendFailed,
}
impl PendingStatus {
    pub fn is_open(self) -> bool {
        matches!(self, Self::Waiting | Self::RetryScheduled)
    }
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PacketMeta {
    pub endpoint_id: String,
    pub protocol_version_id: String,
    pub remote: String,
    pub msg_code: u8,
    pub src_addr: Vec<u8>,
    pub dst_addr: Vec<u8>,
    pub transaction_id: Option<u32>,
    pub session_id: Option<u16>,
    pub call_id: Option<u16>,
    pub resource_id: Option<u16>,
    pub direction: Option<u8>,
    pub resource_version: Option<u16>,
    pub event_type: Option<u8>,
    #[serde(default)]
    pub business: BTreeMap<String, Value>,
    #[serde(default)]
    pub payload: Value,
    /// Must reflect schema, length and CRC validation, not merely decoding.
    pub valid: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PendingRequest {
    pub id: String,
    pub run_id: String,
    pub flow_run_id: String,
    pub wait_node_id: String,
    pub endpoint_id: String,
    pub protocol_version_id: String,
    pub logical_device_id: String,
    pub request_msg_code: u8,
    /// Passive request listeners learn the peer's transaction upon receipt.
    pub incoming_request: bool,
    pub expected_msg_codes: Vec<u8>,
    pub expected_remote: String,
    pub expect_src_addr: Vec<u8>,
    pub expect_dst_addr: Vec<u8>,
    pub transaction_id: Option<u32>,
    pub session_matcher: SessionMatcher,
    pub call_id: Option<u16>,
    pub resource_id: Option<u16>,
    pub direction: Option<u8>,
    pub resource_version: Option<u16>,
    pub event_type: Option<u8>,
    pub business: BTreeMap<String, Value>,
    pub timeout_ms: u64,
    pub retries_remaining: u32,
    pub retry_interval_ms: u64,
    pub exponential_backoff: bool,
    pub deadline_ms: u64,
    pub status: PendingStatus,
    pub attempts: u32,
    pub send_attempt_ids: Vec<String>,
    pub closed_ms: Option<u64>,
}
impl Default for PendingRequest {
    fn default() -> Self {
        Self {
            id: String::new(),
            run_id: String::new(),
            flow_run_id: String::new(),
            wait_node_id: String::new(),
            endpoint_id: String::new(),
            protocol_version_id: String::new(),
            logical_device_id: String::new(),
            request_msg_code: 0,
            incoming_request: false,
            expected_msg_codes: vec![],
            expected_remote: String::new(),
            expect_src_addr: vec![],
            expect_dst_addr: vec![],
            transaction_id: None,
            session_matcher: SessionMatcher::Any,
            call_id: None,
            resource_id: None,
            direction: None,
            resource_version: None,
            event_type: None,
            business: BTreeMap::new(),
            timeout_ms: 3000,
            retries_remaining: 0,
            retry_interval_ms: 0,
            exponential_backoff: false,
            deadline_ms: 0,
            status: PendingStatus::Waiting,
            attempts: 0,
            send_attempt_ids: vec![],
            closed_ms: None,
        }
    }
}
impl PendingRequest {
    fn matches(&self, p: &PacketMeta) -> bool {
        self.expected_msg_codes.contains(&p.msg_code)
            && self.endpoint_id == p.endpoint_id
            && self.protocol_version_id == p.protocol_version_id
            && self.expected_remote == p.remote
            && self.expect_src_addr == p.src_addr
            && self.expect_dst_addr == p.dst_addr
            && optional_matches(self.transaction_id, p.transaction_id)
            && self.session_matcher.matches(p.session_id)
            && optional_matches(self.call_id, p.call_id)
            && optional_matches(self.resource_id, p.resource_id)
            && optional_matches(self.direction, p.direction)
            && optional_matches(self.resource_version, p.resource_version)
            && optional_matches(self.event_type, p.event_type)
            && self
                .business
                .iter()
                .all(|(k, v)| p.business.get(k) == Some(v))
    }
    fn overlaps(&self, b: &Self) -> bool {
        self.expected_msg_codes
            .iter()
            .any(|c| b.expected_msg_codes.contains(c))
            && self.endpoint_id == b.endpoint_id
            && self.protocol_version_id == b.protocol_version_id
            && self.expected_remote == b.expected_remote
            && self.expect_src_addr == b.expect_src_addr
            && self.expect_dst_addr == b.expect_dst_addr
            && optional_overlaps(self.transaction_id, b.transaction_id)
            && self.session_matcher.overlaps(&b.session_matcher)
            && optional_overlaps(self.call_id, b.call_id)
            && optional_overlaps(self.resource_id, b.resource_id)
            && optional_overlaps(self.direction, b.direction)
            && optional_overlaps(self.resource_version, b.resource_version)
            && optional_overlaps(self.event_type, b.event_type)
            && self
                .business
                .iter()
                .all(|(k, v)| b.business.get(k).is_none_or(|other| other == v))
    }
}
fn optional_matches<T: PartialEq>(expected: Option<T>, actual: Option<T>) -> bool {
    expected.is_none() || expected == actual
}
fn optional_overlaps<T: PartialEq>(a: Option<T>, b: Option<T>) -> bool {
    a.is_none() || b.is_none() || a == b
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "type",
    content = "pendingIds",
    rename_all = "SCREAMING_SNAKE_CASE"
)]
pub enum MatchOutcome {
    Matched(String),
    Duplicate(String),
    Late(String),
    Conflict(Vec<String>),
    Unmatched,
    Malformed,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "type",
    content = "pendingId",
    rename_all = "SCREAMING_SNAKE_CASE"
)]
pub enum PendingEvent {
    Retry(String),
    TimedOut(String),
}
#[derive(Clone, Debug)]
pub struct PendingRegistry {
    pub requests: BTreeMap<String, PendingRequest>,
    by_code: BTreeMap<u8, BTreeSet<String>>,
    pub max_pending: usize,
    pub tombstone_ms: u64,
}
impl Default for PendingRegistry {
    fn default() -> Self {
        Self::new(10_000, 60_000)
    }
}
impl PendingRegistry {
    pub fn replace_open(&mut self, request: PendingRequest) -> Result<(), String> {
        let original = self.requests.get(&request.id).ok_or("等待项不存在")?;
        if !original.status.is_open() {
            return Err("等待项已关闭".into());
        }
        if request.status != original.status
            || request.deadline_ms != original.deadline_ms
            || request.attempts != original.attempts
            || request.flow_run_id != original.flow_run_id
            || request.closed_ms != original.closed_ms
        {
            return Err("细化匹配条件不得修改等待项生命周期".into());
        }
        if request.expected_msg_codes.is_empty() {
            return Err("期望响应信令码不能为空".into());
        }
        if self
            .requests
            .values()
            .any(|other| other.id != request.id && other.overlaps(&request))
        {
            return Err("编码后的关联键冲突".into());
        }
        for code in &original.expected_msg_codes {
            if let Some(ids) = self.by_code.get_mut(code) {
                ids.remove(&request.id);
            }
        }
        for code in &request.expected_msg_codes {
            self.by_code
                .entry(*code)
                .or_default()
                .insert(request.id.clone());
        }
        self.requests.insert(request.id.clone(), request);
        Ok(())
    }
    pub fn new(max_pending: usize, tombstone_ms: u64) -> Self {
        Self {
            requests: BTreeMap::new(),
            by_code: BTreeMap::new(),
            max_pending,
            tombstone_ms,
        }
    }
    pub fn register(&mut self, mut request: PendingRequest, now_ms: u64) -> Result<String, String> {
        self.prune(now_ms);
        if request.expected_msg_codes.is_empty()
            || request.timeout_ms == 0
            || request.endpoint_id.is_empty()
            || request.protocol_version_id.is_empty()
            || request.expected_remote.is_empty()
        {
            return Err("等待项缺少响应码/端点/协议版本/对端或超时".into());
        }
        if request.retries_remaining > 100 {
            return Err("节点重试次数超过100次上限".into());
        }
        if self
            .requests
            .values()
            .filter(|r| r.status.is_open())
            .count()
            >= self.max_pending
        {
            return Err("等待队列达到上限".into());
        }
        if let Some(old) = self.requests.values().find(|old| {
            old.overlaps(&request)
                && !(request.incoming_request
                    && old.incoming_request
                    && !old.status.is_open()
                    && (old.transaction_id.is_some() || old.status != PendingStatus::Matched))
        }) {
            return Err(format!(
                "关联键冲突或处于迟到保护窗口: {}，无法区分的请求必须串行",
                old.id
            ));
        }
        if request.id.is_empty() {
            request.id = uuid::Uuid::new_v4().to_string();
        }
        if self.requests.contains_key(&request.id) {
            return Err("重复 pending ID".into());
        }
        request.status = PendingStatus::Waiting;
        request.deadline_ms = now_ms.saturating_add(request.timeout_ms);
        request.attempts = 1;
        request.closed_ms = None;
        request
            .send_attempt_ids
            .push(uuid::Uuid::new_v4().to_string());
        let id = request.id.clone();
        for code in &request.expected_msg_codes {
            self.by_code.entry(*code).or_default().insert(id.clone());
        }
        self.requests.insert(id.clone(), request);
        Ok(id)
    }
    pub fn on_packet(&mut self, packet: &PacketMeta, now_ms: u64) -> MatchOutcome {
        if !packet.valid {
            return MatchOutcome::Malformed;
        }
        let matching: Vec<String> = self
            .by_code
            .get(&packet.msg_code)
            .into_iter()
            .flatten()
            .filter(|id| {
                self.requests.get(*id).is_some_and(|r| {
                    r.matches(packet)
                        && (!r.incoming_request || r.status.is_open() || r.transaction_id.is_some())
                })
            })
            .cloned()
            .collect();
        // An already consumed unsolicited request must not awaken the next
        // listener, even though that new listener has not learned a txn yet.
        if let Some(id) = matching.iter().find(|id| {
            let r = &self.requests[*id];
            r.incoming_request && !r.status.is_open() && r.transaction_id.is_some()
        }) {
            return if self.requests[id].status == PendingStatus::Matched {
                MatchOutcome::Duplicate(id.clone())
            } else {
                MatchOutcome::Late(id.clone())
            };
        }
        // A slow timer task must not extend the final deadline. Leave expiration
        // queued for tick so it can deliver the timeout event to the flow once.
        let open: Vec<_> = matching
            .iter()
            .filter(|id| {
                let r = &self.requests[*id];
                r.status.is_open()
                    && !(r.status == PendingStatus::Waiting
                        && r.retries_remaining == 0
                        && now_ms > r.deadline_ms)
            })
            .cloned()
            .collect();
        if open.len() > 1 {
            return MatchOutcome::Conflict(open);
        }
        if let Some(id) = open.first() {
            let r = self.requests.get_mut(id).unwrap();
            if r.incoming_request {
                r.transaction_id = packet.transaction_id;
                if let Some(session) = packet.session_id {
                    r.session_matcher = SessionMatcher::Exact(session);
                }
            }
            r.status = PendingStatus::Matched;
            r.closed_ms = Some(now_ms);
            return MatchOutcome::Matched(id.clone());
        }
        if let Some(id) = matching.first() {
            return if self.requests[id].status == PendingStatus::Matched {
                MatchOutcome::Duplicate(id.clone())
            } else {
                MatchOutcome::Late(id.clone())
            };
        }
        MatchOutcome::Unmatched
    }
    pub fn tick(&mut self, now_ms: u64) -> Vec<PendingEvent> {
        let mut events = vec![];
        for r in self
            .requests
            .values_mut()
            .filter(|r| r.status.is_open() && now_ms >= r.deadline_ms)
        {
            if r.status == PendingStatus::Waiting {
                if r.retries_remaining == 0 {
                    r.status = PendingStatus::TimedOut;
                    r.closed_ms = Some(now_ms);
                    events.push(PendingEvent::TimedOut(r.id.clone()));
                    continue;
                }
                r.retries_remaining -= 1;
                r.status = PendingStatus::RetryScheduled;
                let scale = if r.exponential_backoff {
                    1u64.checked_shl((r.attempts - 1).min(20))
                        .unwrap_or(u64::MAX)
                } else {
                    1
                };
                r.deadline_ms = now_ms.saturating_add(r.retry_interval_ms.saturating_mul(scale));
            }
            if r.status == PendingStatus::RetryScheduled && now_ms >= r.deadline_ms {
                r.status = PendingStatus::Waiting;
                r.attempts += 1;
                r.deadline_ms = now_ms.saturating_add(r.timeout_ms);
                r.send_attempt_ids.push(uuid::Uuid::new_v4().to_string());
                events.push(PendingEvent::Retry(r.id.clone()));
            }
        }
        self.prune(now_ms);
        events
    }
    pub fn send_failed(&mut self, pending_id: &str, now_ms: u64) -> bool {
        self.close(pending_id, PendingStatus::SendFailed, now_ms)
    }
    pub fn cancel(&mut self, pending_id: &str, now_ms: u64) -> bool {
        self.close(pending_id, PendingStatus::Cancelled, now_ms)
    }
    pub fn cancel_run(&mut self, flow_run_id: &str, now_ms: u64) -> Vec<String> {
        let ids: Vec<_> = self
            .requests
            .values()
            .filter(|r| r.flow_run_id == flow_run_id && r.status.is_open())
            .map(|r| r.id.clone())
            .collect();
        for id in &ids {
            self.cancel(id, now_ms);
        }
        ids
    }
    fn close(&mut self, pending_id: &str, status: PendingStatus, now_ms: u64) -> bool {
        if let Some(r) = self.requests.get_mut(pending_id)
            && r.status.is_open()
        {
            r.status = status;
            r.closed_ms = Some(now_ms);
            return true;
        }
        false
    }
    fn prune(&mut self, now_ms: u64) {
        let expired: Vec<_> = self
            .requests
            .values()
            .filter(|r| {
                r.closed_ms
                    .is_some_and(|t| now_ms.saturating_sub(t) > self.tombstone_ms)
            })
            .map(|r| r.id.clone())
            .collect();
        for id in expired {
            if let Some(r) = self.requests.remove(&id) {
                for code in r.expected_msg_codes {
                    if let Some(ids) = self.by_code.get_mut(&code) {
                        ids.remove(&id);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(device: u8, txn: Option<u32>) -> PendingRequest {
        PendingRequest {
            endpoint_id: "udp".into(),
            protocol_version_id: "v1".into(),
            expected_remote: "127.0.0.1:9000".into(),
            expected_msg_codes: vec![2],
            request_msg_code: 1,
            expect_src_addr: vec![0, 0, 1],
            expect_dst_addr: vec![0, 0, device],
            transaction_id: txn,
            session_matcher: SessionMatcher::Assigned,
            timeout_ms: 100,
            ..Default::default()
        }
    }
    fn packet(device: u8, txn: Option<u32>) -> PacketMeta {
        PacketMeta {
            endpoint_id: "udp".into(),
            protocol_version_id: "v1".into(),
            remote: "127.0.0.1:9000".into(),
            msg_code: 2,
            src_addr: vec![0, 0, 1],
            dst_addr: vec![0, 0, device],
            transaction_id: txn,
            session_id: Some(0x1234),
            valid: true,
            ..Default::default()
        }
    }
    #[test]
    fn reversed_responses_resume_only_the_correct_device() {
        let mut r = PendingRegistry::default();
        let a = r.register(request(59, Some(1)), 0).unwrap();
        let b = r.register(request(60, Some(2)), 0).unwrap();
        assert_eq!(
            r.on_packet(&packet(59, Some(2)), 1),
            MatchOutcome::Unmatched
        );
        let mut auth = packet(59, Some(1));
        auth.msg_code = 0x11;
        assert_eq!(r.on_packet(&auth, 1), MatchOutcome::Unmatched);
        assert_eq!(
            r.on_packet(&packet(60, Some(2)), 2),
            MatchOutcome::Matched(b)
        );
        assert_eq!(
            r.on_packet(&packet(59, Some(1)), 2),
            MatchOutcome::Matched(a.clone())
        );
        assert_eq!(
            r.on_packet(&packet(59, Some(1)), 3),
            MatchOutcome::Duplicate(a)
        );
    }
    #[test]
    fn no_sequence_parallel_is_rejected_and_late_packet_cannot_complete() {
        let mut r = PendingRegistry::default();
        let id = r.register(request(59, None), 0).unwrap();
        assert!(r.register(request(59, None), 1).is_err());
        assert_eq!(r.tick(100), vec![PendingEvent::TimedOut(id.clone())]);
        assert_eq!(r.on_packet(&packet(59, None), 101), MatchOutcome::Late(id));
        assert!(r.register(request(59, None), 102).is_err());
    }
    #[test]
    fn retry_uses_same_transaction_and_independent_interval() {
        let mut r = PendingRegistry::default();
        let mut req = request(59, Some(5));
        req.retries_remaining = 1;
        req.retry_interval_ms = 20;
        let id = r.register(req, 0).unwrap();
        assert!(r.tick(100).is_empty());
        assert!(r.tick(119).is_empty());
        assert_eq!(r.tick(120), vec![PendingEvent::Retry(id.clone())]);
        assert_eq!(r.requests[&id].transaction_id, Some(5));
        assert_eq!(r.requests[&id].attempts, 2);
        assert_eq!(r.tick(220), vec![PendingEvent::TimedOut(id)]);
    }
    #[test]
    fn resource_direction_and_version_are_mandatory_when_configured() {
        let mut r = PendingRegistry::default();
        let mut req = request(59, Some(1));
        req.expected_msg_codes = vec![0x77];
        req.call_id = Some(1);
        req.resource_id = Some(2);
        req.direction = Some(1);
        req.resource_version = Some(4);
        let id = r.register(req, 0).unwrap();
        let mut p = packet(59, Some(1));
        p.msg_code = 0x77;
        p.call_id = Some(1);
        p.resource_id = Some(2);
        p.direction = Some(2);
        p.resource_version = Some(4);
        assert_eq!(r.on_packet(&p, 1), MatchOutcome::Unmatched);
        p.direction = Some(1);
        p.resource_version = Some(3);
        assert_eq!(r.on_packet(&p, 1), MatchOutcome::Unmatched);
        p.resource_version = Some(4);
        assert_eq!(r.on_packet(&p, 1), MatchOutcome::Matched(id));
    }
    #[test]
    fn refining_response_code_updates_index_and_preserves_deadline() {
        let mut r = PendingRegistry::default();
        let id = r.register(request(59, Some(1)), 0).unwrap();
        let mut updated = r.requests[&id].clone();
        updated.expected_msg_codes = vec![4];
        r.replace_open(updated).unwrap();
        assert_eq!(
            r.on_packet(&packet(59, Some(1)), 1),
            MatchOutcome::Unmatched
        );
        let mut p = packet(59, Some(1));
        p.msg_code = 4;
        assert_eq!(r.on_packet(&p, 1), MatchOutcome::Matched(id));
    }
    #[test]
    fn every_transport_protocol_identity_and_business_key_is_checked() {
        let mut r = PendingRegistry::default();
        let mut req = request(59, Some(1));
        req.session_matcher = SessionMatcher::Exact(0x1234);
        req.business
            .insert("itemCount".into(), serde_json::json!(5));
        let id = r.register(req, 0).unwrap();
        let mut base = packet(59, Some(1));
        base.business
            .insert("itemCount".into(), serde_json::json!(5));
        let mut variants = vec![];
        let mut p = base.clone();
        p.endpoint_id = "other".into();
        variants.push(p);
        let mut p = base.clone();
        p.protocol_version_id = "v2".into();
        variants.push(p);
        let mut p = base.clone();
        p.remote = "127.0.0.2:9000".into();
        variants.push(p);
        let mut p = base.clone();
        p.src_addr[2] = 2;
        variants.push(p);
        let mut p = base.clone();
        p.dst_addr[2] = 60;
        variants.push(p);
        let mut p = base.clone();
        p.session_id = Some(0x1235);
        variants.push(p);
        let mut p = base.clone();
        p.business.insert("itemCount".into(), serde_json::json!(4));
        variants.push(p);
        for p in variants {
            assert_eq!(r.on_packet(&p, 1), MatchOutcome::Unmatched);
            assert_eq!(r.requests[&id].status, PendingStatus::Waiting);
        }
        let mut invalid = base.clone();
        invalid.valid = false;
        assert_eq!(r.on_packet(&invalid, 1), MatchOutcome::Malformed);
        assert_eq!(r.on_packet(&base, 2), MatchOutcome::Matched(id));
    }
    #[test]
    fn ambiguous_match_never_consumes_an_arbitrary_waiter() {
        let mut r = PendingRegistry::default();
        let a = r.register(request(59, Some(1)), 0).unwrap();
        let b = r.register(request(60, Some(1)), 0).unwrap();
        // Simulate conflicting restored/imported runtime state; match still fails closed.
        r.requests.get_mut(&b).unwrap().expect_dst_addr = vec![0, 0, 59];
        assert!(
            matches!(r.on_packet(&packet(59,Some(1)),1),MatchOutcome::Conflict(ids) if ids.len()==2)
        );
        assert_eq!(r.requests[&a].status, PendingStatus::Waiting);
        assert_eq!(r.requests[&b].status, PendingStatus::Waiting);
    }

    #[test]
    fn passive_listener_accepts_next_transaction_but_rejects_previous_duplicate() {
        let mut registry = PendingRegistry::default();
        let mut listener = request(59, None);
        listener.incoming_request = true;
        let first = registry.register(listener.clone(), 0).unwrap();
        assert_eq!(
            registry.on_packet(&packet(59, Some(101)), 1),
            MatchOutcome::Matched(first.clone())
        );
        let second = registry.register(listener.clone(), 2).unwrap();
        assert_eq!(
            registry.on_packet(&packet(59, Some(101)), 3),
            MatchOutcome::Duplicate(first)
        );
        assert!(registry.requests[&second].status.is_open());
        assert_eq!(
            registry.on_packet(&packet(59, Some(102)), 4),
            MatchOutcome::Matched(second)
        );
        let cancelled = registry.register(listener.clone(), 5).unwrap();
        registry.cancel(&cancelled, 6);
        let third = registry.register(listener, 7).unwrap();
        assert_eq!(
            registry.on_packet(&packet(59, Some(103)), 8),
            MatchOutcome::Matched(third)
        );
    }

    #[test]
    fn passive_listener_without_wire_transaction_keeps_late_guard() {
        let mut registry = PendingRegistry::default();
        let mut listener = request(59, None);
        listener.incoming_request = true;
        let first = registry.register(listener.clone(), 0).unwrap();
        assert_eq!(
            registry.on_packet(&packet(59, None), 1),
            MatchOutcome::Matched(first)
        );
        assert!(
            registry.register(listener, 2).is_err(),
            "without a wire transaction repeated payloads cannot identify a new request"
        );
    }
}
