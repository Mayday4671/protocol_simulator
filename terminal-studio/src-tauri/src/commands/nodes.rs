//! 节点管理 Commands

use tauri::{AppHandle, State};

use crate::db;
use crate::engine::state::AppState;
use crate::engine::udp;
use simterm_core::error::{AppError, AppResult};
use simterm_core::models::{Node, NodePayload};

#[tauri::command]
pub async fn list_nodes(state: State<'_, AppState>) -> AppResult<Vec<Node>> {
    db::list_nodes(&state.pool).await
}

#[tauri::command]
pub async fn create_node(state: State<'_, AppState>, payload: NodePayload) -> AppResult<Node> {
    validate_node_payload(&payload)?;
    db::insert_node(&state.pool, &payload).await
}

#[tauri::command]
pub async fn update_node(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
    payload: NodePayload,
) -> AppResult<Node> {
    validate_node_payload(&payload)?;
    let node = db::update_node(&state.pool, id, &payload).await?;
    // 若节点正处于监听状态, 用新配置重启监听使端口变更生效
    let was_running = state.engine.sockets.contains_key(&id);
    if was_running {
        udp::stop_server(&app, &state.engine, id).await;
        udp::ensure_server(&app, &state.pool, &state.engine, &node).await?;
    }
    Ok(node)
}

#[tauri::command]
pub async fn delete_node(app: AppHandle, state: State<'_, AppState>, id: i64) -> AppResult<()> {
    udp::stop_server(&app, &state.engine, id).await;
    db::delete_node(&state.pool, id).await
}

fn validate_node_payload(p: &NodePayload) -> AppResult<()> {
    if p.name.trim().is_empty() {
        return Err(AppError::msg("节点名称不能为空"));
    }
    if p.local_ip.trim().is_empty() {
        return Err(AppError::msg("本地 IP 不能为空"));
    }
    if p.remote_ip.trim().is_empty() {
        return Err(AppError::msg("目标 IP 不能为空"));
    }
    if let Some(hex_str) = p.node_id_hex.as_deref() {
        let compact: String = hex_str.chars().filter(|c| !c.is_whitespace()).collect();
        if !compact.is_empty() && hex::decode(&compact).is_err() {
            return Err(AppError::msg(format!(
                "node_id_hex 不是合法十六进制: {hex_str}"
            )));
        }
    }
    Ok(())
}
