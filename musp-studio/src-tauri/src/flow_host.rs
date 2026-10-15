use crate::{
    application::Studio,
    network::{Datagram, Direction, PacketContext, now_ms},
    packet_business::{addr, apply_async, apply_matched, n},
    protocol::DecodedPacket,
    runtime::*,
};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

#[derive(Default)]
pub struct Execution {
    pub engine: FlowEngine,
    pub sent: HashMap<String, SentContext>,
    pub errors: Vec<String>,
    pub bindings: HashMap<String, WireBinding>,
    pub secrets: std::collections::HashSet<String>,
    pub call_requests: HashMap<String, CallRequest>,
    pub end_contexts: HashMap<String, EndContext>,
    pub cleanup_summaries: HashMap<String, (u8, u8)>,
}
#[derive(Clone)]
pub struct CallRequest {
    pub run_id: String,
    pub cancelled: bool,
    pub recorded_ms: u64,
}
#[derive(Clone)]
pub struct EndContext {
    pub reason: u8,
    pub release_reason: u8,
    pub initiator: Value,
}
pub struct SentContext {
    pub request: DecodedPacket,
    pub system: String,
    pub run_id: String,
    pub scenario_id: String,
}
#[derive(Clone)]
pub struct WireBinding {
    pub endpoint_id: String,
    pub protocol_id: String,
    pub remote: String,
    pub src: Value,
    pub dst: Value,
    pub session: u16,
    pub system_id: String,
    pub session_record_id: Option<String>,
}
/// Existing MUSP domain behavior is an explicit protocol identity opt-in.
pub(crate) fn is_musp_protocol(id: &str) -> bool {
    matches!(id, "musp-v08" | "musp-demo")
}
fn decoded_transaction(decoded: &DecodedPacket) -> Result<Option<u32>, String> {
    fn collect<'a>(fields: &'a [crate::protocol::DecodedField], found: &mut Vec<&'a Value>) {
        for field in fields {
            if field.path.starts_with("header.") && field.path.ends_with(".transactionId") {
                found.push(&field.decoded_value);
            }
            collect(&field.children, found);
        }
    }
    let mut found = Vec::new();
    collect(&decoded.fields, &mut found);
    match found.as_slice() {
        [] => Ok(None),
        [value] => crate::packet_business::number(value)
            .and_then(|number| u32::try_from(number).ok())
            .map(Some)
            .ok_or_else(|| "头部事务号超出32位匹配范围或不是整数".into()),
        _ => Err("存在多个头部transactionId字段，无法唯一推导关联事务".into()),
    }
}
pub fn meta(packet: &Datagram, decoded: &DecodedPacket) -> PacketMeta {
    let h = &decoded.values["header"];
    let b = &decoded.values["body"];
    let transaction = decoded_transaction(decoded);
    PacketMeta {
        endpoint_id: packet.endpoint_id.clone(),
        protocol_version_id: packet.protocol_id.clone(),
        remote: packet.remote.clone(),
        msg_code: decoded.message_code,
        src_addr: addr(&h["srcAddr"]),
        dst_addr: addr(&h["dstAddr"]),
        transaction_id: transaction.as_ref().ok().copied().flatten(),
        session_id: h.get("sessionId").map(|_| n(h, "sessionId") as u16),
        call_id: b.get("callId").map(|_| n(b, "callId") as u16),
        resource_id: b.get("resourceId").map(|_| n(b, "resourceId") as u16),
        direction: b.get("direction").map(|_| n(b, "direction") as u8),
        resource_version: b
            .get("resourceVersion")
            .map(|_| n(b, "resourceVersion") as u16),
        event_type: b.get("eventType").map(|_| n(b, "eventType") as u8),
        business: b
            .as_object()
            .map(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default(),
        payload: decoded.values.clone(),
        valid: transaction.is_ok(),
    }
}

impl Studio {
    pub fn start_scheduler(self: &Arc<Self>) {
        let app = self.clone();
        tauri::async_runtime::spawn(async move {
            let mut tick = tokio::time::interval(std::time::Duration::from_millis(20));
            loop {
                tokio::select! {_=app.shutdown.cancelled()=>break,_=tick.tick()=>{}}
                let actions = {
                    let _gate = app.project_gate.read().unwrap();
                    app.execution.lock().unwrap().engine.poll(now_ms())
                };
                app.expire_business_waits();
                app.maintain_business();
                if let Err(error) = app.publish_cleanup_results() {
                    tracing::error!(%error,"结束结果汇总失败");
                }
                if !actions.is_empty() {
                    app.execute_actions(actions).await;
                }
            }
        });
    }
    pub fn route_packet(
        self: &Arc<Self>,
        packet: &Datagram,
        decoded: &DecodedPacket,
    ) -> Result<Value, String> {
        if packet.direction != Direction::Rx {
            return Ok(Value::Null);
        }
        let musp = is_musp_protocol(&decoded.protocol_id);
        if musp && matches!(decoded.message_code, 0x73 | 0x7B) {
            return Ok(
                json!({"match":{"type":"ASYNC"},"business":self.handle_call_request(packet,decoded)
                .unwrap_or_else(|error|json!({"error":error}))}),
            );
        }
        let incoming_key = if musp && matches!(decoded.message_code, 0x60 | 0x65) {
            let peer = format!(
                "{}|{}|{}",
                packet.endpoint_id, packet.protocol_id, packet.remote
            );
            let domain = self.domain.lock().unwrap();
            crate::packet_business::registered_sender(&domain, &peer, decoded)
                .ok()
                .map(|(_, session)| {
                    Self::call_request_key(
                        packet,
                        decoded,
                        &session.id,
                        n(&decoded.values["header"], "transactionId") as u32,
                    )
                })
        } else {
            None
        };
        let meta = meta(packet, decoded);
        let (outcome, mut actions, context, packet_context) = {
            let mut x = self.execution.lock().unwrap();
            let (outcome, actions) = x.engine.on_packet(&meta, now_ms());
            if let (Some(key), MatchOutcome::Matched(id)) = (&incoming_key, &outcome) {
                let run_id = x.engine.pending.requests[id].run_id.clone();
                x.call_requests.entry(key.clone()).or_insert(CallRequest {
                    run_id,
                    cancelled: false,
                    recorded_ms: now_ms(),
                });
            }
            let context = if let MatchOutcome::Matched(id) = &outcome {
                x.sent.remove(id)
            } else {
                None
            };
            let packet_context = if let MatchOutcome::Matched(id) = &outcome {
                x.engine
                    .pending
                    .requests
                    .get(id)
                    .map(|pending| PacketContext {
                        run_id: pending.run_id.clone(),
                        node_id: pending.wait_node_id.clone(),
                    })
            } else {
                None
            };
            (outcome, actions, context, packet_context)
        };
        let run_id = packet_context.as_ref().map(|c| c.run_id.clone());
        let peer = format!(
            "{}|{}|{}",
            packet.endpoint_id, packet.protocol_id, packet.remote
        );
        let result = if !musp {
            Ok(Value::Null)
        } else if let Some(context) = context {
            let mut domain = self.domain.lock().unwrap();
            let mut candidate = domain.clone();
            let result = apply_matched(
                &mut candidate,
                &context.system,
                &peer,
                &context.scenario_id,
                &context.request,
                decoded,
            );
            if result.is_ok() {
                self.db
                    .save_entity(&self.project(), "domain", "current", &json!(candidate))?;
                *domain = candidate;
            }
            result
        } else if matches!(outcome, MatchOutcome::Unmatched | MatchOutcome::Matched(_)) {
            let system = {
                let d = self.domain.lock().unwrap();
                let address = addr(
                    &decoded.values["header"][if decoded.message_code == 0x11 {
                        "dstAddr"
                    } else {
                        "srcAddr"
                    }],
                );
                let systems: Vec<_> = d
                    .sessions
                    .values()
                    .filter(|s| {
                        s.active
                            && s.peer_system_id == peer
                            && s.wire_session_id == n(&decoded.values["header"], "sessionId") as u16
                    })
                    .filter_map(|s| d.devices.get(&s.device_id))
                    .filter(|dev| dev.identities.iter().any(|i| i.address == address))
                    .map(|dev| dev.system_id.clone())
                    .collect();
                if systems.len() == 1 {
                    systems[0].clone()
                } else {
                    String::new()
                }
            };
            let mut domain = self.domain.lock().unwrap();
            let result = apply_async(&mut domain, &system, &peer, decoded);
            if result.as_ref().is_ok_and(|v| !v.is_null()) {
                self.db
                    .save_entity(&self.project(), "domain", "current", &json!(*domain))?;
            }
            result
        } else {
            Ok(Value::Null)
        };
        if let Err(e) = &result {
            let mut x = self.execution.lock().unwrap();
            x.errors.push(e.clone());
            if x.errors.len() > 100 {
                x.errors.remove(0);
            }
            if let Some(run_id) = run_id {
                actions = x.engine.fail_run(&run_id, e.clone(), now_ms());
            }
        }
        if let Ok(value) = &result
            && let Some(cleanup) = value.get("cleanup")
        {
            let actions = serde_json::from_value(cleanup.clone()).map_err(|e| e.to_string())?;
            self.schedule_cleanup(actions)?;
        }
        if decoded.message_code == 0x11 && result.as_ref().is_ok_and(|v| v["authReady"] == true) {
            let h = decoded.values["header"].clone();
            let bytes=self.schema(&packet.protocol_id)?.encode(0x12,&json!({"srcAddr":h["dstAddr"],"dstAddr":h["srcAddr"],"transactionId":h["transactionId"],"sessionId":h["sessionId"],"flags":2,"resultCode":0})).map_err(|e|e.to_string())?;
            let app = self.clone();
            let endpoint = packet.endpoint_id.clone();
            let remote = packet.remote.clone();
            let generation = packet.generation.clone();
            let protocol = packet.protocol_id.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = app
                    .udp
                    .send_generation(&endpoint, &remote, &bytes, Some(&generation))
                    .await
                {
                    tracing::error!(%error,"认证接收确认发送失败");
                    return;
                }
                // Publish retained readiness only after the real AUTH_ACK send.
                // Events are retained by the engine if WAIT_EVENT is reached later.
                let auth_actions = {
                    let mut x = app.execution.lock().unwrap();
                    let ids: Vec<_> = x
                        .engine
                        .runs
                        .values()
                        .filter(|r| {
                            r.variables["endpointId"] == endpoint
                                && r.variables["protocolVersionId"] == protocol
                                && r.variables["remote"] == remote
                                && n(&r.variables, "sessionId") == n(&h, "sessionId")
                                && addr(
                                    r.variables
                                        .get("srcAddr")
                                        .unwrap_or(&r.variables["deviceAddress"]),
                                ) == addr(&h["dstAddr"])
                        })
                        .map(|r| r.id.clone())
                        .collect();
                    let mut auth_actions = Vec::new();
                    for id in ids {
                        auth_actions.extend(x.engine.emit_event(
                            &id,
                            "AUTH_READY",
                            json!({"sessionId":h["sessionId"]}),
                            now_ms(),
                        ));
                    }
                    auth_actions
                };
                app.execute_actions(auth_actions).await;
            });
        }
        if !actions.is_empty() {
            let app = self.clone();
            tauri::async_runtime::spawn(async move {
                app.execute_actions(actions).await;
            });
        }
        Ok(
            json!({"match":outcome,"business":result.unwrap_or_else(|e|json!({"error":e})),"runId":packet_context.as_ref().map(|c|&c.run_id),"nodeId":packet_context.as_ref().map(|c|&c.node_id)}),
        )
    }
    pub async fn execute_actions(self: &Arc<Self>, actions: Vec<FlowAction>) {
        let mut queue: VecDeque<_> = actions.into();
        while let Some(action) = queue.pop_front() {
            match action {
                FlowAction::Send(send) => {
                    if self
                        .execution
                        .lock()
                        .unwrap()
                        .engine
                        .runs
                        .get(&send.run_id)
                        .is_none_or(|r| {
                            matches!(
                                r.status,
                                FlowStatus::Cancelled | FlowStatus::Failed | FlowStatus::Completed
                            )
                        })
                    {
                        continue;
                    }
                    let result = self.execute_send(&send).await;
                    queue.extend(self.execution.lock().unwrap().engine.complete_action(
                        &send.action_id,
                        result,
                        now_ms(),
                    ));
                }
                FlowAction::DomainUpdate {
                    action_id,
                    run_id,
                    mut data,
                    ..
                } => {
                    let _gate = self.project_gate.read().unwrap();
                    if self
                        .execution
                        .lock()
                        .unwrap()
                        .engine
                        .runs
                        .get(&run_id)
                        .is_none_or(|r| {
                            matches!(
                                r.status,
                                FlowStatus::Cancelled | FlowStatus::Failed | FlowStatus::Completed
                            )
                        })
                    {
                        continue;
                    }
                    if data["operation"] == "session_token" {
                        let x = self.execution.lock().unwrap();
                        if let Some(run) = x.engine.runs.get(&run_id) {
                            for key in ["endpointId", "protocolVersionId", "remote"] {
                                data[key] = run.variables[key].clone();
                            }
                        }
                    }
                    let result = {
                        // Serialize cancellation with the final state check and
                        // commit, so a queued create/reserve action cannot run
                        // after its original request has been cancelled.
                        let execution = self.execution.lock().unwrap();
                        if execution.engine.runs.get(&run_id).is_none_or(|r| {
                            matches!(
                                r.status,
                                FlowStatus::Cancelled | FlowStatus::Failed | FlowStatus::Completed
                            )
                        }) {
                            continue;
                        }
                        let mut domain = self.domain.lock().unwrap();
                        let mut candidate = domain.clone();
                        match crate::business::update_domain(
                            &mut candidate,
                            &self.project(),
                            &run_id,
                            &data,
                        ) {
                            Ok(value) => match self.db.save_entity(
                                &self.project(),
                                "domain",
                                "current",
                                &json!(candidate),
                            ) {
                                Ok(()) => {
                                    *domain = candidate;
                                    Ok(value)
                                }
                                Err(e) => Err(e),
                            },
                            Err(e) => Err(e),
                        }
                    };
                    if let Ok(value) = &result
                        && let Some(token) = value["token"].as_str()
                    {
                        self.execution
                            .lock()
                            .unwrap()
                            .secrets
                            .insert(token.replace(' ', "").to_ascii_uppercase());
                    }
                    queue.extend(self.execution.lock().unwrap().engine.complete_action(
                        &action_id,
                        result,
                        now_ms(),
                    ));
                }
                FlowAction::Event { .. } => {}
                FlowAction::Finished { run_id, status, .. } => {
                    let _gate = self.project_gate.read().unwrap();
                    {
                        let x = self.execution.lock().unwrap();
                        if let Some(run) = x.engine.runs.get(&run_id) {
                            let mut value = json!(run);
                            redact_known(&mut value, &x.secrets);
                            let _ = self
                                .db
                                .save_entity(&self.project(), "runs", &run_id, &value);
                        }
                    }
                    if matches!(
                        status,
                        FlowStatus::Failed | FlowStatus::Cancelled | FlowStatus::Interrupted
                    ) {
                        let cleanup = {
                            let mut d = self.domain.lock().unwrap();
                            let calls: Vec<_> = d
                                .calls
                                .values()
                                .filter(|c| c.scenario_run_id == run_id && !c.end_started)
                                .map(|c| c.id.clone())
                                .collect();
                            let mut actions = vec![];
                            for call in calls {
                                actions
                                    .extend(d.begin_end_call(&call, now_ms()).unwrap_or_default());
                            }
                            let _ = self.db.save_entity(
                                &self.project(),
                                "domain",
                                "current",
                                &json!(*d),
                            );
                            actions
                        };
                        if let Err(error) = self.schedule_cleanup(cleanup) {
                            tracing::error!(%error,"流程结束补偿失败");
                        }
                    }
                }
            }
        }
    }
    async fn execute_send(&self, send: &SendAction) -> Result<Value, String> {
        let endpoint = self
            .udp
            .list()
            .await
            .into_iter()
            .find(|e| e.id == send.endpoint_id && e.running)
            .ok_or("发送端点未启动")?;
        if endpoint.protocol_id != send.protocol_version_id {
            return Err("流程协议版本与端点绑定版本不一致".into());
        }
        let schema = self.schema(&send.protocol_version_id)?;
        let mut values = send.values.clone();
        if send.automatic_transaction {
            let transaction = values
                .as_object_mut()
                .and_then(|v| v.remove("transactionId"));
            if schema
                .messages
                .iter()
                .find(|m| m.code == send.msg_code)
                .is_some_and(|message| {
                    crate::flow_preview::needs_header_transaction(&schema, message, &values)
                })
                && let Some(transaction) = transaction
            {
                values["transactionId"] = transaction;
            }
        }
        let bytes = schema
            .encode(send.msg_code, &values)
            .map_err(|e| e.to_string())?;
        let decoded = schema.decode(&bytes).map_err(|e| e.to_string())?;
        let musp = is_musp_protocol(&schema.id);
        self.remember_secrets(&decoded);
        if musp && n(&decoded.values["header"], "flags") & 1 != 0 {
            let h = &decoded.values["header"];
            let peer = format!(
                "{}|{}|{}",
                send.endpoint_id, send.protocol_version_id, send.remote
            );
            let d = self.domain.lock().unwrap();
            let sessions: Vec<_> = d
                .sessions
                .values()
                .filter(|s| {
                    s.active
                        && s.peer_system_id == peer
                        && s.wire_session_id == n(h, "sessionId") as u16
                        && d.devices.get(&s.device_id).is_some_and(|dev| {
                            dev.identities.iter().any(|i| {
                                i.address == addr(&h["srcAddr"]) || i.address == addr(&h["dstAddr"])
                            })
                        })
                })
                .collect();
            if sessions.len() != 1 {
                return Err("REAUTH_REQUIRED: 认证信息不属于本端点/协议/对端/会话".into());
            }
            let token = d.session_token(&sessions[0].id, now_ms())?;
            let spans = decoded.secret_spans();
            if !spans.iter().any(|(start, len)| {
                *start >= 23
                    && *start < bytes[3] as usize
                    && bytes.get(*start..start + len) == Some(token)
            }) {
                return Err("REAUTH_REQUIRED: 认证信息不匹配".into());
            }
        }
        if let Some(id) = &send.pending_id {
            let (system, scenario_id) = {
                let x = self.execution.lock().unwrap();
                let vars = &x
                    .engine
                    .runs
                    .get(&send.run_id)
                    .ok_or("运行不存在")?
                    .variables;
                (
                    vars["systemId"]
                        .as_str()
                        .map(str::to_owned)
                        .or_else(|| {
                            self.db
                                .entities(&self.project(), "systems")
                                .ok()?
                                .first()?
                                .get("id")?
                                .as_str()
                                .map(str::to_owned)
                        })
                        .ok_or("流程没有系统绑定")?,
                    vars["scenarioRunId"]
                        .as_str()
                        .unwrap_or(&send.run_id)
                        .to_string(),
                )
            };
            let binding_key = if musp && [0x70, 0x71].contains(&send.msg_code) {
                let d = self.domain.lock().unwrap();
                let device =
                    d.find_device(&system, "MUSP", &addr(&decoded.values["header"]["dstAddr"]));
                device.and_then(|device| {
                    d.calls
                        .values()
                        .find(|c| {
                            c.scenario_run_id == scenario_id
                                && c.wire_call_id == n(&decoded.values["body"], "callId") as u16
                                && c.participants.contains(&device.id)
                        })
                        .map(|c| {
                            let peer = format!(
                                "{}|{}|{}",
                                send.endpoint_id, send.protocol_version_id, send.remote
                            );
                            let sessions: Vec<_> = d
                                .sessions
                                .values()
                                .filter(|s| {
                                    s.device_id == device.id
                                        && s.active
                                        && s.registered
                                        && s.peer_system_id == peer
                                        && s.wire_session_id
                                            == n(&decoded.values["header"], "sessionId") as u16
                                })
                                .collect();
                            (
                                format!("{}:{}", c.id, device.id),
                                if sessions.len() == 1 {
                                    Some(sessions[0].id.clone())
                                } else {
                                    None
                                },
                            )
                        })
                })
            } else {
                None
            };
            let mut x = self.execution.lock().unwrap();
            let mut req = x
                .engine
                .pending
                .requests
                .get(id)
                .ok_or("等待项不存在")?
                .clone();
            if !req.status.is_open() {
                return Err("等待项已停止".into());
            }
            let h = &decoded.values["header"];
            let b = &decoded.values["body"];
            req.expect_src_addr = addr(&h["dstAddr"]);
            req.expect_dst_addr = addr(&h["srcAddr"]);
            if !send.explicit_transaction_matcher && !send.no_transaction_matcher {
                req.transaction_id = decoded_transaction(&decoded)?;
            }
            if !send.explicit_session_matcher {
                req.session_matcher = if musp && send.msg_code == 1 {
                    SessionMatcher::Assigned
                } else {
                    h.get("sessionId")
                        .map(|_| SessionMatcher::Exact(n(h, "sessionId") as u16))
                        .unwrap_or(SessionMatcher::Any)
                };
            }
            if musp && send.msg_code == 0x21 {
                req.business
                    .insert("itemCount".into(), b["itemCount"].clone());
            }
            if musp && [0x70, 0x71, 0x76, 0x75, 0x78].contains(&send.msg_code) {
                req.call_id = Some(n(b, "callId") as u16);
            }
            if musp && [0x70, 0x71, 0x76].contains(&send.msg_code) {
                req.resource_id = Some(n(b, "resourceId") as u16);
                req.direction = Some(if send.msg_code == 0x70 {
                    1
                } else if send.msg_code == 0x71 {
                    2
                } else {
                    n(b, "direction") as u8
                });
            }
            if musp && send.msg_code == 0x76 {
                req.resource_version = Some(n(b, "resourceVersion") as u16);
            }
            if musp && [0x75, 0x78].contains(&send.msg_code) {
                req.event_type = Some(if send.msg_code == 0x75 { 1 } else { 2 });
            }
            // Re-register the refined encoded key before send_to. This also checks conflicts.
            x.engine.pending.replace_open(req)?;
            if let Some((key, session_record_id)) = binding_key {
                x.bindings.insert(
                    key,
                    WireBinding {
                        endpoint_id: send.endpoint_id.clone(),
                        protocol_id: send.protocol_version_id.clone(),
                        remote: send.remote.clone(),
                        src: json!(crate::application::format_hex(&addr(&h["srcAddr"]))),
                        dst: json!(crate::application::format_hex(&addr(&h["dstAddr"]))),
                        session: n(h, "sessionId") as u16,
                        system_id: system.clone(),
                        session_record_id,
                    },
                );
            }
            x.sent.insert(
                id.clone(),
                SentContext {
                    request: decoded,
                    system,
                    run_id: send.run_id.clone(),
                    scenario_id,
                },
            );
        }
        let sent = self
            .udp
            .send_with_context(
                &send.endpoint_id,
                &send.remote,
                &bytes,
                Some(&endpoint.generation),
                Some(PacketContext {
                    run_id: send.run_id.clone(),
                    node_id: send.node_id.clone(),
                }),
            )
            .await?;
        Ok(json!({"sent":sent}))
    }
    pub fn run_views(&self) -> (Vec<Value>, Vec<Value>) {
        let x = self.execution.lock().unwrap();
        let runs = x
            .engine
            .runs
            .values()
            .map(|r| {
                let mut v = json!(r);
                v["flowId"] = json!(r.definition_id);
                v["currentNode"] = json!(r.trace.last().map(|t| t.node_id.clone()));
                redact_known(&mut v, &x.secrets);
                v
            })
            .collect();
        let pending = x
            .engine
            .pending
            .requests
            .values()
            .filter(|r| r.status.is_open())
            .map(|r| {
                let mut v = json!(r);
                v["deadline"] = json!(r.deadline_ms);
                redact_known(&mut v, &x.secrets);
                v
            })
            .collect();
        (runs, pending)
    }
    pub fn remember_secrets(&self, packet: &DecodedPacket) {
        fn collect(
            fields: &[crate::protocol::DecodedField],
            out: &mut std::collections::HashSet<String>,
        ) {
            for field in fields {
                if field.secret {
                    out.insert(field.raw_hex.replace(' ', "").to_ascii_uppercase());
                    if let Some(value) = field.decoded_value.as_str() {
                        out.insert(value.replace(' ', "").to_ascii_uppercase());
                    }
                }
                collect(&field.children, out);
            }
        }
        let mut x = self.execution.lock().unwrap();
        collect(&packet.fields, &mut x.secrets);
    }
    fn expire_business_waits(&self) {
        let _gate = self.project_gate.read().unwrap();
        let expired = {
            let mut x = self.execution.lock().unwrap();
            let ids: Vec<_> = x
                .sent
                .keys()
                .filter(|id| {
                    x.engine.pending.requests.get(*id).is_none_or(|p| {
                        matches!(
                            p.status,
                            PendingStatus::TimedOut
                                | PendingStatus::Cancelled
                                | PendingStatus::SendFailed
                        )
                    })
                })
                .cloned()
                .collect();
            ids.into_iter()
                .filter_map(|id| x.sent.remove(&id))
                .collect::<Vec<_>>()
        };
        for context in expired {
            if ![0x70, 0x71, 0x76].contains(&context.request.message_code) {
                continue;
            }
            let cleanup = {
                let mut d = self.domain.lock().unwrap();
                let b = &context.request.values["body"];
                let direction = if context.request.message_code == 0x70
                    || context.request.message_code == 0x76 && n(b, "direction") == 1
                {
                    crate::domain::Direction::Rx
                } else {
                    crate::domain::Direction::Tx
                };
                let key = d
                    .resources
                    .values()
                    .find(|r| {
                        // A cancelled configuration wait can outlive the start
                        // of release. It cannot invalidate that newer operation.
                        (context.request.message_code == 0x76
                            || matches!(
                                r.state,
                                crate::domain::ResourceState::Reserved
                                    | crate::domain::ResourceState::Configuring
                            ))
                            && r.key.scenario_run_id == context.scenario_id
                            && r.key.resource_id == n(b, "resourceId") as u16
                            && r.key.version == n(b, "resourceVersion") as u16
                            && r.key.direction == direction
                            && d.calls
                                .get(&r.key.call_id)
                                .is_some_and(|c| c.wire_call_id == n(b, "callId") as u16)
                            && d.devices.get(&r.key.device_id).is_some_and(|dev| {
                                dev.identities.iter().any(|i| {
                                    i.address == addr(&context.request.values["header"]["dstAddr"])
                                })
                            })
                    })
                    .map(|r| r.key.clone());
                let cleanup = key
                    .and_then(|key| d.resource_timeout(&key, now_ms()).ok())
                    .unwrap_or_default();
                let _ = self
                    .db
                    .save_entity(&self.project(), "domain", "current", &json!(*d));
                cleanup
            };
            if let Err(error) = self.schedule_cleanup(cleanup) {
                tracing::error!(%error,"资源超时清理失败");
            }
        }
    }
    fn maintain_business(&self) {
        let _gate = self.project_gate.read().unwrap();
        use std::sync::atomic::Ordering;
        let now = now_ms();
        let last = self.last_maintenance_ms.load(Ordering::Relaxed);
        if now.saturating_sub(last) < 1000 {
            return;
        }
        self.last_maintenance_ms.store(now, Ordering::Relaxed);
        let cleanup = {
            let mut d = self.domain.lock().unwrap();
            let registered_before = d
                .devices
                .values()
                .filter(|device| device.registration_valid)
                .count();
            let active_sessions_before =
                d.sessions.values().filter(|session| session.active).count();
            let timeout = self.heartbeat_timeout_ms.load(Ordering::Relaxed);
            let mut actions = if timeout == 0 {
                vec![]
            } else {
                d.expire_heartbeats(now, timeout)
            };
            let expired: Vec<_> = d
                .sessions
                .values()
                .filter(|s| s.active && s.valid_until_ms <= now)
                .map(|s| s.device_id.clone())
                .collect();
            for id in expired {
                actions.extend(d.offline_device(&id, now, None).unwrap_or_default());
            }
            // A granted session may expire before registration completes, while
            // its device is already OFFLINE. Invalidate the license as well.
            for session in d
                .sessions
                .values_mut()
                .filter(|s| s.active && s.valid_until_ms <= now)
            {
                session.active = false;
            }
            let state_changed = registered_before
                != d.devices
                    .values()
                    .filter(|device| device.registration_valid)
                    .count()
                || active_sessions_before
                    != d.sessions.values().filter(|session| session.active).count();
            if state_changed || !actions.is_empty() {
                let _ = self
                    .db
                    .save_entity(&self.project(), "domain", "current", &json!(*d));
            }
            actions
        };
        if let Err(error) = self.schedule_cleanup(cleanup) {
            tracing::error!(%error,"离线清理失败");
        }
        let mut x = self.execution.lock().unwrap();
        let mut ended: Vec<_> = x
            .engine
            .runs
            .values()
            .filter_map(|r| r.ended_ms.map(|end| (r.id.clone(), end)))
            .collect();
        ended.sort_by_key(|(_, t)| *t);
        let remove = ended.len().saturating_sub(1000);
        for (id, _) in ended.into_iter().take(remove) {
            x.engine.runs.remove(&id);
        }
        let retained_runs: std::collections::HashSet<_> = x.engine.runs.keys().cloned().collect();
        x.call_requests.retain(|_, r| {
            retained_runs.contains(&r.run_id) || now.saturating_sub(r.recorded_ms) < 60_000
        });
    }
}
pub fn redact(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                if k.to_lowercase().contains("token") {
                    *v = json!("[已脱敏]");
                } else {
                    redact(v)
                }
            }
        }
        Value::Array(a) => {
            for v in a {
                redact(v)
            }
        }
        _ => {}
    }
}
pub fn redact_known(value: &mut Value, secrets: &std::collections::HashSet<String>) {
    redact(value);
    match value {
        Value::String(s) if secrets.contains(&s.replace(' ', "").to_ascii_uppercase()) => {
            *s = "[已脱敏]".into()
        }
        Value::Object(map) => {
            for v in map.values_mut() {
                redact_known(v, secrets)
            }
        }
        Value::Array(a) => {
            for v in a {
                redact_known(v, secrets)
            }
        }
        _ => {}
    }
}
