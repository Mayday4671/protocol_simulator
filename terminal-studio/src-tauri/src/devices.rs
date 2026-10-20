use std::{collections::HashMap, net::SocketAddr, time::{Duration, Instant}};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use simterm_core::{error::{AppError, AppResult}, fields::extract_field_values, models::{DefMode, FieldEnc, FieldRole, MsgType, Node, Protocol, ProtocolField}, profile, render::{compact_hex, DictMap}};
use crate::{db, engine::{state::EngineState, udp::frame_specs}};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TerminalProfile {
    pub id: i64, pub name: String, pub node_ref: i64, pub system_ref: i64,
    pub protocol_ref: String, pub device_key: String, pub fields_json: String,
    pub session_hex: Option<String>,
}

/// 系统内参与设备档案的一套协议: 字段模式、可由设备发送(非应答)、且至少绑定一个设备属性 Key。
/// 设备档案的字段集合 = 系统内全部 DeviceSchema 的并集。
pub struct DeviceSchema {
    pub protocol_id: String,
    pub name: String,
    pub fields: Vec<ProtocolField>,
    pub dicts: DictMap,
}

pub async fn system_schemas(pool: &SqlitePool, system_ref: i64) -> AppResult<Vec<DeviceSchema>> {
    let mut schemas = Vec::new();
    for p in db::list_protocols(pool).await? {
        if p.system_ref != Some(system_ref) || p.msg_type == MsgType::Response || p.def_mode != DefMode::Fields { continue; }
        let fields = fields_for(pool, &p).await?;
        if !fields.iter().any(|f| profile::key(f).is_some()) { continue; }
        let dicts = db::dict_map_for_group(pool, p.group_ref).await?;
        schemas.push(DeviceSchema { protocol_id: p.protocol_id.clone(), name: p.name.clone(), fields, dicts });
    }
    Ok(schemas)
}

pub async fn fields_for(pool: &SqlitePool, p: &Protocol) -> AppResult<Vec<ProtocolField>> {
    let header = db::protocol_headers(pool, p).await?;
    frame_specs(p, &header)
}

pub fn identity_param(f: &ProtocolField, identity: &str) -> AppResult<String> {
    let bytes = hex::decode(compact_hex(identity)).map_err(|_| AppError::msg("设备标识不是合法 Hex"))?;
    if bytes.len() != f.length as usize { return Err(AppError::msg(format!("设备标识需 {} 字节", f.length))); }
    Ok(match f.enc {
        FieldEnc::Uint if f.dict_key().is_none() => {
            if bytes.len() > 16 { return Err(AppError::msg("整数设备标识最多 16 字节")); }
            bytes.iter().fold(0u128, |n,b| (n<<8)|*b as u128).to_string()
        },
        FieldEnc::Ascii if f.dict_key().is_none() => String::from_utf8(bytes).map_err(|_|AppError::msg("设备标识不是有效文本"))?.trim_end_matches('\0').to_string(),
        _ => hex::encode_upper(bytes),
    })
}

pub fn identity_field(fields: &[ProtocolField]) -> Option<&ProtocolField> {
    fields.iter().find(|f| f.role == FieldRole::SrcId).or_else(|| fields.iter().find(|f| f.role == FieldRole::DeviceSn))
}

pub async fn save_profile(pool: &SqlitePool, mut p: TerminalProfile) -> AppResult<()> {
    p.name = p.name.trim().to_string(); p.device_key = compact_hex(&p.device_key);
    if p.name.is_empty() { return Err(AppError::msg("请填写设备名称")); }
    let node = db::get_node(pool, p.node_ref).await?;
    let schemas = system_schemas(pool, p.system_ref).await?;
    if schemas.is_empty() { return Err(AppError::msg("该协议系统内尚无协议绑定设备属性，请先在协议字段设置中绑定")); }
    // 设备标识对系统内每套声明了标识字段的协议统一校验, 长度不一致会在此显式报错
    let identities: Vec<(&DeviceSchema, &ProtocolField)> = schemas.iter()
        .filter_map(|s| identity_field(&s.fields).map(|f| (s, f))).collect();
    if identities.is_empty() { return Err(AppError::msg("该系统内协议需声明源设备或设备序列号字段")); }
    for (s, f) in &identities {
        identity_param(f, &p.device_key).map_err(|e| AppError::msg(format!("协议「{}」的设备标识校验失败: {e}", s.name)))?;
    }
    let input: HashMap<String, String> = serde_json::from_str(&p.fields_json).map_err(|_| AppError::msg("设备属性必须为 Key 到字符串的映射"))?;
    let borrowed: Vec<profile::ProfileSchema> = schemas.iter()
        .map(|s| profile::ProfileSchema { protocol_name: s.name.as_str(), fields: &s.fields, dicts: &s.dicts }).collect();
    p.fields_json = serde_json::to_string(&profile::normalize_profile(&borrowed, &input, &node)?).map_err(|e| AppError::msg(e.to_string()))?;
    // protocol_ref 仅记录档案最近一次的编辑/配置来源协议, 不再限定可保存的属性范围
    if !schemas.iter().any(|s| s.protocol_id == p.protocol_ref) { p.protocol_ref = schemas[0].protocol_id.clone(); }
    if p.id > 0 {
        let n = sqlx::query("UPDATE terminal_profiles SET name=?,protocol_ref=?,fields_json=? WHERE id=? AND node_ref=? AND system_ref=? AND device_key=?")
            .bind(p.name).bind(p.protocol_ref).bind(p.fields_json).bind(p.id).bind(p.node_ref).bind(p.system_ref).bind(p.device_key).execute(pool).await?.rows_affected();
        if n == 0 { return Err(AppError::msg("设备身份不能修改，请新建设备")); }
    } else {
        let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM terminal_profiles WHERE node_ref=? AND system_ref=? AND device_key=?")
            .bind(p.node_ref).bind(p.system_ref).bind(&p.device_key).fetch_one(pool).await?;
        if exists > 0 { return Err(AppError::msg("此设备已经存在，请编辑已有设备")); }
        sqlx::query("INSERT INTO terminal_profiles(name,node_ref,system_ref,protocol_ref,device_key,fields_json) VALUES(?,?,?,?,?,?)")
            .bind(p.name).bind(p.node_ref).bind(p.system_ref).bind(p.protocol_ref).bind(p.device_key).bind(p.fields_json).execute(pool).await?;
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConfigKey {
    pub node: i64, pub system: i64, pub response: String, pub sequence: String, pub device: String, pub peer: SocketAddr,
}
pub struct PendingConfig {
    pub sent_at: Instant, pub request: String, pub properties: HashMap<String,String>,
}
const CONFIG_TTL: Duration = Duration::from_secs(300);

fn wire_value(f: &ProtocolField, v: &str) -> Option<String> {
    match f.enc {
        FieldEnc::Uint if f.dict_key().is_none() => Some(format!("{:0width$X}", v.parse::<u128>().ok()?, width=f.length as usize*2)),
        FieldEnc::Ascii if f.dict_key().is_none() => Some(format!("{:<0width$}",hex::encode_upper(v),width=f.length as usize*2)),
        _ => Some(compact_hex(v)),
    }
}

pub async fn track_config(pool: &SqlitePool, engine: &EngineState, node: &Node, request: &Protocol, fields: &[ProtocolField], data: &[u8], dicts: &DictMap, peer: SocketAddr) -> AppResult<Option<ConfigKey>> {
    if !fields.iter().any(|f| profile::key(f).is_some()) { return Ok(None); }
    let Some(response_id) = &request.auto_respond else { return Ok(None) };
    let response = db::get_protocol(pool, response_id).await?;
    let response_fields = fields_for(pool, &response).await?;
    if !response_fields.iter().any(|f| f.role == FieldRole::Result) { return Ok(None); }
    if response.system_ref != request.system_ref { return Err(AppError::msg("配置应答必须属于同一协议系统")); }
    let values = extract_field_values(fields, data, dicts);
    let correlation = fields.iter().find(|f| f.role == FieldRole::Correlation).ok_or_else(|| AppError::msg("配置请求需声明请求流水号语义，不能用最近一次发送记录猜测应答"))?;
    if !response_fields.iter().any(|f| f.role == FieldRole::Correlation) { return Err(AppError::msg("配置应答需回显请求流水号")); }
    let identity = identity_field(fields).ok_or_else(|| AppError::msg("配置请求缺少设备标识"))?;
    let get = |f: &ProtocolField| values.get(&f.name).and_then(|v| wire_value(f,v)).ok_or_else(|| AppError::msg(format!("配置请求缺少字段 [{}]",f.name)));
    let key = ConfigKey {node: node.id, system: request.system_ref.unwrap_or(-1),response:response_id.clone(),sequence:get(correlation)?,device:get(identity)?,peer};
    let properties = profile::normalize_properties(fields,&profile::collect_properties(fields,&values),node,dicts)?;
    engine.pending_configs.retain(|_,v| v.sent_at.elapsed() < CONFIG_TTL);
    if engine.pending_configs.len() >= 2048 { return Err(AppError::msg("待确认的配置请求过多，请等待应答后再试")); }
    match engine.pending_configs.entry(key.clone()) {
        dashmap::mapref::entry::Entry::Occupied(_) => return Err(AppError::msg("此设备的请求流水号仍在等待应答，请使用新流水号")),
        dashmap::mapref::entry::Entry::Vacant(entry) => {entry.insert(PendingConfig{sent_at:Instant::now(),request:request.protocol_id.clone(),properties});}
    }
    Ok(Some(key))
}

/// Only an exact ACK consumes a pending snapshot; wrong-device, replay and expired ACKs do nothing.
pub async fn accept_config(pool: &SqlitePool, engine: &EngineState, node: &Node, response: &Protocol, fields: &[ProtocolField], values: &HashMap<String,String>, dicts: &DictMap, peer: SocketAddr) -> AppResult<bool> {
    let Some(result) = fields.iter().find(|f| f.role == FieldRole::Result) else { return Ok(false) };
    let Some(seq) = fields.iter().find(|f| f.role == FieldRole::Correlation) else { return Ok(false) };
    let Some(device) = fields.iter().find(|f| f.role == FieldRole::DstId).or_else(|| identity_field(fields)) else { return Ok(false) };
    let get = |f: &ProtocolField| values.get(&f.name).and_then(|v| wire_value(f,v));
    let (Some(sequence),Some(device)) = (get(seq),get(device)) else { return Ok(false) };
    let key = ConfigKey{node:node.id,system:response.system_ref.unwrap_or(-1),response:response.protocol_id.clone(),sequence,device,peer};
    let Some((_,pending)) = engine.pending_configs.remove(&key) else { return Ok(false) };
    let ok = result.dict_key().and_then(|k| dicts.get(k)).and_then(|s| s.default_value());
    if pending.sent_at.elapsed() >= CONFIG_TTL || ok.is_none() || get(result).as_deref() != ok { return Ok(false); }
    let identity = (key.node, key.system, key.device.clone());
    // UDP can reorder successful replies. Serialize comparison+commit, so an older
    // request cannot roll back a newer configuration already acknowledged for this device.
    let mut applied = engine.applied_configs.lock().await;
    if applied.get(&identity).is_some_and(|at| *at >= pending.sent_at) { return Ok(false); }
    // 合并而非整体覆盖: 其他协议写入的属性保留, 本次配置应答确认的 Key 以应答为准
    let stored: Option<String> = sqlx::query_scalar("SELECT fields_json FROM terminal_profiles WHERE node_ref=? AND system_ref=? AND device_key=?")
        .bind(key.node).bind(key.system).bind(&key.device).fetch_optional(pool).await?;
    let mut properties = pending.properties.clone();
    if let Some(json) = stored {
        let mut merged: HashMap<String, String> = serde_json::from_str(&json).unwrap_or_default();
        merged.extend(pending.properties.clone());
        properties = merged;
    }
    let json = serde_json::to_string(&properties).map_err(|e|AppError::msg(e.to_string()))?;
    // Keep the user-entered name and current session when committing configuration.
    let n = sqlx::query("INSERT INTO terminal_profiles(name,node_ref,system_ref,protocol_ref,device_key,fields_json) VALUES(?,?,?,?,?,?) ON CONFLICT(node_ref,system_ref,device_key) DO UPDATE SET protocol_ref=excluded.protocol_ref,fields_json=excluded.fields_json")
        .bind(format!("设备 {}",key.device)).bind(key.node).bind(key.system).bind(&pending.request).bind(&key.device).bind(json).execute(pool).await?.rows_affected();
    if n == 0 { return Err(AppError::msg("配置应答提交失败")); }
    applied.insert(identity, pending.sent_at);
    Ok(true)
}
