//! 引擎 Commands：监听启停、手动发送、入网、心跳任务管理

use std::collections::HashMap;
use std::time::Instant;

use tauri::{AppHandle, State};

use crate::db;
use crate::engine::heartbeat;
use crate::engine::state::{AppState, PendingLogin};
use crate::engine::udp;
use simterm_core::error::{AppError, AppResult};
use simterm_core::models::{MsgType,
    EngineStatus, HeartbeatBinding, HeartbeatPayload, HeartbeatView, SendResult,
};

/// 启动节点 UDP 监听（绑定本地端口 + 进入规则引擎接收循环）
#[tauri::command]
pub async fn start_udp_server(
    app: AppHandle,
    state: State<'_, AppState>,
    node_id: i64,
) -> AppResult<()> {
    let node = db::get_node(&state.pool, node_id).await?;
    udp::ensure_server(&app, &state.pool, &state.engine, &node).await?;
    Ok(())
}

/// 停止节点监听（释放端口, 联动停止该节点心跳与挂起入网）
#[tauri::command]
pub async fn stop_udp_server(
    app: AppHandle,
    state: State<'_, AppState>,
    node_id: i64,
) -> AppResult<()> {
    udp::stop_server(&app, &state.engine, node_id);
    Ok(())
}

/// 引擎当前状态（监听中的节点 / 运行中的心跳）
#[tauri::command]
pub async fn engine_status(state: State<'_, AppState>) -> AppResult<EngineStatus> {
    Ok(state.engine.emit_status_event())
}

/// 手动发送：查询协议模板 → 渲染变量 → UDP 发送 → 记录 OUT 日志
#[tauri::command]
pub async fn send_udp_message(
    app: AppHandle,
    state: State<'_, AppState>,
    node_id: i64,
    protocol_id: String,
    params: Option<HashMap<String, String>>,
) -> AppResult<SendResult> {
    let node = db::get_node(&state.pool, node_id).await?;
    let protocol = db::get_protocol(&state.pool, &protocol_id).await?;
    // 请求类协议发送即挂起入网等待: 收到 AutoRespond 应答时触发 login-success
    if protocol.msg_type == MsgType::Request {
        if let Some(resp) = protocol
            .auto_respond
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            state.engine.pending_logins.insert(
                node_id,
                PendingLogin {
                    expect_protocol_id: resp.to_string(),
                    sent_at: Instant::now(),
                },
            );
        }
    }
    udp::send_datagram(
        &app,
        &state.pool,
        &state.engine,
        &node,
        &protocol,
        &params.unwrap_or_default(),
        None,
    )
    .await
}

/// 入网流程：发送 login 请求, 并挂起等待其 AutoRespond 应答;
/// 应答命中后由引擎发出 login-success 事件并自动启动 auto_start 心跳
#[tauri::command]
pub async fn login(
    app: AppHandle,
    state: State<'_, AppState>,
    node_id: i64,
    protocol_id: String,
) -> AppResult<SendResult> {
    let node = db::get_node(&state.pool, node_id).await?;
    let protocol = db::get_protocol(&state.pool, &protocol_id).await?;
    let resp = protocol
        .auto_respond
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| {
            AppError::msg(format!(
                "协议 [{protocol_id}] 未配置 AutoRespond, 无法判定入网应答"
            ))
        })?
        .trim()
        .to_string();
    state.engine.pending_logins.insert(
        node_id,
        PendingLogin {
            expect_protocol_id: resp,
            sent_at: Instant::now(),
        },
    );
    udp::send_datagram(
        &app,
        &state.pool,
        &state.engine,
        &node,
        &protocol,
        &HashMap::new(),
        None,
    )
    .await
}

// ==================== 心跳绑定管理 ====================

#[tauri::command]
pub async fn list_heartbeats(state: State<'_, AppState>) -> AppResult<Vec<HeartbeatView>> {
    db::list_heartbeat_views(&state.pool).await
}

#[tauri::command]
pub async fn upsert_heartbeat(
    state: State<'_, AppState>,
    payload: HeartbeatPayload,
) -> AppResult<HeartbeatBinding> {
    if payload.interval_ms < 100 {
        return Err(AppError::msg("心跳间隔不能小于 100ms"));
    }
    // 校验节点与协议存在
    db::get_node(&state.pool, payload.node_ref).await?;
    db::get_protocol(&state.pool, &payload.protocol_ref).await?;
    db::upsert_heartbeat(&state.pool, &payload).await
}

#[tauri::command]
pub async fn delete_heartbeat(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> AppResult<()> {
    heartbeat::stop_heartbeat(&app, &state.engine, id);
    db::delete_heartbeat(&state.pool, id).await
}

/// 启动心跳任务（已运行则按新配置重启）
#[tauri::command]
pub async fn start_heartbeat_task(
    app: AppHandle,
    state: State<'_, AppState>,
    binding_id: i64,
) -> AppResult<()> {
    let binding = db::get_heartbeat(&state.pool, binding_id).await?;
    heartbeat::spawn_heartbeat(app, state.pool.clone(), state.engine.clone(), &binding).await
}

/// 停止心跳任务
#[tauri::command]
pub async fn stop_heartbeat_task(
    app: AppHandle,
    state: State<'_, AppState>,
    binding_id: i64,
) -> AppResult<()> {
    heartbeat::stop_heartbeat(&app, &state.engine, binding_id);
    Ok(())
}

/// 停止全部引擎任务（退出前清理）
#[tauri::command]
pub async fn stop_all(app: AppHandle, state: State<'_, AppState>) -> AppResult<()> {
    let node_ids: Vec<i64> = state.engine.sockets.iter().map(|r| *r.key()).collect();
    for id in node_ids {
        udp::stop_server(&app, &state.engine, id);
    }
    Ok(())
}

/// 启动某协议的周期发送（控制台开关, 不落库）
#[tauri::command]
pub async fn start_periodic_send(
    app: AppHandle,
    state: State<'_, AppState>,
    node_id: i64,
    protocol_id: String,
    interval_ms: i64,
    params: Option<HashMap<String,String>>,
) -> AppResult<()> {
    crate::engine::heartbeat::start_periodic_send_with_params(
        app,
        state.pool.clone(),
        state.engine.clone(),
        node_id,
        &protocol_id,
        interval_ms,
        params.unwrap_or_default(),
    )
    .await
}

/// 停止周期发送
#[tauri::command]
pub async fn stop_periodic_send(
    app: AppHandle,
    state: State<'_, AppState>,
    node_id: i64,
    protocol_id: String,
) -> AppResult<()> {
    crate::engine::heartbeat::stop_periodic_send(&app, &state.engine, node_id, &protocol_id);
    Ok(())
}
