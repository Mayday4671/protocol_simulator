//! Business effects of validated, uniquely correlated MUSP packets.
use crate::{domain::*, network::now_ms, protocol::DecodedPacket};
use serde_json::{Value, json};

pub fn number(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| {
        value
            .as_str()
            .and_then(|s| u64::from_str_radix(&s.replace(' ', "").replace("0x", ""), 16).ok())
    })
}
pub fn addr(value: &Value) -> Vec<u8> {
    if let Some(array) = value.as_array() {
        return array
            .iter()
            .filter_map(|v| v.as_u64().map(|n| n as u8))
            .collect();
    }
    number(value)
        .map(|n| n.to_be_bytes()[5..].to_vec())
        .unwrap_or_default()
}
pub fn n(value: &Value, key: &str) -> u64 {
    number(&value[key]).unwrap_or(0)
}

/// Resolve one live registered sender within an endpoint/protocol/remote scope.
/// Internal session identity also distinguishes a re-registration reusing a
/// wire session number from the old call epoch.
pub fn registered_sender(
    store: &DomainStore,
    peer: &str,
    packet: &DecodedPacket,
) -> Result<(Device, Session), String> {
    let header = &packet.values["header"];
    let address = addr(&header["srcAddr"]);
    let candidates: Vec<_> = store
        .sessions
        .values()
        .filter(|s| {
            s.active
                && s.registered
                && s.valid_until_ms > now_ms()
                && s.peer_system_id == peer
                && s.wire_session_id == n(header, "sessionId") as u16
        })
        .filter_map(|s| {
            store
                .devices
                .get(&s.device_id)
                .filter(|d| {
                    d.registration_valid
                        && d.identities
                            .iter()
                            .any(|i| i.protocol_id == "MUSP" && i.address == address)
                })
                .map(|d| (d, s))
        })
        .collect();
    if candidates.len() != 1 {
        return Err("请求来源设备/端点/协议/对端/会话不唯一或已失效".into());
    }
    let (device, session) = candidates[0];
    if session.auth_required {
        store.session_token(&session.id, now_ms())?;
    }
    Ok((device.clone(), session.clone()))
}

pub fn apply_matched(
    store: &mut DomainStore,
    system: &str,
    remote: &str,
    scenario: &str,
    request: &DecodedPacket,
    response: &DecodedPacket,
) -> Result<Value, String> {
    let req = &request.values;
    let rsp = &response.values;
    let rb = &req["body"];
    let body = &rsp["body"];
    if response.message_code == 0x22 && request.message_code == 0x21 {
        let mode = match n(rb, "configMode") {
            1 => ConfigMode::Devices,
            2 => ConfigMode::Parameters,
            _ => return Err("未知配置映射模式".into()),
        };
        let operation = match n(rb, "operation") {
            1 => ConfigOperation::Replace,
            2 => ConfigOperation::Merge,
            3 => ConfigOperation::Delete,
            _ => return Err("未知配置操作".into()),
        };
        let items = rb["items"]
            .as_array()
            .ok_or("配置缺少条目数组")?
            .iter()
            .enumerate()
            .map(|(i, item)| ConfigItem {
                address: if mode == ConfigMode::Devices {
                    Some(addr(&item["deviceAddr"]))
                } else {
                    None
                },
                key: if mode == ConfigMode::Parameters {
                    n(item, "channelId").to_string()
                } else {
                    i.to_string()
                },
                values: item.clone(),
            })
            .collect();
        let results = body["results"]
            .as_array()
            .ok_or("响应缺少逐条结果")?
            .iter()
            .map(|v| {
                number(&v["result"])
                    .filter(|n| *n <= 255)
                    .map(|n| n as u8)
                    .ok_or("结果值无效".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let config = ConfigRequest {
            request_key: format!(
                "{system}:{remote}:{}:{}:{}",
                n(&req["header"], "transactionId"),
                n(&req["header"], "sessionId"),
                n(&req["header"], "srcAddr")
            ),
            system_id: system.into(),
            protocol_id: "MUSP".into(),
            mode,
            operation,
            target_address: Some(addr(&rb["targetDeviceAddr"])),
            version: n(rb, "configVersion"),
            items,
            effective_at: ApplyTrigger::SuccessAck,
            atomic: false,
            batch_complete: true,
        };
        // Candidate configFlags do not silently impose real-world atomic semantics.
        return serde_json::to_value(store.apply_config_result(&config, &results)?)
            .map_err(|e| e.to_string());
    }
    let source = addr(&req["header"]["srcAddr"]);
    let sender = store
        .find_device(system, "MUSP", &source)
        .map(|d| d.id.clone());
    match response.message_code {
        0x02 if request.message_code == 0x01 => {
            let device = sender.ok_or("注册响应无对应本地设备")?;
            if n(body, "resultCode") != 0 {
                return Err(format!("注册许可未成功: {:02X}", n(body, "resultCode")));
            }
            let sid = store.permit_session(
                &device,
                remote,
                n(&rsp["header"], "sessionId") as u16,
                now_ms().saturating_add(n(body, "validSeconds") * 1000),
                n(body, "authMode") == 1,
            )?;
            return Ok(json!({"sessionId":sid,"wireSessionId":n(&rsp["header"],"sessionId")}));
        }
        0x04 if request.message_code == 0x03 => {
            let device = sender.ok_or("注册完成无对应本地设备")?;
            let session = store
                .sessions
                .values()
                .find(|s| {
                    s.device_id == device
                        && s.peer_system_id == remote
                        && s.active
                        && s.wire_session_id == n(&rsp["header"], "sessionId") as u16
                })
                .ok_or("注册会话不匹配")?
                .id
                .clone();
            store.complete_registration(&session, n(body, "resultCode") as u8, now_ms())?;
        }
        0x32 if request.message_code == 0x31 => {
            if n(body, "resultCode") == 0
                && let Some(device) = sender
            {
                store.record_heartbeat(&device, now_ms())?;
            }
        }
        0x72 | 0x77 => {
            let target = addr(&req["header"]["dstAddr"]);
            let device = store
                .find_device(system, "MUSP", &target)
                .map(|d| d.id.clone())
                .ok_or("资源响应设备不匹配")?;
            let direction = if response.message_code == 0x72 {
                if request.message_code == 0x70 {
                    Direction::Rx
                } else {
                    Direction::Tx
                }
            } else if n(rb, "direction") == 1 {
                Direction::Rx
            } else {
                Direction::Tx
            };
            let key = store
                .resources
                .values()
                .find(|r| {
                    r.key.scenario_run_id == scenario
                        && r.key.device_id == device
                        && r.key.resource_id == n(rb, "resourceId") as u16
                        && r.key.direction == direction
                        && r.key.version == n(rb, "resourceVersion") as u16
                        && store
                            .calls
                            .get(&r.key.call_id)
                            .is_some_and(|c| c.wire_call_id == n(rb, "callId") as u16)
                })
                .ok_or("资源确认找不到当前资源版本")?
                .key
                .clone();
            if response.message_code == 0x72 {
                let cleanup =
                    store.acknowledge_resource(&key, n(body, "resultCode") as u8, now_ms())?;
                return Ok(json!({"cleanup":cleanup}));
            }
            store.ack_release(&key, n(body, "resultCode") as u8)?;
        }
        0x79 => {
            let device = store
                .find_device(system, "MUSP", &addr(&req["header"]["dstAddr"]))
                .map(|d| d.id.clone())
                .ok_or("事件确认设备不匹配")?;
            let call = store
                .calls
                .values()
                .find(|c| {
                    c.scenario_run_id == scenario
                        && c.wire_call_id == n(rb, "callId") as u16
                        && c.participants.contains(&device)
                })
                .ok_or("事件确认通话不匹配")?
                .id
                .clone();
            if n(body, "resultCode") == 0 {
                if n(body, "eventType") == 1 {
                    store.control_ready(&call, &device)?;
                } else if n(body, "eventType") == 2 {
                    store.stop_participant(&call, &device)?;
                }
            }
        }
        _ => {}
    }
    Ok(json!({"applied":true}))
}

pub fn apply_async(
    store: &mut DomainStore,
    system: &str,
    remote: &str,
    packet: &DecodedPacket,
) -> Result<Value, String> {
    let values = &packet.values;
    let header = &values["header"];
    let body = &values["body"];
    if packet.message_code == 0x11 {
        let target = addr(&header["dstAddr"]);
        let device = store
            .find_device(system, "MUSP", &target)
            .map(|d| d.id.clone())
            .ok_or("认证下发设备未知")?;
        let sid = store
            .sessions
            .values()
            .find(|s| {
                s.device_id == device
                    && s.active
                    && s.peer_system_id == remote
                    && s.wire_session_id == n(header, "sessionId") as u16
            })
            .ok_or("认证下发会话不匹配")?
            .id
            .clone();
        let token = body["token"]
            .as_str()
            .map(crate::application::parse_hex)
            .transpose()?
            .ok_or("认证Token格式错误")?;
        store.issue_token(
            &sid,
            token,
            now_ms().saturating_add(n(body, "tokenValidSeconds") * 1000),
            now_ms(),
        )?;
        return Ok(json!({"authReady":true}));
    }
    if packet.message_code == 0x7D {
        let source = addr(&header["srcAddr"]);
        let device = store
            .find_device(system, "MUSP", &source)
            .map(|d| d.id.clone())
            .ok_or("离线通知设备未知")?;
        if !store.sessions.values().any(|s| {
            s.device_id == device
                && s.active
                && s.peer_system_id == remote
                && s.wire_session_id == n(header, "sessionId") as u16
        }) {
            return Err("离线通知会话不匹配".into());
        }
        let actions = store.offline_device(
            &device,
            now_ms(),
            if n(body, "offlineMode") == 1 {
                Some(3000)
            } else {
                None
            },
        )?;
        return Ok(json!({"cleanup":actions}));
    }
    Ok(Value::Null)
}
