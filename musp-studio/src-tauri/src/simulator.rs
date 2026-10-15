//! Explicitly enabled local simulation peer. Responses are encoded and sent through
//! the same actual UDP socket as any external device; no fabricated packet logs.
use crate::{
    network::now_ms,
    packet_business::n,
    protocol::{CompiledProtocol, DecodedPacket},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct FaultPolicy {
    pub drop_every: u32,
    pub delay_ms: u64,
    pub duplicate_replies: bool,
    pub corrupt_crc: bool,
}
#[derive(Default)]
pub struct PeerState {
    next_session: u16,
    sessions: HashMap<(String, u64), u16>,
    configs: HashMap<(String, u64), Value>,
    resources: HashMap<(String, u64, u64, u64, u8, u64), Value>,
    responses: HashMap<String, Vec<u8>>,
    pub fault: FaultPolicy,
    replies: u64,
}
impl PeerState {
    pub fn delivery(&mut self) -> Option<FaultPolicy> {
        self.replies += 1;
        if self.fault.drop_every != 0
            && self
                .replies
                .is_multiple_of(u64::from(self.fault.drop_every))
        {
            None
        } else {
            Some(self.fault.clone())
        }
    }
}
pub fn respond(
    schema: &CompiledProtocol,
    p: &DecodedPacket,
    state: &mut PeerState,
    remote: &str,
) -> Result<Option<Vec<u8>>, String> {
    let h = &p.values["header"];
    let b = &p.values["body"];
    let sender = n(h, "srcAddr");
    let target = n(h, "dstAddr");
    let mut session = n(h, "sessionId");
    let request_key = format!(
        "{remote}:{}:{}:{}:{}:{}",
        sender,
        target,
        p.message_code,
        n(h, "transactionId"),
        session
    );
    if let Some(bytes) = state.responses.get(&request_key) {
        return Ok(Some(bytes.clone()));
    }
    let (code, body) = match p.message_code {
        0x01 => {
            state.next_session = state.next_session.wrapping_add(1).max(1);
            session = u64::from(state.next_session);
            state
                .sessions
                .insert((remote.into(), sender), session as u16);
            (2, json!({"resultCode":0,"validSeconds":3600,"authMode":0}))
        }
        0x03 => {
            let valid = state.sessions.get(&(remote.into(), sender)) == Some(&(session as u16));
            (
                4,
                json!({"resultCode":if valid{0}else{1},"acceptedCapabilities":3}),
            )
        }
        0x21 => {
            let count = b["items"].as_array().ok_or("配置缺少items")?.len();
            state
                .configs
                .insert((remote.into(), n(b, "targetDeviceAddr")), b.clone());
            (
                0x22,
                json!({"results":(0..count).map(|_|json!({"result":170})).collect::<Vec<_>>()}),
            )
        }
        0x31 => (
            0x32,
            json!({"resultCode":0,"serverTimeSeconds":now_ms()/1000}),
        ),
        0x70 | 0x71 => {
            let direction = if p.message_code == 0x70 { 1 } else { 2 };
            let key = (
                format!("{remote}:{session}"),
                target,
                n(b, "callId"),
                n(b, "resourceId"),
                direction,
                n(b, "resourceVersion"),
            );
            state.resources.insert(key, b.clone());
            (
                0x72,
                json!({"callId":n(b,"callId"),"resourceId":n(b,"resourceId"),"direction":direction,"resultCode":0}),
            )
        }
        0x76 => {
            let key = (
                format!("{remote}:{session}"),
                target,
                n(b, "callId"),
                n(b, "resourceId"),
                n(b, "direction") as u8,
                n(b, "resourceVersion"),
            );
            let existed = state.resources.remove(&key).is_some();
            (
                0x77,
                json!({"callId":n(b,"callId"),"resourceId":n(b,"resourceId"),"direction":n(b,"direction"),"resourceVersion":n(b,"resourceVersion"),"resultCode":if existed{0}else{1}}),
            )
        }
        0x75 | 0x78 => (
            0x79,
            json!({"callId":n(b,"callId"),"eventType":if p.message_code==0x75{1}else{2},"resultCode":0}),
        ),
        0x62 => (
            0x63,
            json!({"callId":n(b,"callId"),"pagingResult":0,"acceptedMedia":n(b,"mediaType")}),
        ),
        _ => return Ok(None),
    };
    let bytes=schema.encode(code,&json!({"header":{"srcAddr":target,"dstAddr":sender,"flags":2,"sessionId":session,"transactionId":n(h,"transactionId")},"body":body})).map_err(|e|e.to_string())?;
    if state.responses.len() >= 4096 {
        state.responses.clear();
    }
    state.responses.insert(request_key, bytes.clone());
    Ok(Some(bytes))
}
