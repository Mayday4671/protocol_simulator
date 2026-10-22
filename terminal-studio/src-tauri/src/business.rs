use crate::{
    db, devices,
    engine::state::{AppState, EngineState},
};
use serde::Serialize;
use simterm_core::{
    business::{self, Row, SaveRule},
    error::{AppError, AppResult},
    models::{MsgType, Node, Protocol, ProtocolField},
    render::DictMap,
};
use sqlx::SqlitePool;
use std::{
    collections::BTreeMap,
    net::SocketAddr,
    time::{Duration, Instant},
};

pub struct PendingSave {
    rule: SaveRule,
    rows: Vec<Row>,
    sequence: String,
    sent_at: Instant,
    response_schema: String,
    record_type: Option<business::RecordType>,
}
pub type PendingSaves = BTreeMap<String, PendingSave>;

#[derive(Serialize, sqlx::FromRow)]
pub struct BusinessRecord {
    pub id: i64,
    pub node_ref: i64,
    pub system_ref: i64,
    pub kind: String,
    pub record_key: String,
    pub name: String,
    pub device_ref: Option<i64>,
    pub device_name: Option<String>,
    pub properties_json: String,
    pub request_ref: String,
    pub updated_at: String,
}

pub async fn rules(pool: &SqlitePool) -> AppResult<Vec<SaveRule>> {
    let rows: Vec<String> =
        sqlx::query_scalar("SELECT rule_json FROM business_save_rules ORDER BY request_ref")
            .fetch_all(pool)
            .await?;
    rows.into_iter()
        .map(|s| serde_json::from_str(&s).map_err(|e| AppError::msg(e.to_string())))
        .collect()
}
pub async fn rule(pool: &SqlitePool, request: &str) -> AppResult<Option<SaveRule>> {
    let raw: Option<String> =
        sqlx::query_scalar("SELECT rule_json FROM business_save_rules WHERE request_ref=?")
            .bind(request)
            .fetch_optional(pool)
            .await?;
    raw.map(|s| serde_json::from_str(&s).map_err(|e| AppError::msg(e.to_string())))
        .transpose()
}
pub async fn validate(pool: &SqlitePool, r: &SaveRule) -> AppResult<()> {
    let request = db::get_protocol(pool, &r.request).await?;
    let response = db::get_protocol(pool, &r.response).await?;
    if request.system_ref.is_none() || request.system_ref != response.system_ref {
        return Err(AppError::msg("请求与应答须属于同一系统"));
    }
    if request.msg_type != MsgType::Request {
        return Err(AppError::msg("保存规则须配置在请求协议上"));
    }
    if response.msg_type != MsgType::Response {
        return Err(AppError::msg("保存规则须选择应答类型协议"));
    }
    crate::record_types::check_rule(pool, request.system_ref.unwrap(), r).await?;
    business::validate(
        r,
        &devices::fields_for(pool, &request).await?,
        &devices::fields_for(pool, &response).await?,
    )
}
pub async fn save(pool: &SqlitePool, r: &SaveRule) -> AppResult<()> {
    validate(pool, r).await?;
    sqlx::query("INSERT INTO business_save_rules(request_ref,response_ref,rule_json) VALUES(?,?,?) ON CONFLICT(request_ref) DO UPDATE SET response_ref=excluded.response_ref,rule_json=excluded.rule_json")
        .bind(&r.request).bind(&r.response).bind(serde_json::to_string(r)?).execute(pool).await?;
    Ok(())
}
#[tauri::command]
pub async fn list_save_rules(state: tauri::State<'_, AppState>) -> AppResult<Vec<SaveRule>> {
    rules(&state.pool).await
}
#[tauri::command]
pub async fn save_business_rule(
    state: tauri::State<'_, AppState>,
    request: String,
    rule: Option<SaveRule>,
) -> AppResult<()> {
    let mut pending = state.engine.pending_saves.lock().await;
    if let Some(r) = rule {
        if r.request != request {
            return Err(AppError::msg("请求标识不一致"));
        }
        save(&state.pool, &r).await?;
    } else {
        sqlx::query("DELETE FROM business_save_rules WHERE request_ref=?")
            .bind(&request)
            .execute(&state.pool)
            .await?;
    }
    pending.retain(|_, p| p.rule.request != request);
    Ok(())
}
#[tauri::command]
pub async fn list_business_records(
    state: tauri::State<'_, AppState>,
) -> AppResult<Vec<BusinessRecord>> {
    Ok(sqlx::query_as("SELECT b.*,t.name AS device_name FROM business_records b LEFT JOIN terminal_profiles t ON t.id=b.device_ref ORDER BY b.updated_at DESC,b.id DESC").fetch_all(&state.pool).await?)
}
#[tauri::command]
pub async fn delete_business_record(state: tauri::State<'_, AppState>, id: i64) -> AppResult<()> {
    let kind: Option<String> = sqlx::query_scalar("SELECT kind FROM business_records WHERE id=?")
        .bind(id)
        .fetch_optional(&state.pool)
        .await?;
    if kind.as_deref() == Some("device") {
        return Err(AppError::msg("设备请在设备档案中删除；先删除关联资料"));
    }
    sqlx::query("DELETE FROM business_records WHERE id=?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(())
}

fn pending_key(node: i64, system: i64, response: &str, peer: SocketAddr) -> String {
    format!("{node}|{system}|{response}|{peer}")
}

pub async fn track(
    pool: &SqlitePool,
    engine: &EngineState,
    node: &Node,
    request: &Protocol,
    fields: &[ProtocolField],
    data: &[u8],
    dicts: &DictMap,
    peer: SocketAddr,
) -> AppResult<Option<String>> {
    let Some(r) = rule(pool, &request.protocol_id).await? else {
        return Ok(None);
    };
    validate(pool, &r).await?;
    let rows = business::snapshot(fields, data, dicts, &r.request_array)?;
    // Validate every mapped request property and uniqueness before a packet leaves.
    let mut keys = std::collections::HashSet::new();
    for row in &rows {
        if !keys.insert(business::row_identity(&r, row)?) {
            return Err(AppError::msg("资料唯一标识重复，请修改后发送"));
        }
        for m in r.mappings.iter().filter(|m| m.source == "request" && r.operation != "delete") {
            business::get(row, &m.field)?;
        }
    }
    let root = business::snapshot(fields, data, dicts, "")?;
    let sequence = if r.serial {
        String::new()
    } else {
        business::get(&root[0], &r.request_sequence)?.hex.clone()
    };
    let response = db::get_protocol(pool, &r.response).await?;
    let schema = serde_json::to_string(&devices::fields_for(pool, &response).await?)?;
    let key = pending_key(node.id, request.system_ref.unwrap_or(-1), &r.response, peer);
    let record_type = crate::record_types::explicit(pool, request.system_ref.unwrap_or(-1), &r.kind).await?;
    let mut pending = engine.pending_saves.lock().await;
    if let Some(old) = pending.get(&key) {
        if old.sent_at.elapsed() >= Duration::from_millis(old.rule.timeout_ms) {
            return Err(AppError::msg(
                "上一笔保存请求已超时；请停止并重新启动该通道监听后重试，避免迟到应答误关联",
            ));
        }
        return Err(AppError::msg(
            "此通道和对端仍有一笔保存请求等待应答，请勿重复发送",
        ));
    }
    if pending.len() >= 2048 {
        return Err(AppError::msg("待保存请求过多"));
    }
    if !r.serial {
        let mut used = engine.used_save_sequences.lock().await;
        if used.len() >= 65536 {
            return Err(AppError::msg("保存请求流水号缓存已满，请重启监听"));
        }
        if !used.insert(format!("{key}|{sequence}")) {
            return Err(AppError::msg(
                "该流水号本次监听已使用，请使用新的流水号，避免重复应答误入库",
            ));
        }
    }
    pending.insert(
        key.clone(),
        PendingSave {
            rule: r,
            rows,
            sequence,
            sent_at: Instant::now(),
            response_schema: schema,
            record_type,
        },
    );
    Ok(Some(key))
}

pub async fn accept(
    pool: &SqlitePool,
    engine: &EngineState,
    node: &Node,
    response: &Protocol,
    fields: &[ProtocolField],
    data: &[u8],
    dicts: &DictMap,
    peer: SocketAddr,
) -> AppResult<String> {
    let key = pending_key(
        node.id,
        response.system_ref.unwrap_or(-1),
        &response.protocol_id,
        peer,
    );
    let mut pending = engine.pending_saves.lock().await;
    let Some(p) = pending.get(&key) else {
        return Ok(String::new());
    };
    if p.sent_at.elapsed() >= Duration::from_millis(p.rule.timeout_ms) {
        return Ok("｜保存请求已超时，未保存".into());
    }
    if rule(pool, &p.rule.request).await?.as_ref() != Some(&p.rule)
        || serde_json::to_string(fields)? != p.response_schema
        || crate::record_types::explicit(pool, response.system_ref.unwrap_or(-1), &p.rule.kind).await? != p.record_type
    {
        pending.remove(&key);
        return Ok("｜协议、资料类型或保存规则已变更，未保存，请重新发送".into());
    }
    let root = business::snapshot(fields, data, dicts, "")?;
    if !p.rule.serial && business::get(&root[0], &p.rule.response_sequence)?.hex != p.sequence {
        return Ok("｜关联流水号不匹配，未保存".into());
    }
    let rows = business::snapshot(fields, data, dicts, &p.rule.response_array)?;
    let writes = business::evaluate(&p.rule, &p.rows, &rows)?;
    let mut applied = engine.applied_configs.lock().await;
    // Reserve the SQLite writer before reading properties: incoming ACK and OUT log writes can overlap.
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let system = response
        .system_ref
        .ok_or_else(|| AppError::msg("应答缺少系统"))?;
    let mut applied_keys = Vec::new();
    for write in &writes {
        let scope = if p.rule.key_scope == "device" { write.device.as_deref().unwrap_or("") } else { "" };
        let identity = (node.id, system, format!("{}:{scope}:{}", p.rule.kind, write.key));
        if applied.get(&identity).is_some_and(|t| *t >= p.sent_at) {
            continue;
        }
        let mut device_ref = None;
        if let Some(device) = &write.device {
            device_ref=sqlx::query_scalar::<_,i64>("SELECT id FROM terminal_profiles WHERE node_ref=? AND system_ref=? AND device_key=?")
                .bind(node.id).bind(system).bind(device).fetch_optional(&mut *tx).await?;
            if device_ref.is_none() {
                return Err(AppError::msg(format!(
                    "所属设备 {device} 不存在于本通道和系统，整笔未保存"
                )));
            }
        }
        if p.rule.operation == "delete" {
            // A channel-wide identifier may have a parent too; do not delete another device's record.
            let existing: Option<Option<i64>> = sqlx::query_scalar("SELECT device_ref FROM business_records WHERE node_ref=? AND system_ref=? AND kind=? AND scope_key=? AND record_key=?")
                .bind(node.id).bind(system).bind(&p.rule.kind).bind(scope).bind(&write.key).fetch_optional(&mut *tx).await?;
            if existing.is_some() && write.device.is_some() && existing.flatten() != device_ref { return Err(AppError::msg("资料所属设备不匹配，整笔未删除")); }
            if p.rule.kind == "device" {
                let id: Option<i64> = sqlx::query_scalar("SELECT id FROM terminal_profiles WHERE node_ref=? AND system_ref=? AND device_key=?").bind(node.id).bind(system).bind(&write.key).fetch_optional(&mut *tx).await?;
                if let Some(id) = id {
                    let linked: i64 = sqlx::query_scalar("SELECT count(*) FROM business_records WHERE device_ref=?").bind(id).fetch_one(&mut *tx).await?;
                    if linked > 0 { return Err(AppError::msg("设备仍有关联资料，整笔未删除；请先删除关联资料")); }
                    sqlx::query("DELETE FROM terminal_profiles WHERE id=?").bind(id).execute(&mut *tx).await?;
                    let raw: Option<String> = sqlx::query_scalar("SELECT captures_json FROM nodes WHERE id=?").bind(node.id).fetch_one(&mut *tx).await?;
                    let mut captures: serde_json::Value = serde_json::from_str(raw.as_deref().unwrap_or("{}"))?;
                    if let Some(map) = captures.as_object_mut() { map.remove(&format!("s{system}:{}", write.key)); }
                    sqlx::query("UPDATE nodes SET captures_json=? WHERE id=?").bind(captures.to_string()).bind(node.id).execute(&mut *tx).await?;
                }
            }
            sqlx::query("DELETE FROM business_records WHERE node_ref=? AND system_ref=? AND kind=? AND scope_key=? AND record_key=?")
                .bind(node.id).bind(system).bind(&p.rule.kind).bind(scope).bind(&write.key).execute(&mut *tx).await?;
            applied_keys.push(identity);
            continue;
        }
        let stored:Option<String>=sqlx::query_scalar("SELECT properties_json FROM business_records WHERE node_ref=? AND system_ref=? AND kind=? AND scope_key=? AND record_key=?")
            .bind(node.id).bind(system).bind(&p.rule.kind).bind(scope).bind(&write.key).fetch_optional(&mut *tx).await?;
        let mut properties: BTreeMap<String, business::Cell> = stored
            .as_deref()
            .map(serde_json::from_str)
            .transpose()?
            .unwrap_or_default();
        properties.extend(write.properties.clone());
        if p.rule.kind == "device" {
            let old:Option<String>=sqlx::query_scalar("SELECT fields_json FROM terminal_profiles WHERE node_ref=? AND system_ref=? AND device_key=?")
                .bind(node.id).bind(system).bind(&write.key).fetch_optional(&mut *tx).await?;
            let mut values: BTreeMap<String, String> = old
                .as_deref()
                .map(serde_json::from_str)
                .transpose()?
                .unwrap_or_default();
            values.extend(
                write
                    .properties
                    .iter()
                    .map(|(k, v)| (k.clone(), v.value.clone())),
            );
            sqlx::query("INSERT INTO terminal_profiles(name,node_ref,system_ref,protocol_ref,device_key,fields_json) VALUES(?,?,?,?,?,?) ON CONFLICT(node_ref,system_ref,device_key) DO UPDATE SET fields_json=excluded.fields_json,protocol_ref=excluded.protocol_ref")
                .bind(&write.name).bind(node.id).bind(system).bind(&p.rule.request).bind(&write.key).bind(serde_json::to_string(&values)?).execute(&mut *tx).await?;
        }
        sqlx::query("INSERT INTO business_records(node_ref,system_ref,kind,scope_key,record_key,name,device_ref,properties_json,request_ref) VALUES(?,?,?,?,?,?,?,?,?) ON CONFLICT(node_ref,system_ref,kind,scope_key,record_key) DO UPDATE SET name=excluded.name,device_ref=excluded.device_ref,properties_json=excluded.properties_json,request_ref=excluded.request_ref,updated_at=datetime('now','localtime')")
            .bind(node.id).bind(system).bind(&p.rule.kind).bind(scope).bind(&write.key).bind(&write.name).bind(device_ref).bind(serde_json::to_string(&properties)?).bind(&p.rule.request).execute(&mut *tx).await?;
        applied_keys.push(identity);
    }
    tx.commit().await?;
    let saved = applied_keys.len();
    for key in applied_keys {
        applied.insert(key, p.sent_at);
    }
    let total = p.rows.len();
    let action = if p.rule.operation == "delete" { "删除处理完成" } else { "已保存" };
    pending.remove(&key);
    Ok(format!("｜业务资料{action} {saved}/{total} 条"))
}
