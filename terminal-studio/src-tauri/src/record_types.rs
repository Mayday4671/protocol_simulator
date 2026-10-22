//! System-scoped record definitions. Wire encoding stays in protocol fields.
use crate::engine::state::AppState;
use simterm_core::{business::{RecordType, SaveRule}, error::{AppError, AppResult}};
use sqlx::SqlitePool;
use std::collections::BTreeMap;
use tauri::Emitter;

pub async fn explicit(pool: &SqlitePool, system: i64, kind: &str) -> AppResult<Option<RecordType>> {
    let raw: Option<String> = sqlx::query_scalar("SELECT definition_json FROM business_record_types WHERE system_ref=? AND kind=?")
        .bind(system).bind(kind).fetch_optional(pool).await?;
    raw.map(|s| serde_json::from_str(&s).map_err(Into::into)).transpose()
}

pub async fn list(pool: &SqlitePool, system: i64) -> AppResult<Vec<RecordType>> {
    let mut types = BTreeMap::new();
    // Virtual defaults and inferred legacy types keep old configurations usable without rewriting them.
    for (kind, name, required) in [("device", "设备档案", false)] {
        types.insert(kind.to_owned(), RecordType { kind: kind.into(), name: name.into(), device_required: required, key_scope: "channel".into(), properties: vec![] });
    }
    let rules: Vec<String> = sqlx::query_scalar("SELECT r.rule_json FROM business_save_rules r JOIN protocols p ON p.protocol_id=r.request_ref WHERE p.system_ref=?").bind(system).fetch_all(pool).await?;
    for raw in rules {
        let r: SaveRule = serde_json::from_str(&raw)?;
        let t = types.entry(r.kind.clone()).or_insert(RecordType { kind: r.kind.clone(), name: if r.kind == "phone" { "电话资料".into() } else { r.kind.clone() }, device_required: !r.device_field.is_empty(), key_scope: r.key_scope.clone(), properties: vec![] });
        // Legacy kinds could be used by both linked and unlinked requests.
        t.device_required &= !r.device_field.is_empty();
        for m in r.mappings { if !t.properties.contains(&m.property) { t.properties.push(m.property); } }
    }
    let records: Vec<(String, String, Option<i64>, String)> = sqlx::query_as("SELECT kind,properties_json,device_ref,scope_key FROM business_records WHERE system_ref=?").bind(system).fetch_all(pool).await?;
    for (kind, raw, device, scope) in records {
        let t = types.entry(kind.clone()).or_insert(RecordType { kind: kind.clone(), name: kind, device_required: device.is_some(), key_scope: if scope.is_empty() { "channel".into() } else { "device".into() }, properties: vec![] });
        t.device_required &= device.is_some();
        let props: BTreeMap<String, serde_json::Value> = serde_json::from_str(&raw)?;
        for key in props.into_keys() { if !t.properties.contains(&key) { t.properties.push(key); } }
    }
    let stored: Vec<String> = sqlx::query_scalar("SELECT definition_json FROM business_record_types WHERE system_ref=?").bind(system).fetch_all(pool).await?;
    for raw in stored { let t: RecordType = serde_json::from_str(&raw)?; types.insert(t.kind.clone(), t); }
    Ok(types.into_values().collect())
}

pub async fn check_rule(pool: &SqlitePool, system: i64, rule: &SaveRule) -> AppResult<()> {
    if let Some(t) = explicit(pool, system, &rule.kind).await? { t.check_rule(rule)?; }
    else if rule.kind == "phone" && rule.device_field.is_empty() { return Err(AppError::msg("电话资料必须关联所属设备")); }
    Ok(())
}

pub async fn save(pool: &SqlitePool, system: i64, t: &RecordType) -> AppResult<()> {
    t.validate()?;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let old: Option<String> = sqlx::query_scalar("SELECT definition_json FROM business_record_types WHERE system_ref=? AND kind=?").bind(system).bind(&t.kind).fetch_optional(&mut *tx).await?;
    if let Some(raw) = old {
        let old: RecordType = serde_json::from_str(&raw)?;
        if old.device_required != t.device_required || old.key_scope != t.key_scope {
            let rules: i64 = sqlx::query_scalar("SELECT count(*) FROM business_save_rules r JOIN protocols p ON p.protocol_id=r.request_ref WHERE p.system_ref=? AND json_extract(r.rule_json,'$.kind')=?").bind(system).bind(&t.kind).fetch_one(&mut *tx).await?;
            let records: i64 = sqlx::query_scalar("SELECT count(*) FROM business_records WHERE system_ref=? AND kind=?").bind(system).bind(&t.kind).fetch_one(&mut *tx).await?;
            if rules > 0 || records > 0 { return Err(AppError::msg(format!("此类型被 {rules} 条规则和 {records} 条资料引用，不能修改设备关联要求或唯一范围；请新建类型或先解除引用"))); }
        }
    }
    let raw_rules: Vec<(String, String)> = sqlx::query_as("SELECT p.name,r.rule_json FROM business_save_rules r JOIN protocols p ON p.protocol_id=r.request_ref WHERE p.system_ref=?").bind(system).fetch_all(&mut *tx).await?;
    for (name, raw) in raw_rules {
        let r: SaveRule = serde_json::from_str(&raw)?;
        if r.kind == t.kind { t.check_rule(&r).map_err(|e| AppError::msg(format!("规则 [{name}] 仍引用此定义：{e}")))?; }
    }
    let records: Vec<(String, String, Option<i64>, String)> = sqlx::query_as("SELECT name,properties_json,device_ref,scope_key FROM business_records WHERE system_ref=? AND kind=?").bind(system).bind(&t.kind).fetch_all(&mut *tx).await?;
    for (name, raw, device, scope) in records {
        if (t.device_required && device.is_none()) || (t.key_scope == "device") != !scope.is_empty() { return Err(AppError::msg(format!("资料 [{name}] 的关联或唯一范围与新定义不一致"))); }
        let props: BTreeMap<String, serde_json::Value> = serde_json::from_str(&raw)?;
        if let Some(key) = props.keys().find(|p| !t.properties.contains(p)) { return Err(AppError::msg(format!("资料 [{name}] 仍有属性 [{key}]，不能移除其定义"))); }
    }
    sqlx::query("INSERT INTO business_record_types(system_ref,kind,definition_json) VALUES(?,?,?) ON CONFLICT(system_ref,kind) DO UPDATE SET definition_json=excluded.definition_json")
        .bind(system).bind(&t.kind).bind(serde_json::to_string(t)?).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn delete(pool: &SqlitePool, system: i64, kind: &str) -> AppResult<()> {
    if kind == "device" { return Err(AppError::msg("设备档案类型不可删除，可编辑属性或不使用")); }
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let rules: Vec<(String, String)> = sqlx::query_as("SELECT p.name,r.rule_json FROM business_save_rules r JOIN protocols p ON p.protocol_id=r.request_ref WHERE p.system_ref=?").bind(system).fetch_all(&mut *tx).await?;
    for (name, raw) in rules { if serde_json::from_str::<SaveRule>(&raw)?.kind == kind { return Err(AppError::msg(format!("请求 [{name}] 的应答保存规则正在使用此类型，请先关闭或修改规则"))); } }
    let records: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM business_records WHERE system_ref=? AND kind=?").bind(system).bind(kind).fetch_one(&mut *tx).await?;
    if records > 0 { return Err(AppError::msg(format!("此类型仍有 {records} 条资料，请先处理资料"))); }
    sqlx::query("DELETE FROM business_record_types WHERE system_ref=? AND kind=?").bind(system).bind(kind).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

#[tauri::command]
pub async fn list_record_types(state: tauri::State<'_, AppState>, system: i64) -> AppResult<Vec<RecordType>> { list(&state.pool, system).await }
#[tauri::command]
pub async fn save_record_type(app: tauri::AppHandle, state: tauri::State<'_, AppState>, system: i64, definition: RecordType) -> AppResult<()> {
    let _pending = state.engine.pending_saves.lock().await;
    save(&state.pool, system, &definition).await?;
    let _ = app.emit("record-types-updated", ());
    Ok(())
}
#[tauri::command]
pub async fn delete_record_type(app: tauri::AppHandle, state: tauri::State<'_, AppState>, system: i64, kind: String) -> AppResult<()> {
    let _pending = state.engine.pending_saves.lock().await;
    delete(&state.pool, system, &kind).await?;
    let _ = app.emit("record-types-updated", ());
    Ok(())
}

pub async fn export(pool: &SqlitePool, systems: &[i64], rules: &[SaveRule]) -> AppResult<Vec<simterm_core::xml::RecordTypeXml>> {
    let mut out = vec![];
    for system in systems {
        let name: String = sqlx::query_scalar("SELECT name FROM protocol_systems WHERE id=?").bind(system).fetch_one(pool).await?;
        let explicit_kinds: Vec<String> = sqlx::query_scalar("SELECT kind FROM business_record_types WHERE system_ref=?").bind(system).fetch_all(pool).await?;
        for t in list(pool, *system).await? {
            if explicit_kinds.contains(&t.kind) || rules.iter().any(|r| r.kind == t.kind) {
                out.push(simterm_core::xml::RecordTypeXml { system: name.clone(), definition: t });
            }
        }
    }
    Ok(out)
}
