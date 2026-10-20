//! 日志查询 Commands

use tauri::State;

use crate::db;
use crate::engine::state::AppState;
use simterm_core::error::{AppError, AppResult};
use simterm_core::models::{LogFilter, LogRecord, LogStats};

/// 分页/过滤查询历史日志
#[tauri::command]
pub async fn query_logs(
    state: State<'_, AppState>,
    filter: LogFilter,
) -> AppResult<Vec<LogRecord>> {
    db::query_logs(&state.pool, &filter).await
}

/// 清理日志: keep_recent <= 0 时全清, 否则保留最近 N 条; 返回删除条数
#[tauri::command]
pub async fn clear_logs(state: State<'_, AppState>, keep_recent: i64) -> AppResult<u64> {
    db::clear_logs(&state.pool, keep_recent).await
}

/// 收发统计（Dashboard）
#[tauri::command]
pub async fn log_stats(state: State<'_, AppState>) -> AppResult<LogStats> {
    db::log_stats(&state.pool).await
}

#[tauri::command]
pub async fn traffic_stats(state: State<'_, AppState>, range: String) -> AppResult<simterm_core::traffic_stats::TrafficStats> {
    simterm_core::traffic_stats::aggregate(&state.pool, &range, chrono::Local::now().naive_local()).await
}

/// 按日志 id 取回单条记录
async fn get_log_row(
    pool: &sqlx::SqlitePool,
    id: i64,
) -> AppResult<simterm_core::models::LogRecord> {
    use simterm_core::models::LogRecord;
    sqlx::query_as::<_, LogRecord>("SELECT * FROM logs WHERE id=?")
        .bind(id)
        .fetch_one(pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => AppError::msg(format!("日志不存在: id={id}")),
            other => other.into(),
        })
}

fn msg_type_label(t: simterm_core::models::MsgType) -> &'static str {
    match t {
        simterm_core::models::MsgType::Request => "请求",
        simterm_core::models::MsgType::Response => "应答",
        simterm_core::models::MsgType::Heartbeat => "心跳",
        simterm_core::models::MsgType::Event => "事件",
    }
}

/// 解析一帧已记录报文: 逐字段给出 位置/十六进制/十进制/字典含义,
/// 使用协议显示分区与记录元数据；未覆盖的剩余字节单独标为 unparsed。
#[tauri::command]
pub async fn parse_datagram(
    state: State<'_, AppState>,
    log_id: i64,
) -> AppResult<simterm_core::models::DatagramParse> {
    use simterm_core::models::DatagramParse;

    let row = get_log_row(&state.pool, log_id).await?;
    let protocol_id = row
        .protocol_id
        .clone()
        .ok_or_else(|| AppError::msg("该报文未命中任何协议, 无法按字段解析"))?;
    let p = crate::db::get_protocol(&state.pool, &protocol_id).await?;

    let bytes: Vec<u8> = hex::decode(row.raw_hex.as_deref().unwrap_or("").replace(' ', ""))
        .map_err(|_| AppError::msg("日志原始报文不是合法十六进制"))?;

    let dicts = crate::db::dict_map_for_group(&state.pool, p.group_ref).await?;
    let header = crate::db::protocol_headers(&state.pool, &p).await?;
    // 与发送渲染保持同一字段顺序；公共头字段数只提供缺省分区来源。
    let specs = crate::engine::udp::frame_specs(&p, &header)?;
    let details = simterm_core::datagram::parse_field_details(&specs, header.len(), &bytes, &dicts);
    Ok(DatagramParse {
        log_id,
        ts: row.ts.clone(),
        direction: row.direction,
        node_name: None,
        local_addr: row.local_addr.clone(),
        remote_addr: row.remote_addr.clone(),
        protocol_id: p.protocol_id.clone(),
        protocol_name: p.name.clone(),
        msg_type_label: msg_type_label(p.msg_type).to_string(),
        total_len: bytes.len(),
        header_len: details.header_len,
        defined_len: details.defined_len,
        raw_hex: hex::encode_upper(&bytes),
        fields: details.fields,
    })
}
