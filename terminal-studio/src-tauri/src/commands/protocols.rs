//! 协议管理 Commands：CRUD + Markdown 导入/导出

use tauri::{AppHandle, State};

use crate::db;
use crate::engine::heartbeat;
use simterm_core::rules;
use crate::engine::state::AppState;
use simterm_core::error::{AppError, AppResult};
use simterm_core::xml::{export_protocols_xml, parse_protocols_xml, GroupHeaderMd};
use simterm_core::models::{
    DefMode, DictSetMd, FieldEnc, GroupHeaderPayload, GroupHeadersPayload, HeaderDefinition, GroupPayload, ImportResult, Protocol,
    ProtocolGroup, ProtocolMd, ProtocolPayload, ProtocolSystem, SystemPayload,
};

#[tauri::command]
pub async fn list_protocols(state: State<'_, AppState>) -> AppResult<Vec<Protocol>> {
    db::list_protocols(&state.pool).await
}

/// 协议保存公共路径: 载荷校验 + 字典绑定校验 + 落库(流程内嵌信令等内部生成方复用)
pub async fn validate_and_upsert(
    pool: &sqlx::SqlitePool,
    payload: &ProtocolPayload,
) -> AppResult<()> {
    let mut payload = payload.clone();
    // 列表字段长度 = 单条子字段之和(自动推导; 引擎按此口径渲染/解析, 手误不一致直接归一不再报错)
    for f in payload.fields.iter_mut() {
        if !f.unit_fields.is_empty() {
            let sum: u32 = f.unit_fields.iter().map(|u| u.length).sum();
            if sum > 0 {
                f.length = sum;
            }
        }
    }
    let payload = &payload;
    validate_protocol_payload(payload)?;
    validate_field_dicts(pool, payload).await?;
    db::upsert_protocol(pool, payload).await?;
    Ok(())
}

#[tauri::command]
pub async fn upsert_protocol(
    state: State<'_, AppState>,
    payload: ProtocolPayload,
) -> AppResult<Protocol> {
    validate_protocol_payload(&payload)?;
    validate_field_dicts(&state.pool, &payload).await?;
    db::upsert_protocol(&state.pool, &payload).await
}

#[tauri::command]
pub async fn delete_protocol(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> AppResult<()> {
    // 先停掉引用该协议的心跳任务, 再删行(级联删绑定)
    if let Ok(p) = sqlx_get_protocol_by_pk(&state.pool, id).await {
        heartbeat::stop_protocol_heartbeats(&app, &state.engine, &p.protocol_id);
    }
    db::delete_protocol(&state.pool, id).await
}

async fn sqlx_get_protocol_by_pk(
    pool: &sqlx::SqlitePool,
    id: i64,
) -> AppResult<Protocol> {
    sqlx::query_as::<_, Protocol>("SELECT p.*, s.name AS system_name, g.name AS group_name FROM protocols p LEFT JOIN protocol_systems s ON s.id = p.system_ref LEFT JOIN protocol_groups g ON g.id = p.group_ref WHERE p.id=?")
        .bind(id)
        .fetch_one(pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => AppError::msg(format!("协议不存在: id={id}")),
            other => other.into(),
        })
}

pub(crate) fn validate_protocol_payload(p: &ProtocolPayload) -> AppResult<()> {
    if p.system_ref.is_none() {
        return Err(AppError::msg("请选择归属系统（导入协议必须包含系统名称）"));
    }
    if p.protocol_id.trim().is_empty() {
        return Err(AppError::msg("ProtocolID 不能为空"));
    }
    if p.name.trim().is_empty() {
        return Err(AppError::msg("协议名称不能为空"));
    }
    if p.def_mode == DefMode::Fields {
        // 字段表可为空: 纯公共头报文（整帧由系统公共头构成）
        for (i, f) in p.fields.iter().enumerate() {
            if f.name.trim().is_empty() {
                return Err(AppError::msg(format!("第 {} 个字段缺少字段名", i + 1)));
            }
            if f.length == 0 || f.length > 65535 {
                return Err(AppError::msg(format!(
                    "字段 [{}] 长度 {} 非法 (需 1-65535)",
                    f.name, f.length
                )));
            }
            // 字面 hex 才做长度校验: 空值(发送时按声明长度补 0)、含 {变量} 的值
            // (渲染时才知道结果)、绑定字典的值(可写条目含义)都不在此列
            let is_literal_hex = f.enc == FieldEnc::Hex
                && f.dict_key().is_none()
                && !f.value.trim().is_empty()
                && !f.value.contains('{');
            if is_literal_hex {
                let compact: String = f.value.chars().filter(|c| !c.is_whitespace()).collect();
                match hex::decode(&compact) {
                    Ok(b) if b.len() == f.length as usize
                        || (simterm_core::fields::is_variable_len(f)
                            && !f.unit_fields.is_empty()
                            && b.len() % f.length as usize == 0) => {}
                    Ok(b) => {
                        return Err(AppError::msg(format!(
                            "字段 [{}] hex 值需 {} 字节, 实际 {} 字节（值: \"{}\"）",
                            f.name,
                            f.length,
                            b.len(),
                            &f.value[..f.value.len().min(30)]
                        )))
                    }
                    Err(_) => {
                        return Err(AppError::msg(format!(
                            "字段 [{}] 的值不是合法十六进制",
                            f.name
                        )))
                    }
                }
            }
        }
    } else if p.payload_template.trim().is_empty() {
        return Err(AppError::msg("Payload 模板不能为空"));
    }
    // 字段断言匹配条件: field/value 必须成对非空（协议帧布局校验在保存后的运行时进行）
    for (i, c) in p.match_fields.iter().enumerate() {
        if c.field.trim().is_empty() || c.value.trim().is_empty() {
            return Err(AppError::msg(format!(
                "第 {} 条匹配条件缺少字段名或期望值",
                i + 1
            )));
        }
    }
    if let Some(rule) = p.match_rule.as_deref() {
        let rule = rule.trim();
        if !rule.is_empty() {
            rules::validate_rule(rule).map_err(AppError::msg)?;
        }
    }
    Ok(())
}

/// E2E 自检: 用界面保存同一条校验路径逐协议体检(空手打开-保存是否会被拦)
#[cfg(feature = "e2e")]
pub async fn ProtocolPayloadForTest() {}

#[cfg(feature = "e2e")]
pub async fn validate_all_protocols(pool: &sqlx::SqlitePool) -> Vec<String> {
    let mut errs = Vec::new();
    let all = db::list_protocols(pool).await.unwrap_or_default();
    for p in &all {
        let fields: Vec<simterm_core::models::ProtocolField> =
            serde_json::from_str(p.fields_json.as_deref().unwrap_or("[]")).unwrap_or_default();
        let conds: Vec<simterm_core::models::MatchCond> =
            serde_json::from_str(p.match_fields_json.as_deref().unwrap_or("[]")).unwrap_or_default();
        let payload = ProtocolPayload {
            header_refs: match p.header_refs() { Ok(refs) => refs, Err(e) => { errs.push(format!("{}: {e}", p.name)); continue; } },
            protocol_id: p.protocol_id.clone(),
            name: p.name.clone(),
            format: p.format,
            msg_type: p.msg_type,
            def_mode: p.def_mode,
            fields,
            payload_template: p.payload_template.clone(),
            match_rule: p.match_rule.clone(),
            match_fields: conds,
            auto_respond: p.auto_respond.clone(),
            respond_delay_ms: p.respond_delay_ms,
            auto_periodic: p.auto_periodic.clone(),
            auto_periodic_interval_ms: p.auto_periodic_interval_ms,
            description: p.description.clone(),
            system_ref: p.system_ref,
            group_ref: p.group_ref,
        };
        if let Err(e) = validate_protocol_payload(&payload) {
            errs.push(format!("{}: {e}", p.name));
            continue;
        }
        if let Err(e) = validate_field_dicts(pool, &payload).await {
            errs.push(format!("{}: {e}", p.name));
        }
    }
    errs
}

/// 校验字段的字典绑定与公共头覆盖标记:
/// 字典在「全局共享 + 本协议所属分组」作用域内存在、长度一致、取值属于该字典;
/// hdr_ref 指向的头字段在分组公共头中存在。
///
/// 保存时就拦住, 而不是等到发送报文那一刻才报错。
async fn validate_field_dicts(pool: &sqlx::SqlitePool, p: &ProtocolPayload) -> AppResult<()> {
    let dicts = db::dict_map_for_group(pool, p.group_ref).await?;
    let headers = db::headers_for_selection(pool, p.group_ref, p.header_refs.as_deref()).await?;
    validate_resolved_field_dicts(&dicts, &headers, p)
}

fn validate_resolved_field_dicts(dicts: &simterm_core::render::DictMap, headers: &[simterm_core::models::ProtocolField], p: &ProtocolPayload) -> AppResult<()> {
    let (head, body) = simterm_core::fields::merge_header_specs(headers, &p.fields)?;
    for f in head.iter().chain(body.iter()) {
        let Some(key) = f.dict_key() else { continue };
        let set = dicts.get(key).ok_or_else(|| {
            AppError::msg(format!(
                "字段 [{}] 绑定的字典 \"{key}\" 在当前作用域内不存在（可用范围: 全局共享字典 + 本协议所属分组的字典）",
                f.name
            ))
        })?;
        if set.byte_len != f.length as usize {
            return Err(AppError::msg(format!(
                "字段 [{}] 长度 {} 字节与字典「{}」的 {} 字节不一致",
                f.name, f.length, set.name, set.byte_len
            )));
        }
        // {req:x}/{session} 等变量在渲染时才求值, 保存校验跳过(不属于任何条目是正常的)
        let v = f.value.trim();
        if !v.is_empty() && !v.contains('{') && set.find_value(v).is_none() {
            return Err(AppError::msg(format!(
                "字段 [{}] 的值 \"{v}\" 不属于字典「{}」, 可选: {}",
                f.name,
                set.name,
                set.options_hint()
            )));
        }
    }
    Ok(())
}

/// 读取 .md 文件批量导入协议（按 protocol_id upsert）
#[tauri::command]
pub async fn import_protocols_from_xml(
    state: State<'_, AppState>,
    file_path: String,
) -> AppResult<ImportResult> {
    let md_text = std::fs::read_to_string(&file_path)
        .map_err(|e| AppError::msg(format!("读取文件失败 {file_path}: {e}")))?;
    import_xml_text(&state, md_text).await
}

/// 在线编辑用: 直接以 XML 文本导入(与文件导入同语义)
#[tauri::command]
pub async fn import_protocols_xml_text(
    state: State<'_, AppState>,
    xml: String,
) -> AppResult<ImportResult> {
    import_xml_text(&state, xml).await
}

pub(crate) async fn import_xml_text(state: &AppState, md_text: String) -> AppResult<ImportResult> {
    let md = parse_protocols_xml(&md_text)?;
    // A rejected protocol cannot be silently omitted while its group's header changes are applied.
    if !md.failed.is_empty() { return Ok(ImportResult { failed: md.failed, ..Default::default() }); }
    let mut ids = std::collections::HashSet::new();
    for p in &md.protocols {
        if !ids.insert(p.protocol_id.clone()) { return Err(AppError::msg(format!("导入协议标识 [{}] 重复", p.protocol_id))); }
    }
    let mut result = ImportResult {
        failed: md.failed,
        ..Default::default()
    };

    // 字典先落库: 协议字段绑定校验依赖它们已存在
    for d in &md.dicts {
        match import_dict_set_xml(&state.pool, d).await {
            Ok(()) => result.dict_sets += 1,
            Err(e) => result.failed.push(format!("字典 [{}] 导入失败: {e}", d.key)),
        }
    }

    // Stage every named group against its final imported state, then commit that group atomically.
    let mut grouped: std::collections::BTreeMap<(String, String), Vec<&GroupHeaderMd>> = std::collections::BTreeMap::new();
    for header in &md.group_headers { grouped.entry((header.system.clone(), header.group.clone())).or_default().push(header); }
    for protocol in &md.protocols {
        if let (Some(system), Some(group)) = (&protocol.system, &protocol.group) {
            if !group.is_empty() { grouped.entry((system.clone(), group.clone())).or_default(); }
        }
    }
    let mut processed_groups = std::collections::HashSet::new();
    for ((system, group_name), incoming) in grouped {
        processed_groups.insert((system.clone(), group_name.clone()));
        let saved: AppResult<(usize, usize)> = async {
            let group = resolve_group_of_header(&state.pool, &system, &group_name).await?;
            let mut definitions = simterm_core::headers::group_definitions(&group)?;
            let mut enabled = None;
            let has_headers = !incoming.is_empty();
            if incoming.iter().any(|h| h.id.is_empty()) { definitions.clear(); }
            else {
                for header in incoming {
                    validate_group_header(&GroupHeaderPayload { enabled: header.enabled, fields: header.fields.clone() })?;
                    let definition = HeaderDefinition { id: header.id.clone(), name: header.name.clone(), fields: header.fields.clone() };
                    if definition.id == simterm_core::headers::LEGACY_HEADER_ID { enabled = Some(header.enabled); }
                    if let Some(old) = definitions.iter_mut().find(|h| h.id == definition.id) { *old = definition; } else { definitions.push(definition); }
                }
            }
            let candidate = if has_headers { db::group_with_headers(&group, &definitions, enabled)? } else { group.clone() };
            let dicts = db::dict_map_for_group(&state.pool, Some(group.id)).await?;
            let mut updates = Vec::new();
            let mut updated = 0usize;
            for p in md.protocols.iter().filter(|p| p.system.as_deref() == Some(system.as_str()) && p.group.as_deref() == Some(group_name.as_str())) {
                let mut payload = md_to_payload(p)?;
                payload.system_ref = Some(group.system_ref);
                payload.group_ref = Some(group.id);
                validate_protocol_payload(&payload)?;
                let selected = simterm_core::headers::resolve_headers(Some(&candidate), payload.header_refs.as_deref())?;
                validate_resolved_field_dicts(&dicts, &selected, &payload)?;
                if db::get_protocol(&state.pool, &payload.protocol_id).await.is_ok() { updated += 1; }
                updates.push(payload);
            }
            db::import_group_protocols(&state.pool, group.id, has_headers.then_some(definitions.as_slice()), enabled, &updates).await?;
            Ok((updates.len() - updated, updated))
        }.await;
        match saved {
            Ok((imported, updated)) => { result.imported += imported; result.updated += updated; },
            Err(e) => result.failed.push(format!("分组 [{system}/{group_name}] 导入失败，头库及协议未修改: {e}")),
        }
    }

    for p in md.protocols {
        if processed_groups.contains(&(p.system.clone().unwrap_or_default(), p.group.clone().unwrap_or_default())) { continue; }
        let mut payload = md_to_payload(&p)?;
        // 系统与分组按需解析/创建（分组依赖系统存在）
        if let Some(sys) = p.system.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            let system = db::resolve_system_by_name(&state.pool, sys).await?;
            payload.system_ref = Some(system.id);
            if let Some(g) = p.group.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                let group = db::resolve_group_by_name(&state.pool, system.id, g).await?;
                payload.group_ref = Some(group.id);
            }
        }
        // 校验（含正则合法性与字典绑定）
        if let Err(e) = validate_protocol_payload(&payload) {
            result.failed.push(format!("[{}] {e}", p.protocol_id));
            continue;
        }
        if let Err(e) = validate_field_dicts(&state.pool, &payload).await {
            result.failed.push(format!("[{}] {e}", p.protocol_id));
            continue;
        }
        let existed = db::get_protocol(&state.pool, &p.protocol_id).await.is_ok();
        match db::upsert_protocol(&state.pool, &payload).await {
            Ok(_) => {
                if existed {
                    result.updated += 1;
                } else {
                    result.imported += 1;
                }
            }
            Err(e) => result.failed.push(format!("[{}] 写入数据库失败: {e}", p.protocol_id)),
        }
    }

    // 业务流程落库(按 系统+流程名 upsert; 协议已在上面写入, 步骤引用按 protocol_id 原样保留)
    for f in &md.flows {
        let system = match db::resolve_system_by_name(&state.pool, &f.system).await {
            Ok(s) => s,
            Err(e) => {
                result.failed.push(format!("流程 [{}] 系统解析失败: {e}", f.name));
                continue;
            }
        };
        let steps: Vec<serde_json::Value> = f
            .steps
            .iter()
            .enumerate()
            .map(|(idx, st)| {
                let (x, y) = if st.x == 0.0 && st.y == 0.0 {
                    (360.0, 60.0 + idx as f64 * 175.0)
                } else {
                    (st.x, st.y)
                };
                let params: serde_json::Map<String, serde_json::Value> = st
                    .params
                    .iter()
                    .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                    .collect();
                serde_json::json!({
                    "id": st.id, "name": st.name, "x": x, "y": y,
                    "protocol_id": st.protocol_id, "terminal_mode": st.terminal_mode,
                    "terminal_id": st.terminal_id, "params": params,
                    "expect_protocol": st.expect_protocol, "timeout_ms": st.timeout_ms,
                    "delay_ms": st.delay_ms, "next": st.next,
                    "on_success": st.on_success, "on_failure": st.on_failure, "on_timeout": st.on_timeout,
                })
            })
            .collect();
        let steps_json = serde_json::to_string(&steps)
            .map_err(|e| AppError::msg(format!("流程步骤序列化失败: {e}")))?;
        let trigger_json = if f.trigger_protocols.is_empty() {
            None
        } else {
            serde_json::to_string(&f.trigger_protocols).ok()
        };
        let existing: Option<i64> = sqlx::query_scalar(
            "SELECT id FROM business_flows WHERE name=? AND system_ref=?",
        )
        .bind(&f.name)
        .bind(system.id)
        .fetch_optional(&state.pool)
        .await?;
        let res = match existing {
            Some(id) => {
                sqlx::query("UPDATE business_flows SET description=?,steps_json=?,trigger_protocols_json=?,schedule_interval_ms=?,updated_at=datetime('now','localtime') WHERE id=?")
                    .bind(&f.description)
                    .bind(&steps_json)
                    .bind(&trigger_json)
                    .bind(f.schedule_interval_ms)
                    .bind(id)
                    .execute(&state.pool)
                    .await
            }
            None => {
                sqlx::query("INSERT INTO business_flows(name,system_ref,description,steps_json,trigger_protocols_json,schedule_interval_ms) VALUES(?,?,?,?,?,?)")
                    .bind(&f.name)
                    .bind(system.id)
                    .bind(&f.description)
                    .bind(&steps_json)
                    .bind(&trigger_json)
                    .bind(f.schedule_interval_ms)
                    .execute(&state.pool)
                    .await
            }
        };
        if let Err(e) = res {
            result.failed.push(format!("流程 [{}] 写入失败: {e}", f.name));
        }
    }
    Ok(result)
}

/// 按 系统+分组 名解析分组（导入公共头用, 不存在则创建）
async fn resolve_group_of_header(
    pool: &sqlx::SqlitePool,
    system: &str,
    group: &str,
) -> AppResult<ProtocolGroup> {
    let s = db::resolve_system_by_name(pool, system).await?;
    db::resolve_group_by_name(pool, s.id, group).await
}

/// 落库一张模板中定义的字典（按 Key + 归属分组 upsert, 条目整体替换）
async fn import_dict_set_xml(pool: &sqlx::SqlitePool, d: &DictSetMd) -> AppResult<()> {
    if d.items.is_empty() {
        return Err(AppError::msg("字典没有任何条目"));
    }
    // dict 挂在 group 内 → 分组专属; 仅 system 或根级 → 全局共享
    let group_ref = match (
        d.system.as_deref().map(str::trim).filter(|s| !s.is_empty()),
        d.group.as_deref().map(str::trim).filter(|s| !s.is_empty()),
    ) {
        (Some(sys_name), Some(g)) => Some(resolve_group_of_header(pool, sys_name, g).await?.id),
        (None, Some(_)) => return Err(AppError::msg("分组字典必须包含归属系统")),
        _ => None,
    };
    db::import_dict_set(pool, &d.key, &d.name, group_ref, d.byte_len(), &d.items).await?;
    Ok(())
}

/// 收集这批协议引用到的字典, 供导出时一并写入（换机导入后绑定不丢）
async fn collect_referenced_dicts(
    pool: &sqlx::SqlitePool,
    protocols: &[Protocol],
) -> AppResult<Vec<DictSetMd>> {
    // 以 set id 去重, BTreeMap 保证导出顺序稳定
    let mut out: std::collections::BTreeMap<i64, DictSetMd> = std::collections::BTreeMap::new();
    for p in protocols {
        let mut fields = p.field_specs();
        if let Some(gid) = p.group_ref {
            for header in simterm_core::headers::group_definitions(&db::get_group(pool, gid).await?)? { fields.extend(header.fields); }
        }
        fn dict_keys(fields: &[simterm_core::models::ProtocolField], keys: &mut Vec<String>) {
            for field in fields {
                if let Some(key) = field.dict_key() { keys.push(key.to_string()); }
                dict_keys(&field.unit_fields, keys);
            }
        }
        let mut keys = Vec::new();
        dict_keys(&fields, &mut keys);
        for key in keys {
            // 与渲染时一致: 分组专属优先, 回落到全局共享字典
            let found = match db::find_dict_set(pool, &key, p.group_ref).await? {
                Some(s) => Some(s),
                None => db::find_dict_set(pool, &key, None).await?,
            };
            let Some(set) = found else { continue };
            if out.contains_key(&set.id) {
                continue;
            }
            // 分组名(及其所属系统名)经分组链路取回
            let (system, group) = match set.group_ref {
                Some(gid) => match db::get_group(pool, gid).await {
                    Ok(g) => (
                        db::get_system(pool, g.system_ref).await.ok().map(|s| s.name),
                        Some(g.name),
                    ),
                    Err(_) => (None, None),
                },
                None => (None, None),
            };
            let items = db::list_dict_items(pool, set.id)
                .await?
                .into_iter()
                .map(|i| (i.value, i.label))
                .collect();
            out.insert(
                set.id,
                DictSetMd {
                    key: set.key,
                    name: set.name,
                    system,
                    group,
                    items,
                },
            );
        }
    }
    Ok(out.into_values().collect())
}

fn md_to_payload(p: &ProtocolMd) -> AppResult<ProtocolPayload> {
    Ok(ProtocolPayload {
        header_refs: p.header_refs.clone(),
        protocol_id: p.protocol_id.trim().to_string(),
        name: p.name.trim().to_string(),
        format: p.format,
        msg_type: p.msg_type,
        def_mode: p.def_mode,
        fields: p.fields.clone(),
        payload_template: p.payload_template.clone(),
        match_rule: p.match_rule.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string),
        match_fields: p.match_fields.clone(),
        auto_respond: p.auto_respond.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string),
        respond_delay_ms: p.respond_delay_ms,
        auto_periodic: p.auto_periodic.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string),
        auto_periodic_interval_ms: p.auto_periodic_interval_ms,
        description: p.description.clone().filter(|s| !s.trim().is_empty()),
        system_ref: None,
        group_ref: None,
    })
}

// ==================== 协议系统（三级分层第一层） ====================

#[tauri::command]
pub async fn list_systems(state: State<'_, AppState>) -> AppResult<Vec<ProtocolSystem>> {
    db::list_systems(&state.pool).await
}

#[tauri::command]
pub async fn create_system(
    state: State<'_, AppState>,
    payload: SystemPayload,
) -> AppResult<ProtocolSystem> {
    if payload.name.trim().is_empty() {
        return Err(AppError::msg("系统名称不能为空"));
    }
    db::resolve_system_by_name(&state.pool, payload.name.trim()).await
}

#[tauri::command]
pub async fn update_system(
    state: State<'_, AppState>,
    id: i64,
    payload: SystemPayload,
) -> AppResult<ProtocolSystem> {
    if payload.name.trim().is_empty() {
        return Err(AppError::msg("系统名称不能为空"));
    }
    db::update_system(&state.pool, id, &payload).await
}

#[tauri::command]
pub async fn delete_system(state: State<'_, AppState>, id: i64) -> AppResult<()> {
    db::delete_system(&state.pool, id).await
}

/// 将指定（或全部）协议导出为 .md 文件，返回导出条数
#[tauri::command]
pub async fn export_protocols_to_xml(
    state: State<'_, AppState>,
    protocol_ids: Vec<String>,
    file_path: String,
) -> AppResult<u64> {
    let all = db::list_protocols(&state.pool).await?;
    let selected: Vec<Protocol> = if protocol_ids.is_empty() {
        all
    } else {
        all.into_iter()
            .filter(|p| protocol_ids.contains(&p.protocol_id))
            .collect()
    };
    if selected.is_empty() {
        return Err(AppError::msg("没有可导出的协议"));
    }
    // 引用到的字典随文件一起导出, 换机导入后字段绑定仍然有效
    let dicts = collect_referenced_dicts(&state.pool, &selected).await?;
    // 选中协议所属分组、且已启用的公共头一并导出
    let headers = collect_enabled_headers(&state.pool, &selected).await?;
    let flows = collect_flows(&state.pool, &selected).await?;
    let md_text = export_protocols_xml(&selected, &dicts, &headers, &flows);
    std::fs::write(&file_path, md_text)
        .map_err(|e| AppError::msg(format!("写入文件失败 {file_path}: {e}")))?;
    Ok(selected.len() as u64)
}

/// 在线编辑用: 导出为 XML 文本(与文件导出同内容, 含引用字典与分组公共头)
#[tauri::command]
pub async fn export_protocols_xml_text(
    state: State<'_, AppState>,
    protocol_ids: Vec<String>,
) -> AppResult<String> {
    let all = db::list_protocols(&state.pool).await?;
    let selected: Vec<Protocol> = if protocol_ids.is_empty() {
        all
    } else {
        all.into_iter()
            .filter(|p| protocol_ids.contains(&p.protocol_id))
            .collect()
    };
    if selected.is_empty() {
        return Err(AppError::msg("没有可导出的协议"));
    }
    let dicts = collect_referenced_dicts(&state.pool, &selected).await?;
    let headers = collect_enabled_headers(&state.pool, &selected).await?;
    let flows = collect_flows(&state.pool, &selected).await?;
    Ok(export_protocols_xml(&selected, &dicts, &headers, &flows))
}

/// 收集业务流程(选中协议所属系统 + 全部流程一并导出, 换机不丢编排)
async fn collect_flows(pool: &sqlx::SqlitePool, protocols: &[Protocol]) -> AppResult<Vec<simterm_core::xml::FlowMd>> {
    let mut system_ids: Vec<i64> = Vec::new();
    for p in protocols {
        if let Some(sid) = p.system_ref {
            if !system_ids.contains(&sid) {
                system_ids.push(sid);
            }
        }
    }
    let systems = db::list_systems(pool).await?;
    let rows: Vec<(i64, String, Option<i64>, Option<i64>, Option<String>, String, Option<String>, i64)> = sqlx::query_as(
        "SELECT id,name,system_ref,group_ref,description,steps_json,trigger_protocols_json,schedule_interval_ms FROM business_flows ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    let mut out = Vec::new();
    for (_id, name, sys_ref, _group, description, steps_json, trigger_json, schedule) in rows {
        let Some(sid) = sys_ref else { continue };
        if !system_ids.contains(&sid) {
            continue;
        }
        let Some(sys_name) = systems.iter().find(|s| s.id == sid).map(|s| s.name.clone()) else { continue };
        let steps: Vec<serde_json::Value> = serde_json::from_str(&steps_json).unwrap_or_default();
        let mut md_steps = Vec::new();
        for st in steps {
            let get_s = |k: &str| st.get(k).and_then(|v| v.as_str()).map(str::to_string);
            let get_o = |k: &str| st.get(k).and_then(|v| v.as_str()).map(str::to_string).filter(|s| !s.is_empty());
            let mut params = Vec::new();
            if let Some(obj) = st.get("params").and_then(|v| v.as_object()) {
                for (k, v) in obj {
                    params.push((k.clone(), v.as_str().unwrap_or_default().to_string()));
                }
            }
            md_steps.push(simterm_core::xml::FlowStepMd {
                id: get_s("id").unwrap_or_default(),
                name: get_s("name").unwrap_or_default(),
                x: st.get("x").and_then(|v| v.as_f64()).unwrap_or(0.0),
                y: st.get("y").and_then(|v| v.as_f64()).unwrap_or(0.0),
                protocol_id: get_s("protocol_id").unwrap_or_default(),
                terminal_mode: get_s("terminal_mode").unwrap_or_else(|| "single".into()),
                terminal_id: st.get("terminal_id").and_then(|v| v.as_i64()),
                params,
                expect_protocol: get_o("expect_protocol"),
                timeout_ms: st.get("timeout_ms").and_then(|v| v.as_i64()).unwrap_or(3000),
                delay_ms: st.get("delay_ms").and_then(|v| v.as_i64()).unwrap_or(0),
                next: get_o("next"),
                on_success: get_o("on_success"),
                on_failure: get_o("on_failure"),
                on_timeout: get_o("on_timeout"),
            });
        }
        out.push(simterm_core::xml::FlowMd {
            system: sys_name,
            name,
            description,
            steps: md_steps,
            trigger_protocols: trigger_json
                .as_deref()
                .and_then(|t| serde_json::from_str(t).ok())
                .unwrap_or_default(),
            schedule_interval_ms: schedule,
        });
    }
    Ok(out)
}

/// 收集选中协议所属分组里已启用的公共头定义
async fn collect_enabled_headers(
    pool: &sqlx::SqlitePool,
    protocols: &[Protocol],
) -> AppResult<Vec<GroupHeaderMd>> {
    let mut group_ids: Vec<i64> = Vec::new();
    for p in protocols {
        db::protocol_headers(pool, p).await?;
        if let Some(gid) = p.group_ref {
            if !group_ids.contains(&gid) {
                group_ids.push(gid);
            }
        }
    }
    let mut out = Vec::new();
    for gid in group_ids {
        let g = db::get_group(pool, gid).await?;
        let system = db::get_system(pool, g.system_ref).await?.name;
        let headers = simterm_core::headers::group_definitions(&g)?;
        if headers.is_empty() && g.header_profiles_json.is_some() {
            out.push(GroupHeaderMd { system, group: g.name, id: String::new(), name: String::new(), enabled: false, fields: Vec::new() });
        } else {
            for header in headers {
                out.push(GroupHeaderMd { system: system.clone(), group: g.name.clone(), enabled: header.id != simterm_core::headers::LEGACY_HEADER_ID || g.header_enabled, id: header.id, name: header.name, fields: header.fields });
            }
        }
    }
    Ok(out)
}

// ==================== 协议分组（三级分层第二层） ====================

#[tauri::command]
pub async fn list_groups(state: State<'_, AppState>) -> AppResult<Vec<ProtocolGroup>> {
    db::list_groups(&state.pool).await
}

#[tauri::command]
pub async fn create_group(
    state: State<'_, AppState>,
    payload: GroupPayload,
) -> AppResult<ProtocolGroup> {
    if payload.name.trim().is_empty() {
        return Err(AppError::msg("分组名称不能为空"));
    }
    db::get_system(&state.pool, payload.system_ref).await?;
    db::resolve_group_by_name(&state.pool, payload.system_ref, payload.name.trim()).await
}

#[tauri::command]
pub async fn update_group(
    state: State<'_, AppState>,
    id: i64,
    payload: GroupPayload,
) -> AppResult<ProtocolGroup> {
    if payload.name.trim().is_empty() {
        return Err(AppError::msg("分组名称不能为空"));
    }
    db::get_system(&state.pool, payload.system_ref).await?;
    db::update_group(&state.pool, id, &payload).await
}

#[tauri::command]
pub async fn delete_group(state: State<'_, AppState>, id: i64) -> AppResult<()> {
    db::delete_group(&state.pool, id).await
}

/// 更新分组公共 UDP 头（开关 + 字段表）
#[tauri::command]
pub async fn update_group_header(
    state: State<'_, AppState>,
    id: i64,
    payload: GroupHeaderPayload,
) -> AppResult<ProtocolGroup> {
    if payload.enabled {
        validate_group_header(&payload)?;
    }
    db::update_group_header(&state.pool, id, &payload).await
}

/// Replace the group's ordered definition library without silently breaking existing protocols.
#[tauri::command]
pub async fn update_group_headers(state: State<'_, AppState>, id: i64, payload: GroupHeadersPayload) -> AppResult<ProtocolGroup> {
    simterm_core::headers::validate_definitions(&payload.headers)?;
    for header in &payload.headers {
        validate_group_header(&GroupHeaderPayload { enabled: true, fields: header.fields.clone() })?;
    }
    db::update_group_headers(&state.pool, id, &payload).await
}

/// 公共头字段合法性（启用时逐字段检查名称与长度）
fn validate_group_header(payload: &GroupHeaderPayload) -> AppResult<()> {
    for (i, f) in payload.fields.iter().enumerate() {
        if f.name.trim().is_empty() {
            return Err(AppError::msg(format!(
                "公共头第 {} 个字段缺少字段名",
                i + 1
            )));
        }
        if f.length == 0 || f.length > 65535 {
            return Err(AppError::msg(format!(
                "公共头字段 [{}] 长度 {} 非法",
                f.name, f.length
            )));
        }
    }
    Ok(())
}
