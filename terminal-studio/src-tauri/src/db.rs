use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    Row, SqlitePool,
};
use tauri::Manager;

use simterm_core::{
    error::{AppError, AppResult},
    models::{
        DefMode, DictItem, DictItemPayload, DictSet, DictSetPayload, DictSetView, Direction,
        GroupHeaderPayload, GroupHeadersPayload, HeaderDefinition, GroupPayload, HeartbeatBinding, HeartbeatPayload, HeartbeatView,
        LogFilter, LogRecord, LogStats, Node, NodePayload, Protocol, ProtocolGroup,
        ProtocolPayload, ProtocolSystem, ProtocolField, SystemPayload,
    },
    render::{DictMap, DictSetLite},
};

/// 初始化 SQLite 连接池并执行迁移（数据库文件位于应用数据目录 simterm.db）
pub async fn init_pool(app: &tauri::AppHandle) -> AppResult<SqlitePool> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::msg(format!("无法获取应用数据目录: {e}")))?;
    std::fs::create_dir_all(&data_dir)?;
    let db_path = std::env::var_os("SIMTERM_DB").map(std::path::PathBuf::from)
        .unwrap_or_else(|| data_dir.join("simterm.db"));

    let options = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}

// ==================== 节点 CRUD ====================

pub async fn list_nodes(pool: &SqlitePool) -> AppResult<Vec<Node>> {
    let rows = sqlx::query_as::<_, Node>(
        "SELECT * FROM nodes ORDER BY id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get_node(pool: &SqlitePool, id: i64) -> AppResult<Node> {
    sqlx::query_as::<_, Node>("SELECT * FROM nodes WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => AppError::msg(format!("节点不存在: id={id}")),
            other => other.into(),
        })
}

pub async fn insert_node(pool: &SqlitePool, p: &NodePayload) -> AppResult<Node> {
    let result = sqlx::query(
        "INSERT INTO nodes (name, role, local_ip, local_port, remote_ip, remote_port, node_id_hex, enabled, group_bind_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )
    .bind(&p.name)
    .bind(p.role)
    .bind(&p.local_ip)
    .bind(p.local_port)
    .bind(&p.remote_ip)
    .bind(p.remote_port)
    .bind(&p.node_id_hex)
    .bind(p.enabled)
    .bind(p.group_binds.as_ref().map(|g| serde_json::to_string(g).unwrap_or_default()))
    .execute(pool)
    .await?;
    get_node(pool, result.last_insert_rowid()).await
}

pub async fn update_node(pool: &SqlitePool, id: i64, p: &NodePayload) -> AppResult<Node> {
    let result = sqlx::query(
        "UPDATE nodes SET name=?1, role=?2, local_ip=?3, local_port=?4, remote_ip=?5,
         remote_port=?6, node_id_hex=?7, enabled=?8, group_bind_json=?9, updated_at=datetime('now','localtime')
         WHERE id=?10",
    )
    .bind(&p.name)
    .bind(p.role)
    .bind(&p.local_ip)
    .bind(p.local_port)
    .bind(&p.remote_ip)
    .bind(p.remote_port)
    .bind(&p.node_id_hex)
    .bind(p.enabled)
    .bind(p.group_binds.as_ref().map(|g| serde_json::to_string(g).unwrap_or_default()))
    .bind(id)
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::msg(format!("节点不存在: id={id}")));
    }
    get_node(pool, id).await
}

/// 更新节点认证会话（注册应答返回的 Session）; 不影响其他字段
pub async fn update_node_session(
    pool: &SqlitePool,
    id: i64,
    session_hex: &str,
) -> AppResult<()> {
    sqlx::query(
        "UPDATE nodes SET session_hex=?1, updated_at=datetime('now','localtime') WHERE id=?2",
    )
    .bind(session_hex)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// 写入一条响应捕获: 当前终端桶 captures[device_key][field]=value(读改写整个 JSON)。
/// 多终端单通道: 各终端(源设备/序列号值区分)的安全信息/会话/分配值互不干扰
pub async fn update_node_capture(
    pool: &SqlitePool,
    id: i64,
    device_key: &str,
    field: &str,
    value: &str,
) -> AppResult<()> {
    // Merge patches inside SQLite: concurrent responses must not overwrite each other's buckets.
    let json = serde_json::json!({device_key: {field: value}}).to_string();
    sqlx::query(
        "UPDATE nodes SET captures_json=json_patch(COALESCE(captures_json,'{}'),?1), updated_at=datetime('now','localtime') WHERE id=?2",
    )
    .bind(json)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_node(pool: &SqlitePool, id: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM nodes WHERE id=?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ==================== 协议 CRUD ====================

pub async fn list_protocols(pool: &SqlitePool) -> AppResult<Vec<Protocol>> {
    let rows = sqlx::query_as::<_, Protocol>("SELECT p.*, s.name AS system_name, g.name AS group_name FROM protocols p LEFT JOIN protocol_systems s ON s.id = p.system_ref LEFT JOIN protocol_groups g ON g.id = p.group_ref ORDER BY p.id")
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

pub async fn get_protocol(pool: &SqlitePool, protocol_id: &str) -> AppResult<Protocol> {
    sqlx::query_as::<_, Protocol>("SELECT p.*, s.name AS system_name, g.name AS group_name FROM protocols p LEFT JOIN protocol_systems s ON s.id = p.system_ref LEFT JOIN protocol_groups g ON g.id = p.group_ref WHERE p.protocol_id=?")
        .bind(protocol_id)
        .fetch_one(pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => {
                AppError::msg(format!("协议不存在: protocol_id={protocol_id}"))
            }
            other => other.into(),
        })
}

/// 参与规则引擎的协议（按 id 顺序，先配置的先匹配）:
/// 带匹配正则或带字段断言条件（match_fields_json）任一即参与
pub async fn matchable_protocols(pool: &SqlitePool) -> AppResult<Vec<Protocol>> {
    // Header-only type codes must also participate; eligibility is checked against the resolved frame.
    list_protocols(pool).await
}

/// Shared guard for protocol and flow writes; SQL triggers also protect direct imports.
pub async fn validate_system_scope(pool: &SqlitePool, system_ref: Option<i64>, group_ref: Option<i64>) -> AppResult<()> {
    let system_id = system_ref.ok_or_else(|| AppError::msg("请选择归属系统"))?;
    get_system(pool, system_id).await?;
    if let Some(group_id) = group_ref {
        if get_group(pool, group_id).await?.system_ref != system_id {
            return Err(AppError::msg("分组必须属于所选系统"));
        }
    }
    Ok(())
}

pub async fn upsert_protocol(pool: &SqlitePool, p: &ProtocolPayload) -> AppResult<Protocol> {
    validate_system_scope(pool, p.system_ref, p.group_ref).await?;
    let headers = headers_for_selection(pool, p.group_ref, p.header_refs.as_deref()).await?;
    simterm_core::fields::merge_header_specs(&headers, &p.fields)?;
    let mut connection = pool.acquire().await?;
    write_protocol(&mut connection, p).await?;
    drop(connection);
    get_protocol(pool, &p.protocol_id).await
}

async fn write_protocol(connection: &mut sqlx::SqliteConnection, p: &ProtocolPayload) -> AppResult<()> {
    let header_refs_json = p.header_refs.as_ref().map(serde_json::to_string).transpose().map_err(|e| AppError::msg(e.to_string()))?;
    let match_fields_json = if p.match_fields.is_empty() {
        None
    } else {
        Some(
            serde_json::to_string(&p.match_fields)
                .map_err(|e| AppError::msg(format!("匹配条件序列化失败: {e}")))?,
        )
    };
    sqlx::query(
        "INSERT INTO protocols (protocol_id, name, format, msg_type, def_mode, fields_json, payload_template, match_rule, match_fields_json, auto_respond, respond_delay_ms, auto_periodic, auto_periodic_interval_ms, description, system_ref, group_ref, header_refs_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
         ON CONFLICT(protocol_id) DO UPDATE SET
           name=excluded.name, format=excluded.format, msg_type=excluded.msg_type,
           def_mode=excluded.def_mode, fields_json=excluded.fields_json,
           payload_template=excluded.payload_template, match_rule=excluded.match_rule,
           match_fields_json=excluded.match_fields_json,
           auto_respond=excluded.auto_respond, respond_delay_ms=excluded.respond_delay_ms,
           auto_periodic=excluded.auto_periodic,
           auto_periodic_interval_ms=excluded.auto_periodic_interval_ms,
           description=excluded.description, system_ref=excluded.system_ref,
           group_ref=excluded.group_ref, header_refs_json=excluded.header_refs_json, updated_at=datetime('now','localtime')",
    )
    .bind(&p.protocol_id)
    .bind(&p.name)
    .bind(p.format)
    .bind(p.msg_type)
    .bind(p.def_mode)
    .bind(if p.def_mode == DefMode::Fields {
        Some(serde_json::to_string(&p.fields).unwrap_or_else(|_| "[]".into()))
    } else {
        None
    })
    .bind(&p.payload_template)
    .bind(&p.match_rule)
    .bind(match_fields_json)
    .bind(&p.auto_respond)
    .bind(p.respond_delay_ms)
    .bind(&p.auto_periodic)
    .bind(p.auto_periodic_interval_ms)
    .bind(&p.description)
    .bind(p.system_ref)
    .bind(p.group_ref)
    .bind(header_refs_json)
    .execute(connection)
    .await?;
    Ok(())
}

// ==================== 协议系统 CRUD ====================

pub async fn list_systems(pool: &SqlitePool) -> AppResult<Vec<ProtocolSystem>> {
    let rows = sqlx::query_as::<_, ProtocolSystem>("SELECT * FROM protocol_systems ORDER BY id")
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

/// 按名称查找系统, 不存在则创建（md 导入用）
pub async fn resolve_system_by_name(pool: &SqlitePool, name: &str) -> AppResult<ProtocolSystem> {
    let name = name.trim();
    if name.is_empty() { return Err(AppError::msg("系统名称不能为空")); }
    if let Ok(existing) =
        sqlx::query_as::<_, ProtocolSystem>("SELECT * FROM protocol_systems WHERE name=?")
            .bind(name)
            .fetch_one(pool)
            .await
    {
        return Ok(existing);
    }
    let result = sqlx::query("INSERT INTO protocol_systems (name) VALUES (?)")
        .bind(name)
        .execute(pool)
        .await?;
    Ok(ProtocolSystem {
        id: result.last_insert_rowid(),
        name: name.to_string(),
        description: None,
        header_enabled: false,
        header_fields_json: None,
        created_at: String::new(),
    })
}

pub async fn get_system(pool: &SqlitePool, id: i64) -> AppResult<ProtocolSystem> {
    sqlx::query_as::<_, ProtocolSystem>("SELECT * FROM protocol_systems WHERE id=?")
        .bind(id)
        .fetch_one(pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => AppError::msg(format!("系统不存在: id={id}")),
            other => other.into(),
        })
}

pub async fn update_system(
    pool: &SqlitePool,
    id: i64,
    p: &SystemPayload,
) -> AppResult<ProtocolSystem> {
    let r = sqlx::query("UPDATE protocol_systems SET name=?1, description=?2 WHERE id=?3")
        .bind(&p.name)
        .bind(&p.description)
        .bind(id)
        .execute(pool)
        .await?;
    if r.rows_affected() == 0 {
        return Err(AppError::msg(format!("系统不存在: id={id}")));
    }
    get_system(pool, id).await
}

/// Resolve only headers selected by the protocol within its own group.
pub async fn headers_for_selection(pool: &SqlitePool, group_ref: Option<i64>, refs: Option<&[String]>) -> AppResult<Vec<ProtocolField>> {
    let group = match group_ref { Some(id) => Some(get_group(pool, id).await?), None => None };
    simterm_core::headers::resolve_headers(group.as_ref(), refs)
}

pub async fn protocol_headers(pool: &SqlitePool, protocol: &Protocol) -> AppResult<Vec<ProtocolField>> {
    if let Some(group_id) = protocol.group_ref {
        let group = get_group(pool, group_id).await?;
        if Some(group.system_ref) != protocol.system_ref { return Err(AppError::msg("分组必须属于协议归属系统")); }
        return simterm_core::headers::resolve_headers(Some(&group), protocol.header_refs()?.as_deref());
    }
    simterm_core::headers::resolve_headers(None, protocol.header_refs()?.as_deref())
}

/// Legacy editor still updates the default definition while preserving other headers.
pub async fn update_group_header(pool: &SqlitePool, group_id: i64, p: &GroupHeaderPayload) -> AppResult<ProtocolGroup> {
    let group = get_group(pool, group_id).await?;
    let mut headers = simterm_core::headers::group_definitions(&group)?;
    let legacy = HeaderDefinition { id: simterm_core::headers::LEGACY_HEADER_ID.into(), name: format!("{} 公共头", group.name), fields: p.fields.clone() };
    if legacy.fields.is_empty() { headers.retain(|h| h.id != legacy.id); }
    else if let Some(existing) = headers.iter_mut().find(|h| h.id == legacy.id) { *existing = legacy; }
    else { headers.push(legacy); }
    save_group_headers(pool, group_id, &headers, Some(p.enabled)).await
}

pub async fn update_group_headers(pool: &SqlitePool, group_id: i64, p: &GroupHeadersPayload) -> AppResult<ProtocolGroup> {
    save_group_headers(pool, group_id, &p.headers, None).await
}

pub(crate) fn group_with_headers(old: &ProtocolGroup, headers: &[HeaderDefinition], legacy_enabled: Option<bool>) -> AppResult<ProtocolGroup> {
    use simterm_core::headers::{group_definitions, validate_definitions, LEGACY_HEADER_ID};
    validate_definitions(headers)?;
    let old_headers = group_definitions(old)?;
    let legacy = headers.iter().find(|h| h.id == LEGACY_HEADER_ID);
    let mut candidate = old.clone();
    candidate.header_enabled = legacy.is_some() && legacy_enabled.unwrap_or_else(|| if old_headers.iter().any(|h| h.id == LEGACY_HEADER_ID) { old.header_enabled } else { true });
    candidate.header_profiles_json = Some(serde_json::to_string(headers).map_err(|e| AppError::msg(e.to_string()))?);
    candidate.header_fields_json = legacy.map(|h| serde_json::to_string(&h.fields)).transpose().map_err(|e| AppError::msg(e.to_string()))?;
    Ok(candidate)
}

pub(crate) async fn save_group_headers(pool: &SqlitePool, group_id: i64, headers: &[HeaderDefinition], legacy_enabled: Option<bool>) -> AppResult<ProtocolGroup> {
    import_group_protocols(pool, group_id, Some(headers), legacy_enabled, &[]).await
}

/// Validate the final group state and commit headers plus imported protocols atomically.
/// Protocols absent from this import retain their references and still protect their headers.
pub(crate) async fn import_group_protocols(pool: &SqlitePool, group_id: i64, headers: Option<&[HeaderDefinition]>, legacy_enabled: Option<bool>, updates: &[ProtocolPayload]) -> AppResult<ProtocolGroup> {
    use simterm_core::headers::{group_definitions, resolve_headers, selected_header_ids};
    let mut tx = pool.begin().await?;
    let old = sqlx::query_as::<_, ProtocolGroup>("SELECT * FROM protocol_groups WHERE id=?").bind(group_id).fetch_one(&mut *tx).await?;
    let candidate = match headers { Some(headers) => group_with_headers(&old, headers, legacy_enabled)?, None => old.clone() };
    let new_headers = group_definitions(&candidate)?;
    let old_headers = group_definitions(&old)?;
    let protocols = sqlx::query_as::<_, Protocol>("SELECT p.*, NULL AS system_name, NULL AS group_name FROM protocols p WHERE group_ref=?").bind(group_id).fetch_all(&mut *tx).await?;
    let mut incoming = std::collections::HashMap::new();
    for update in updates {
        if update.group_ref != Some(group_id) || update.system_ref != Some(old.system_ref) { return Err(AppError::msg(format!("协议 [{}] 不属于待导入分组", update.name))); }
        if incoming.insert(update.protocol_id.as_str(), update).is_some() { return Err(AppError::msg(format!("导入协议标识 [{}] 重复", update.protocol_id))); }
        let selected = resolve_headers(Some(&candidate), update.header_refs.as_deref())?;
        simterm_core::fields::merge_header_specs(&selected, &update.fields)
            .map_err(|e| AppError::msg(format!("协议 [{}] 的公共头配置无效: {e}", update.name)))?;
    }
    for removed in old_headers.iter().filter(|h| !new_headers.iter().any(|new| new.id == h.id)) {
        let mut users = Vec::new();
        for protocol in &protocols {
            let refs = match incoming.get(protocol.protocol_id.as_str()) {
                Some(update) => update.header_refs.clone(),
                None => protocol.header_refs()?,
            };
            if selected_header_ids(Some(&old), refs.as_deref())?.contains(&removed.id) { users.push(protocol.name.clone()); }
        }
        if !users.is_empty() { return Err(AppError::msg(format!("公共头 [{}] 正被协议 {} 使用，不能删除", removed.name, users.join("、")))); }
    }
    for protocol in protocols.iter().filter(|p| !incoming.contains_key(p.protocol_id.as_str())) {
        let fields = resolve_headers(Some(&candidate), protocol.header_refs()?.as_deref())?;
        simterm_core::fields::merge_header_specs(&fields, &protocol.field_specs())
            .map_err(|e| AppError::msg(format!("协议 [{}] 的公共头配置无效: {e}", protocol.name)))?;
    }
    sqlx::query("UPDATE protocol_groups SET header_profiles_json=?1, header_enabled=?2, header_fields_json=?3 WHERE id=?4")
        .bind(&candidate.header_profiles_json).bind(candidate.header_enabled).bind(&candidate.header_fields_json).bind(group_id).execute(&mut *tx).await?;
    for update in updates { write_protocol(&mut tx, update).await?; }
    tx.commit().await?;
    Ok(candidate)
}

/// 取分组（含公共头）
pub async fn get_group(pool: &SqlitePool, id: i64) -> AppResult<ProtocolGroup> {
    sqlx::query_as::<_, ProtocolGroup>("SELECT * FROM protocol_groups WHERE id=?")
        .bind(id)
        .fetch_one(pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => AppError::msg(format!("分组不存在: id={id}")),
            other => other.into(),
        })
}

pub async fn delete_system(pool: &SqlitePool, id: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM protocol_systems WHERE id=?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_protocol(pool: &SqlitePool, id: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM protocols WHERE id=?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ==================== 心跳绑定 CRUD ====================

pub async fn list_heartbeat_views(pool: &SqlitePool) -> AppResult<Vec<HeartbeatView>> {
    let rows = sqlx::query_as::<_, HeartbeatView>(
        "SELECT b.id, b.node_ref, n.name AS node_name, b.protocol_ref, b.interval_ms, b.auto_start
         FROM heartbeat_bindings b JOIN nodes n ON n.id = b.node_ref
         ORDER BY b.id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn get_heartbeat(pool: &SqlitePool, id: i64) -> AppResult<HeartbeatBinding> {
    sqlx::query_as::<_, HeartbeatBinding>(
        "SELECT * FROM heartbeat_bindings WHERE id=?",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(|e| match e {
        sqlx::Error::RowNotFound => AppError::msg(format!("心跳绑定不存在: id={id}")),
        other => other.into(),
    })
}

pub async fn upsert_heartbeat(
    pool: &SqlitePool,
    p: &HeartbeatPayload,
) -> AppResult<HeartbeatBinding> {
    sqlx::query(
        "INSERT INTO heartbeat_bindings (node_ref, protocol_ref, interval_ms, auto_start)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(node_ref, protocol_ref) DO UPDATE SET
           interval_ms=excluded.interval_ms, auto_start=excluded.auto_start",
    )
    .bind(p.node_ref)
    .bind(&p.protocol_ref)
    .bind(p.interval_ms)
    .bind(p.auto_start)
    .execute(pool)
    .await?;

    let row = sqlx::query_as::<_, HeartbeatBinding>(
        "SELECT * FROM heartbeat_bindings WHERE node_ref=?1 AND protocol_ref=?2",
    )
    .bind(p.node_ref)
    .bind(&p.protocol_ref)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn delete_heartbeat(pool: &SqlitePool, id: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM heartbeat_bindings WHERE id=?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// 某节点下 auto_start=1 的心跳绑定（入网成功后自动启动用）
pub async fn auto_start_heartbeats(pool: &SqlitePool, node_id: i64) -> AppResult<Vec<HeartbeatBinding>> {
    let rows = sqlx::query_as::<_, HeartbeatBinding>(
        "SELECT * FROM heartbeat_bindings WHERE node_ref=? AND auto_start=1",
    )
    .bind(node_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// ==================== 日志 ====================

#[allow(clippy::too_many_arguments)]
pub async fn insert_log(
    pool: &SqlitePool,
    direction: Direction,
    node_id: Option<i64>,
    local_addr: &str,
    remote_addr: &str,
    protocol_id: Option<&str>,
    raw_hex: Option<&str>,
    raw_text: Option<&str>,
    parsed: Option<&str>,
    status: &str,
) -> AppResult<i64> {
    let result = sqlx::query(
        "INSERT INTO logs (direction, node_ref, local_addr, remote_addr, protocol_id, raw_hex, raw_text, parsed, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )
    .bind(direction)
    .bind(node_id)
    .bind(local_addr)
    .bind(remote_addr)
    .bind(protocol_id)
    .bind(raw_hex)
    .bind(raw_text)
    .bind(parsed)
    .bind(status)
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

pub async fn query_logs(pool: &SqlitePool, f: &LogFilter) -> AppResult<Vec<LogRecord>> {
    let mut sql = String::from(
        "SELECT * FROM logs WHERE 1=1",
    );
    if f.keyword.is_some() {
        sql.push_str(
            " AND (raw_hex LIKE ? OR raw_text LIKE ? OR protocol_id LIKE ? OR remote_addr LIKE ?)",
        );
    }
    if f.node_id.is_some() {
        sql.push_str(" AND node_ref = ?");
    }
    if f.direction.is_some() {
        sql.push_str(" AND direction = ?");
    }
    sql.push_str(" ORDER BY id DESC LIMIT ? OFFSET ?");

    let mut q = sqlx::query_as::<_, LogRecord>(&sql);
    if let Some(kw) = &f.keyword {
        let like = format!("%{kw}%");
        q = q
            .bind(like.clone())
            .bind(like.clone())
            .bind(like.clone())
            .bind(like);
    }
    if let Some(n) = f.node_id {
        q = q.bind(n);
    }
    if let Some(d) = f.direction {
        q = q.bind(d);
    }
    q = q.bind(f.limit).bind(f.offset);

    Ok(q.fetch_all(pool).await?)
}

/// 清理历史日志, 保留最近 keep_recent 条, 返回删除条数
pub async fn clear_logs(pool: &SqlitePool, keep_recent: i64) -> AppResult<u64> {
    if keep_recent <= 0 {
        let r = sqlx::query("DELETE FROM logs").execute(pool).await?;
        Ok(r.rows_affected())
    } else {
        let r = sqlx::query(
            "DELETE FROM logs WHERE id NOT IN (SELECT id FROM logs ORDER BY id DESC LIMIT ?)",
        )
        .bind(keep_recent)
        .execute(pool)
        .await?;
        Ok(r.rows_affected())
    }
}

pub async fn log_stats(pool: &SqlitePool) -> AppResult<LogStats> {
    let rows = sqlx::query(
        "SELECT direction, COUNT(*) AS cnt FROM logs WHERE ts >= date('now','localtime')
         GROUP BY direction",
    )
    .fetch_all(pool)
    .await?;
    let mut stats = LogStats::default();
    for row in rows {
        let dir: String = row.try_get("direction")?;
        let cnt: i64 = row.try_get("cnt")?;
        match dir.as_str() {
            "in" => stats.today_in = cnt,
            "out" => stats.today_out = cnt,
            _ => {}
        }
    }
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM logs")
        .fetch_one(pool)
        .await?;
    stats.total = total;
    Ok(stats)
}

// ==================== 协议分组 CRUD ====================

pub async fn list_groups(pool: &SqlitePool) -> AppResult<Vec<ProtocolGroup>> {
    let rows =
        sqlx::query_as::<_, ProtocolGroup>("SELECT * FROM protocol_groups ORDER BY system_ref, id")
            .fetch_all(pool)
            .await?;
    Ok(rows)
}

/// 系统内按名称查找分组, 不存在则创建（md 导入用）
pub async fn resolve_group_by_name(
    pool: &SqlitePool,
    system_ref: i64,
    name: &str,
) -> AppResult<ProtocolGroup> {
    if let Ok(existing) = sqlx::query_as::<_, ProtocolGroup>(
        "SELECT * FROM protocol_groups WHERE system_ref=? AND name=?",
    )
    .bind(system_ref)
    .bind(name)
    .fetch_one(pool)
    .await
    {
        return Ok(existing);
    }
    let result = sqlx::query("INSERT INTO protocol_groups (system_ref, name) VALUES (?, ?)")
        .bind(system_ref)
        .bind(name)
        .execute(pool)
        .await?;
    Ok(ProtocolGroup {
        header_profiles_json: None,
        id: result.last_insert_rowid(),
        system_ref,
        name: name.to_string(),
        header_enabled: false,
        header_fields_json: None,
        created_at: String::new(),
    })
}

pub async fn update_group(
    pool: &SqlitePool,
    id: i64,
    p: &GroupPayload,
) -> AppResult<ProtocolGroup> {
    let r = sqlx::query("UPDATE protocol_groups SET system_ref=?1, name=?2 WHERE id=?3")
        .bind(p.system_ref)
        .bind(&p.name)
        .bind(id)
        .execute(pool)
        .await?;
    if r.rows_affected() == 0 {
        return Err(AppError::msg(format!("分组不存在: id={id}")));
    }
    sqlx::query_as::<_, ProtocolGroup>("SELECT * FROM protocol_groups WHERE id=?")
        .bind(id)
        .fetch_one(pool)
        .await
        .map_err(|e| e.into())
}

pub async fn delete_group(pool: &SqlitePool, id: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM protocol_groups WHERE id=?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ==================== 字典: 字典(dict_sets) → 条目(dict_items) ====================

/// 字典行 + LEFT JOIN 得到的系统名/分组名
#[derive(Debug, Clone, sqlx::FromRow)]
struct DictSetRow {
    id: i64,
    key: String,
    name: String,
    system_ref: Option<i64>,
    group_ref: Option<i64>,
    system_name: Option<String>,
    group_name: Option<String>,
    byte_len: i64,
    description: Option<String>,
    created_at: String,
}

impl DictSetRow {
    fn into_view(self, items: Vec<DictItem>) -> DictSetView {
        DictSetView {
            id: self.id,
            key: self.key,
            name: self.name,
            system_ref: self.system_ref,
            group_ref: self.group_ref,
            system_name: self.system_name,
            group_name: self.group_name,
            byte_len: self.byte_len,
            description: self.description,
            created_at: self.created_at,
            items,
        }
    }
}

/// 全部字典（含条目）; 全局字典在前, 其后按系统名、分组名、Key 排序
pub async fn list_dict_sets(pool: &SqlitePool) -> AppResult<Vec<DictSetView>> {
    let rows = sqlx::query_as::<_, DictSetRow>(
        r#"SELECT s.id, s."key" AS "key", s.name, s.system_ref, s.group_ref, s.byte_len, s.description, s.created_at,
                  ps.name AS system_name, g.name AS group_name
           FROM dict_sets s
           LEFT JOIN protocol_groups g ON g.id = s.group_ref
           LEFT JOIN protocol_systems ps ON ps.id = COALESCE(g.system_ref, s.system_ref)
           ORDER BY (s.group_ref IS NOT NULL), ps.name, g.name, s."key""#,
    )
    .fetch_all(pool)
    .await?;

    let mut grouped = grouped_items(pool).await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            let items = grouped.remove(&r.id).unwrap_or_default();
            r.into_view(items)
        })
        .collect())
}

pub async fn get_dict_set(pool: &SqlitePool, id: i64) -> AppResult<DictSetView> {
    let row = sqlx::query_as::<_, DictSetRow>(
        r#"SELECT s.id, s."key" AS "key", s.name, s.system_ref, s.group_ref, s.byte_len, s.description, s.created_at,
                  ps.name AS system_name, g.name AS group_name
           FROM dict_sets s
           LEFT JOIN protocol_groups g ON g.id = s.group_ref
           LEFT JOIN protocol_systems ps ON ps.id = COALESCE(g.system_ref, s.system_ref)
           WHERE s.id = ?1"#,
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .map_err(|e| match e {
        sqlx::Error::RowNotFound => AppError::msg(format!("字典不存在: id={id}")),
        other => other.into(),
    })?;
    let items = list_dict_items(pool, id).await?;
    Ok(row.into_view(items))
}

/// 按 Key + 归属分组查字典（None 表示全局共享档）
pub async fn find_dict_set(
    pool: &SqlitePool,
    key: &str,
    group_ref: Option<i64>,
) -> AppResult<Option<DictSet>> {
    let row = sqlx::query_as::<_, DictSet>(
        r#"SELECT id, "key" AS "key", name, system_ref, group_ref, byte_len, description, created_at
           FROM dict_sets
           WHERE "key" = ?1 AND ((?2 IS NULL AND group_ref IS NULL) OR group_ref = ?2)"#,
    )
    .bind(key)
    .bind(group_ref)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// 新建字典
pub async fn insert_dict_set(pool: &SqlitePool, p: &DictSetPayload) -> AppResult<DictSetView> {
    let r = sqlx::query(
        r#"INSERT INTO dict_sets ("key", name, system_ref, group_ref, byte_len, description)
           VALUES (?1, ?2, NULL, ?3, ?4, ?5)"#,
    )
    .bind(&p.key)
    .bind(&p.name)
    .bind(p.group_ref)
    .bind(p.byte_len)
    .bind(&p.description)
    .execute(pool)
    .await?;
    get_dict_set(pool, r.last_insert_rowid()).await
}

/// 修改字典
pub async fn update_dict_set(
    pool: &SqlitePool,
    id: i64,
    p: &DictSetPayload,
) -> AppResult<DictSetView> {
    let r = sqlx::query(
        r#"UPDATE dict_sets SET "key"=?1, name=?2, system_ref=NULL, group_ref=?3, byte_len=?4, description=?5
           WHERE id=?6"#,
    )
    .bind(&p.key)
    .bind(&p.name)
    .bind(p.group_ref)
    .bind(p.byte_len)
    .bind(&p.description)
    .bind(id)
    .execute(pool)
    .await?;
    if r.rows_affected() == 0 {
        return Err(AppError::msg(format!("字典不存在: id={id}")));
    }
    get_dict_set(pool, id).await
}

/// 删除字典（条目随外键级联删除）
pub async fn delete_dict_set(pool: &SqlitePool, id: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM dict_sets WHERE id=?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_dict_items(pool: &SqlitePool, set_ref: i64) -> AppResult<Vec<DictItem>> {
    let rows = sqlx::query_as::<_, DictItem>(
        "SELECT * FROM dict_items WHERE set_ref=?1 ORDER BY sort_order, id",
    )
    .bind(set_ref)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 新增/修改条目（id 为 None 时新增, 同字典内值重复则更新其含义与排序）
pub async fn upsert_dict_item(
    pool: &SqlitePool,
    id: Option<i64>,
    p: &DictItemPayload,
) -> AppResult<DictItem> {
    let item_id = match id {
        Some(id) => {
            let r = sqlx::query(
                "UPDATE dict_items SET set_ref=?1, value=?2, label=?3, sort_order=?4 WHERE id=?5",
            )
            .bind(p.set_ref)
            .bind(&p.value)
            .bind(&p.label)
            .bind(p.sort_order)
            .bind(id)
            .execute(pool)
            .await?;
            if r.rows_affected() == 0 {
                return Err(AppError::msg(format!("字典条目不存在: id={id}")));
            }
            id
        }
        None => {
            sqlx::query(
                "INSERT INTO dict_items (set_ref, value, label, sort_order) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(set_ref, value) DO UPDATE SET
                   label=excluded.label, sort_order=excluded.sort_order",
            )
            .bind(p.set_ref)
            .bind(&p.value)
            .bind(&p.label)
            .bind(p.sort_order)
            .execute(pool)
            .await?;
            sqlx::query_scalar::<_, i64>("SELECT id FROM dict_items WHERE set_ref=?1 AND value=?2")
                .bind(p.set_ref)
                .bind(&p.value)
                .fetch_one(pool)
                .await?
        }
    };
    sqlx::query_as::<_, DictItem>("SELECT * FROM dict_items WHERE id=?")
        .bind(item_id)
        .fetch_one(pool)
        .await
        .map_err(|e| e.into())
}

pub async fn delete_dict_item(pool: &SqlitePool, id: i64) -> AppResult<()> {
    sqlx::query("DELETE FROM dict_items WHERE id=?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// 导入模板时落库一张字典: 按 Key+分组 upsert, 并整体替换条目（保证重导入幂等）
pub async fn import_dict_set(
    pool: &SqlitePool,
    key: &str,
    name: &str,
    group_ref: Option<i64>,
    byte_len: i64,
    items: &[(String, String)],
) -> AppResult<i64> {
    let set_id = match find_dict_set(pool, key, group_ref).await? {
        Some(existing) => {
            sqlx::query("UPDATE dict_sets SET name=?1, byte_len=?2 WHERE id=?3")
                .bind(name)
                .bind(byte_len)
                .bind(existing.id)
                .execute(pool)
                .await?;
            existing.id
        }
        None => {
            let r = sqlx::query(
                r#"INSERT INTO dict_sets ("key", name, system_ref, group_ref, byte_len, description)
                   VALUES (?1, ?2, NULL, ?3, ?4, ?5)"#,
            )
            .bind(key)
            .bind(name)
            .bind(group_ref)
            .bind(byte_len)
            .bind(Some("由模板导入"))
            .execute(pool)
            .await?;
            r.last_insert_rowid()
        }
    };
    sqlx::query("DELETE FROM dict_items WHERE set_ref=?")
        .bind(set_id)
        .execute(pool)
        .await?;
    for (i, (value, label)) in items.iter().enumerate() {
        sqlx::query(
            "INSERT OR IGNORE INTO dict_items (set_ref, value, label, sort_order)
             VALUES (?1, ?2, ?3, ?4)",
        )
        .bind(set_id)
        .bind(value)
        .bind(label)
        .bind(i as i64)
        .execute(pool)
        .await?;
    }
    Ok(set_id)
}

/// 构建某分组作用域内的字典映射（渲染与解析用）
///
/// 取「全局共享字典 + 该分组专属字典」, 同 Key 时分组专属覆盖全局,
/// 于是不同分组可以各有一张同 Key 的字典而互不干扰。
pub async fn dict_map_for_group(
    pool: &SqlitePool,
    group_ref: Option<i64>,
) -> AppResult<DictMap> {
    let sets = sqlx::query_as::<_, DictSet>(
        r#"SELECT id, "key" AS "key", name, system_ref, group_ref, byte_len, description, created_at
           FROM dict_sets
           WHERE group_ref IS NULL OR group_ref = ?1
           ORDER BY (group_ref IS NULL) DESC, id"#,
    )
    .bind(group_ref)
    .fetch_all(pool)
    .await?;
    if sets.is_empty() {
        return Ok(DictMap::new());
    }
    let mut grouped = grouped_items(pool).await?;
    let mut map = DictMap::new();
    for s in sets {
        let items = grouped
            .remove(&s.id)
            .unwrap_or_default()
            .into_iter()
            .map(|i| (i.value, i.label))
            .collect();
        map.insert(DictSetLite {
            key: s.key,
            name: s.name,
            byte_len: s.byte_len.max(0) as usize,
            items,
        });
    }
    Ok(map)
}

/// 引用了指定字典 Key 的协议 ID 列表（删除字典/改 Key 前的引用检查）
///
/// 字段绑定存在 fields_json 里, 形如 `"dict":"device_type"`。
pub async fn dict_set_references(pool: &SqlitePool, key: &str) -> AppResult<Vec<String>> {
    // LIKE 的 _ 与 % 需转义, 否则 target_id 会误命中 targetXid
    let escaped = key
        .replace('\\', r"\\")
        .replace('_', r"\_")
        .replace('%', r"\%");
    let rows = sqlx::query_scalar::<_, String>(
        r#"SELECT protocol_id FROM protocols
           WHERE fields_json IS NOT NULL AND fields_json LIKE ?1 ESCAPE '\'
           ORDER BY id"#,
    )
    .bind(format!("%\"dict\":\"{escaped}\"%"))
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// 一次取回全部条目并按 set_ref 分组（字典数据量小, 避免 N+1 查询）
async fn grouped_items(
    pool: &SqlitePool,
) -> AppResult<std::collections::HashMap<i64, Vec<DictItem>>> {
    let items =
        sqlx::query_as::<_, DictItem>("SELECT * FROM dict_items ORDER BY sort_order, id")
            .fetch_all(pool)
            .await?;
    let mut grouped: std::collections::HashMap<i64, Vec<DictItem>> =
        std::collections::HashMap::new();
    for it in items {
        grouped.entry(it.set_ref).or_default().push(it);
    }
    Ok(grouped)
}
