//! Cleanup uses the same real UDP and correlated flow executor as user flows.
use crate::{
    application::Studio,
    domain::*,
    flow_host::{EndContext, WireBinding},
    network::{Datagram, now_ms},
    packet_business::{addr, n, registered_sender},
    protocol::DecodedPacket,
    runtime::*,
};
use serde_json::{Value, json};
use std::sync::Arc;

impl Studio {
    pub async fn end_call(self: &Arc<Self>, id: &str) -> Result<Value, String> {
        self.execution
            .lock()
            .unwrap()
            .end_contexts
            .entry(id.into())
            .or_insert(EndContext {
                reason: 3,
                release_reason: 0,
                initiator: Value::Null,
            });
        let actions = {
            let mut domain = self.domain.lock().unwrap();
            let actions = domain.begin_end_call(id, now_ms())?;
            self.db
                .save_entity(&self.project(), "domain", "current", &json!(*domain))?;
            actions
        };
        self.schedule_cleanup(actions)?;
        Ok(json!({"accepted":true,"callId":id}))
    }

    pub fn schedule_cleanup(&self, actions: Vec<CleanupAction>) -> Result<(), String> {
        for action in actions {
            let (call_id, device_id, body, code, expected) = match action {
                CleanupAction::NotifyEnd { call_id, device_id } => (
                    call_id,
                    device_id,
                    json!({"reasonCode":0,"initiatorAddr":0,"stopMode":0}),
                    0x78,
                    0x79,
                ),
                CleanupAction::ReleaseResource { resource } => (
                    resource.call_id.clone(),
                    resource.device_id.clone(),
                    json!({"resourceId":resource.resource_id,"direction":if resource.direction==Direction::Rx{1}else{2},"resourceVersion":resource.version,"releaseReason":0}),
                    0x76,
                    0x77,
                ),
                CleanupAction::UnknownResource { .. } => continue,
            };
            let binding = self
                .execution
                .lock()
                .unwrap()
                .bindings
                .get(&format!("{call_id}:{device_id}"))
                .cloned();
            let Some(binding) = binding else {
                let mut domain = self.domain.lock().unwrap();
                let keys: Vec<_> = domain
                    .resources
                    .values()
                    .filter(|r| {
                        r.key.call_id == call_id
                            && r.key.device_id == device_id
                            && r.state != ResourceState::Released
                    })
                    .map(|r| r.key.clone())
                    .collect();
                for key in keys {
                    domain.resource_timeout(&key, now_ms())?;
                }
                self.db
                    .save_entity(&self.project(), "domain", "current", &json!(*domain))?;
                continue;
            };
            let call = self
                .domain
                .lock()
                .unwrap()
                .calls
                .get(&call_id)
                .cloned()
                .ok_or("通话不存在")?;
            let mut body = body;
            body["callId"] = json!(call.wire_call_id);
            if let Some(context) = self.execution.lock().unwrap().end_contexts.get(&call_id) {
                if code == 0x78 {
                    body["reasonCode"] = json!(context.reason);
                    body["initiatorAddr"] = if context.initiator.is_null() {
                        binding.src.clone()
                    } else {
                        context.initiator.clone()
                    };
                }
                if code == 0x76 {
                    body["releaseReason"] = json!(context.release_reason);
                }
            }
            let WireBinding {
                endpoint_id,
                protocol_id,
                remote,
                src,
                dst,
                session,
                system_id,
                ..
            } = binding;
            let id = uuid::Uuid::new_v4().to_string();
            let definition:FlowDefinition=serde_json::from_value(json!({"id":id,"nameZh":"通话结束与资源回收","version":id,"protocol":{"id":protocol_id},"nodes":[
                {"id":"start","type":"START"},
                {"id":"cleanup","type":"SEND_AND_WAIT","data":{"endpointId":endpoint_id,"protocolVersionId":protocol_id,"remote":remote,"code":code,"expectedCodes":[expected],"timeoutMs":1000,"retries":2,"retryIntervalMs":100,"values":{"srcAddr":src,"dstAddr":dst,"sessionId":session,"body":body}}},
                {"id":"end","type":"END"}],"edges":[{"source":"start","target":"cleanup"},{"source":"cleanup","target":"end","label":"matched"},{"source":"cleanup","target":"end","label":"timeout"},{"source":"cleanup","target":"end","label":"error"}]})).map_err(|e|e.to_string())?;
            let mut x = self.execution.lock().unwrap();
            x.engine.publish(definition)?;
            x.engine.start(
                &id,
                json!({"scenarioRunId":call.scenario_run_id,"systemId":system_id}),
                now_ms(),
            )?;
            // start freezes this generated definition into the run snapshot.
            x.engine.definitions.remove(&id);
        }
        Ok(())
    }

    pub(crate) fn call_request_key(
        packet: &Datagram,
        decoded: &DecodedPacket,
        session_record: &str,
        transaction: u32,
    ) -> String {
        let h = &decoded.values["header"];
        format!(
            "{}|{}|{}|{}|{:?}|{:?}|{}",
            packet.endpoint_id,
            packet.protocol_id,
            packet.remote,
            session_record,
            addr(&h["srcAddr"]),
            addr(&h["dstAddr"]),
            transaction
        )
    }

    pub(crate) fn handle_call_request(
        self: &Arc<Self>,
        packet: &Datagram,
        decoded: &DecodedPacket,
    ) -> Result<Value, String> {
        let h = &decoded.values["header"];
        let body = &decoded.values["body"];
        if decoded.message_code == 0x7B
            && (n(body, "cancelReason") > 2
                || n(body, "originalTransactionId") == n(h, "transactionId"))
        {
            return Err("取消请求必须使用新事务，取消原因限于原协议00/01/02".into());
        }
        let peer = format!(
            "{}|{}|{}",
            packet.endpoint_id, packet.protocol_id, packet.remote
        );
        let mut cancelled_actions = Vec::new();
        let mut cleanup = Vec::new();
        let result;
        {
            // Same lock order as a queued workflow domain action.
            let mut execution = self.execution.lock().unwrap();
            let mut domain = self.domain.lock().unwrap();
            let (device, session) = registered_sender(&domain, &peer, decoded)?;
            let mut candidate = domain.clone();
            if decoded.message_code == 0x73 {
                let wire = n(body, "callId") as u16;
                let matching: Vec<_> = candidate
                    .calls
                    .values()
                    .filter(|call| {
                        call.wire_call_id == wire
                            && call.participants.contains(&device.id)
                            && execution
                                .bindings
                                .get(&format!("{}:{}", call.id, device.id))
                                .is_some_and(|binding| {
                                    binding.endpoint_id == packet.endpoint_id
                                        && binding.protocol_id == packet.protocol_id
                                        && binding.remote == packet.remote
                                        && binding.session == session.wire_session_id
                                        && binding.session_record_id.as_deref() == Some(&session.id)
                                        && addr(&binding.dst) == addr(&h["srcAddr"])
                                        && addr(&binding.src) == addr(&h["dstAddr"])
                                })
                    })
                    .cloned()
                    .collect();
                result = match matching.as_slice() {
                    [] => 1,
                    [call] => {
                        if call.end_started {
                            if call.state == CallState::Ended { 0 } else { 3 }
                        } else {
                            let reason = n(body, "reasonCode");
                            if reason > 3 {
                                2
                            } else {
                                execution.end_contexts.insert(
                                    call.id.clone(),
                                    EndContext {
                                        reason: if reason == 1 {
                                            1
                                        } else if reason == 2 {
                                            2
                                        } else {
                                            0
                                        },
                                        release_reason: if reason == 1 {
                                            1
                                        } else if reason == 2 {
                                            3
                                        } else {
                                            0
                                        },
                                        initiator: h["srcAddr"].clone(),
                                    },
                                );
                                // A setup run belonging only to this call must not
                                // apply late configuration after end is accepted.
                                if candidate
                                    .calls
                                    .values()
                                    .filter(|c| c.scenario_run_id == call.scenario_run_id)
                                    .count()
                                    == 1
                                {
                                    cancelled_actions.extend(
                                        execution.engine.cancel(&call.scenario_run_id, now_ms()),
                                    );
                                }
                                cleanup.extend(candidate.begin_end_call(&call.id, now_ms())?);
                                0
                            }
                        }
                    }
                    _ => 2,
                };
            } else {
                let key = Self::call_request_key(
                    packet,
                    decoded,
                    &session.id,
                    n(body, "originalTransactionId") as u32,
                );
                let request = execution.call_requests.get(&key).cloned();
                result = if let Some(request) = request {
                    let calls: Vec<_> = candidate
                        .calls
                        .values()
                        .filter(|c| c.scenario_run_id == request.run_id)
                        .cloned()
                        .collect();
                    if calls
                        .iter()
                        .any(|c| matches!(c.state, CallState::Active | CallState::Ready))
                    {
                        2
                    } else if request.cancelled {
                        0
                    } else {
                        execution.call_requests.get_mut(&key).unwrap().cancelled = true;
                        cancelled_actions
                            .extend(execution.engine.cancel(&request.run_id, now_ms()));
                        for call in calls {
                            execution
                                .end_contexts
                                .entry(call.id.clone())
                                .or_insert(EndContext {
                                    reason: if n(body, "cancelReason") == 1 { 2 } else { 0 },
                                    release_reason: if n(body, "cancelReason") == 1 {
                                        3
                                    } else {
                                        0
                                    },
                                    initiator: h["srcAddr"].clone(),
                                });
                            cleanup.extend(candidate.begin_end_call(&call.id, now_ms())?);
                        }
                        0
                    }
                } else {
                    1
                };
            }
            self.db
                .save_entity(&self.project(), "domain", "current", &json!(candidate))?;
            *domain = candidate;
        }
        let response_body = if decoded.message_code == 0x73 {
            json!({"callId":body["callId"],"resultCode":result})
        } else {
            json!({"resultCode":result})
        };
        let code = if decoded.message_code == 0x73 {
            0x74
        } else {
            0x7C
        };
        let bytes=self.schema(&packet.protocol_id)?.encode(code,&json!({"srcAddr":h["dstAddr"],"dstAddr":h["srcAddr"],
            "sessionId":h["sessionId"],"transactionId":h["transactionId"],"flags":2,"body":response_body})).map_err(|e|e.to_string())?;
        let app = self.clone();
        let endpoint = packet.endpoint_id.clone();
        let remote = packet.remote.clone();
        let generation = packet.generation.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(error) = app
                .udp
                .send_generation(&endpoint, &remote, &bytes, Some(&generation))
                .await
            {
                tracing::error!(%error,"结束/取消受理响应发送失败");
            }
            if let Err(error) = app.schedule_cleanup(cleanup) {
                tracing::error!(%error,"结束/取消资源清理失败");
            }
            app.execute_actions(cancelled_actions).await;
        });
        Ok(json!({"responseCode":code,"resultCode":result,"accepted":result==0||result==3}))
    }

    /// Emit an actual asynchronous 0x7A only after the current cleanup waits
    /// settle. UNKNOWN capacity remains reserved; send success is not delivery.
    pub(crate) fn publish_cleanup_results(&self) -> Result<(), String> {
        let domain = self.domain.lock().unwrap().clone();
        let mut execution = self.execution.lock().unwrap();
        let mut notifications = Vec::new();
        for call in domain.calls.values().filter(|call| call.end_started) {
            let resources: Vec<_> = domain
                .resources
                .values()
                .filter(|r| r.key.call_id == call.id)
                .collect();
            if resources.iter().any(|r| {
                matches!(
                    r.state,
                    ResourceState::Reserved | ResourceState::Configuring | ResourceState::Releasing
                )
            }) {
                continue;
            }
            if execution.sent.iter().any(|(id, s)| {
                s.scenario_id == call.scenario_run_id
                    && n(&s.request.values["body"], "callId") == u64::from(call.wire_call_id)
                    && execution
                        .engine
                        .pending
                        .requests
                        .get(id)
                        .is_some_and(|p| p.status.is_open())
            }) {
                continue;
            }
            let unknown = resources
                .iter()
                .filter(|r| r.state != ResourceState::Released)
                .count();
            let count = u8::try_from(unknown).map_err(|_| "未核实资源数超过0x7A一字节上限")?;
            let code = if unknown == 0 && call.active_devices.is_empty() {
                0
            } else if resources.iter().any(|r| r.state == ResourceState::Failed) {
                3
            } else if resources.iter().any(|r| r.state == ResourceState::Released) {
                1
            } else {
                2
            };
            if execution.cleanup_summaries.get(&call.id) == Some(&(code, count)) {
                continue;
            }
            execution
                .cleanup_summaries
                .insert(call.id.clone(), (code, count));
            for participant in &call.participants {
                if let Some(binding) = execution
                    .bindings
                    .get(&format!("{}:{}", call.id, participant))
                {
                    if !domain
                        .devices
                        .get(participant)
                        .is_some_and(|d| d.registration_valid)
                    {
                        continue;
                    }
                    if binding.session_record_id.as_ref().is_some_and(|id| {
                        !domain
                            .sessions
                            .get(id)
                            .is_some_and(|s| s.active && s.registered)
                    }) {
                        continue;
                    }
                    notifications.push((
                        binding.clone(),
                        json!({"callId":call.wire_call_id,"resultCode":code,"unknownCount":count}),
                    ));
                }
            }
        }
        for (binding, body) in notifications {
            let id = uuid::Uuid::new_v4().to_string();
            let definition:FlowDefinition=serde_json::from_value(json!({"id":id,"nameZh":"通话结束结果汇总","protocol":{"id":binding.protocol_id},"nodes":[
                {"id":"start","type":"START"},{"id":"result","type":"SEND","data":{"endpointId":binding.endpoint_id,"protocolVersionId":binding.protocol_id,"remote":binding.remote,"code":0x7A,"values":{"srcAddr":binding.src,"dstAddr":binding.dst,"sessionId":binding.session,"body":body}}},{"id":"end","type":"END"}],
                "edges":[{"source":"start","target":"result"},{"source":"result","target":"end"}]})).map_err(|e|e.to_string())?;
            execution.engine.publish(definition)?;
            execution.engine.start(&id, json!({}), now_ms())?;
            execution.engine.definitions.remove(&id);
        }
        Ok(())
    }

    /// Operator reconciliation is explicitly audited and never inferred from a timeout.
    pub fn reconcile_resource(&self, r: &Value) -> Result<Value, String> {
        if r["confirmedReleased"] != true {
            return Err("必须明确确认已在设备侧核实释放".into());
        }
        let reason = crate::application::required(r, "reason")?;
        let id = crate::application::required(r, "resourceId")?;
        let mut store = self.domain.lock().unwrap();
        let mut candidate = store.clone();
        let resource = candidate.resources.get_mut(id).ok_or("资源不存在")?;
        if resource.state != ResourceState::Unknown {
            return Err("仅允许核对 UNKNOWN 资源".into());
        }
        let key = resource.key.clone();
        resource.state = ResourceState::Releasing;
        candidate.ack_release(&key, 0)?;
        self.db.save_entity(
            &self.project(),
            "reconciliations",
            &uuid::Uuid::new_v4().to_string(),
            &json!({"resourceId":id,"reason":reason,"timestamp":now_ms(),"confirmedReleased":true}),
        )?;
        self.db
            .save_entity(&self.project(), "domain", "current", &json!(candidate))?;
        *store = candidate;
        Ok(json!({"released":true}))
    }
}
