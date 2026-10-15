use crate::{
    application::{format_hex, parse_hex, required},
    domain::*,
    network::now_ms,
};
use serde_json::{Value, json};

pub fn device_view(d: &Device) -> Value {
    json!({"id":d.id,"systemId":d.system_id,"name":d.name,"address":d.identities.first().map(|i|format_hex(&i.address)).unwrap_or_default(),"status":d.state,"source":d.origin,"config":d.config,"remote":d.config["remote"],"identities":d.identities,"configVersion":d.config_version,"activeCalls":d.active_calls})
}
pub fn domain_views(store: &DomainStore) -> (Vec<Value>, Vec<Value>, Vec<Value>) {
    let devices = store.devices.values().map(device_view).collect();
    let calls=store.calls.values().map(|c|json!({"id":c.id,"callId":format!("{:04X}",c.wire_call_id),"type":c.kind,"status":c.state,"participants":c.participants,"startupMode":c.start_mode,"epoch":c.epoch,"requestedRx":store.resources.values().filter(|r|r.key.call_id==c.id&&r.key.direction==Direction::Rx).map(|r|r.requested_bw).sum::<u64>(),"requestedTx":store.resources.values().filter(|r|r.key.call_id==c.id&&r.key.direction==Direction::Tx).map(|r|r.requested_bw).sum::<u64>()})).collect();
    let resources=store.resources.values().map(|r|json!({"id":r.id,"callId":r.key.call_id,"deviceId":r.key.device_id,"direction":r.key.direction,"version":r.key.version,"status":r.state,"bandwidth":r.assigned_bw,"requestedBw":r.requested_bw,"reservedBw":r.reserved_bw,"releasedBw":r.released_bw,"poolId":r.pool_id,"resourceId":r.key.resource_id,"unit":r.unit})).collect();
    (devices, calls, resources)
}
pub fn upsert_device(store: &mut DomainStore, r: &Value) -> Result<Value, String> {
    let address = parse_hex(required(r, "address")?)?;
    if address.len() != 3 {
        return Err("MUSP协议地址必须为3字节".into());
    }
    let system = required(r, "systemId")?;
    let name = required(r, "name")?;
    if name.len() > 256 {
        return Err("设备名称过长".into());
    }
    let protocol = r["protocolId"].as_str().unwrap_or("MUSP");
    let id = if let Some(id) = r["id"].as_str() {
        if store
            .find_device(system, protocol, &address)
            .is_some_and(|d| d.id != id)
        {
            return Err("同系统协议地址已存在".into());
        }
        let d = store.devices.get_mut(id).ok_or("设备不存在")?;
        if d.registration_valid && d.identities.first().is_some_and(|i| i.address != address) {
            return Err("在线设备须先离线再更改协议身份".into());
        }
        d.name = name.into();
        d.system_id = system.into();
        d.identities = vec![ProtocolIdentity {
            protocol_id: protocol.into(),
            address,
        }];
        id.into()
    } else {
        store.create_device(system, name, protocol, address, "MANUAL")?
    };
    if r.get("config").is_some() || r.get("remote").is_some() {
        let mut config = r
            .get("config")
            .cloned()
            .unwrap_or_else(|| store.devices[&id].config.clone());
        if !config.is_object() {
            return Err("设备配置必须为JSON对象".into());
        }
        if let Some(remote) = r.get("remote") {
            if let Some(s) = remote.as_str()
                && !s.is_empty()
            {
                s.parse::<std::net::SocketAddr>()
                    .map_err(|_| "远端地址须为IP:端口")?;
            }
            config["remote"] = remote.clone();
        }
        let version = store.devices[&id].config_version + 1;
        store.update_config(&id, config, version)?;
    }
    Ok(device_view(&store.devices[&id]))
}
pub fn delete_device(store: &mut DomainStore, id: &str) -> Result<(), String> {
    let d = store.devices.get(id).ok_or("设备不存在")?;
    if d.registration_valid
        || store
            .resources
            .values()
            .any(|r| r.key.device_id == id && r.state != ResourceState::Released)
    {
        return Err("设备仍在线或持有未释放资源，不能删除".into());
    }
    store.devices.remove(id);
    store.sessions.retain(|_, s| s.device_id != id);
    Ok(())
}

/// Used by explicitly authored workflow update nodes. Input variables are resolved
/// by the runtime, and network ACK effects are only applied after correlation.
pub fn update_domain(
    store: &mut DomainStore,
    project: &str,
    run: &str,
    r: &Value,
) -> Result<Value, String> {
    let op = required(r, "operation")?;
    match op {
        "session_token" => {
            let device = required(r, "deviceId")?;
            let peer = format!(
                "{}|{}|{}",
                required(r, "endpointId")?,
                required(r, "protocolVersionId")?,
                required(r, "remote")?
            );
            let sessions: Vec<_> = store
                .sessions
                .values()
                .filter(|s| {
                    s.device_id == device
                        && s.active
                        && s.peer_system_id == peer
                        && r["sessionId"]
                            .as_u64()
                            .is_none_or(|id| id == u64::from(s.wire_session_id))
                })
                .collect();
            if sessions.len() != 1 {
                return Err("认证会话与当前端点/协议版本/对端不匹配或不唯一".into());
            }
            let session = sessions[0];
            let token = store.session_token(&session.id, now_ms())?;
            Ok(json!({"token":format_hex(token),"sessionId":session.wire_session_id}))
        }
        "create_device" => upsert_device(store, r),
        "update_config" => {
            let id = required(r, "deviceId")?;
            store.update_config(id, r["config"].clone(), r["version"].as_u64().unwrap_or(1))?;
            Ok(json!({"deviceId":id}))
        }
        "create_call" => {
            let participants: Vec<String> =
                serde_json::from_value(r["participants"].clone()).map_err(|e| e.to_string())?;
            let mode = if r["startMode"] == "CONTROL_NOTIFY" {
                StartMode::ControlNotify
            } else {
                StartMode::LocalReady
            };
            let id = store.create_call(
                project,
                run,
                r["wireCallId"]
                    .as_u64()
                    .filter(|n| *n <= 65535)
                    .ok_or("callId须为2字节")? as u16,
                r["kind"].as_str().unwrap_or("VIDEO"),
                participants,
                mode,
                now_ms(),
            )?;
            Ok(json!({"callId":id}))
        }
        "permit_call" => {
            store.permit_call(required(r, "callId")?)?;
            Ok(json!({"permitted":true}))
        }
        "add_pool" => {
            let id = required(r, "poolId")?;
            store.add_pool(
                id,
                r["name"].as_str().unwrap_or(id),
                r["capacity"].as_u64().ok_or("资源容量无效")?,
                r["unit"].as_str().unwrap_or("kbps"),
            )?;
            Ok(json!({"poolId":id}))
        }
        "reserve_resource" => {
            let direction = if r["direction"] == "RX" {
                Direction::Rx
            } else if r["direction"] == "TX" {
                Direction::Tx
            } else {
                return Err("方向须为RX/TX".into());
            };
            let id = store.reserve_resource(
                required(r, "callId")?,
                required(r, "deviceId")?,
                r["resourceId"]
                    .as_u64()
                    .filter(|n| *n <= u16::MAX as u64)
                    .ok_or("resourceId须为2字节")? as u16,
                direction,
                r["version"]
                    .as_u64()
                    .unwrap_or(1)
                    .try_into()
                    .map_err(|_| "resource version须为2字节")?,
                required(r, "poolId")?,
                r["requestedBw"].as_u64().ok_or("缺少申请带宽")?,
                r["assignedBw"].as_u64().ok_or("缺少分配带宽")?,
            )?;
            Ok(json!({"resourceUuid":id}))
        }
        "mark_configuring" => {
            store.mark_configuring(required(r, "resourceUuid")?)?;
            Ok(json!({"configuring":true}))
        }
        _ => Err(format!("不支持的领域操作: {op}")),
    }
}
