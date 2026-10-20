//! 心跳任务：tokio::time::interval 定时渲染并发送心跳报文

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use sqlx::SqlitePool;
use tauri::{AppHandle, Runtime};
use tokio::time::MissedTickBehavior;

use crate::db;
use crate::engine::state::{EngineState, HeartbeatTask};
use crate::engine::udp;
use simterm_core::error::AppResult;
use simterm_core::models::{Direction, HeartbeatBinding, Node, Protocol};

/// 启动（或重启）一条心跳绑定对应的定时任务
pub async fn spawn_heartbeat<R: Runtime>(
    app: AppHandle<R>,
    pool: SqlitePool,
    engine: Arc<EngineState>,
    binding: &HeartbeatBinding,
) -> AppResult<()> {
    let node: Node = db::get_node(&pool, binding.node_ref).await?;
    let protocol: Protocol = db::get_protocol(&pool, &binding.protocol_ref).await?;

    // 已有任务则先停止（重启语义）
    if let Some(old) = engine.heartbeats.remove(&binding.id) {
        old.1.handle.abort();
    }
    // 心跳依赖收发能力, 确保节点监听已启动
    udp::ensure_server(&app, &pool, &engine, &node).await?;

    let interval = Duration::from_millis(binding.interval_ms.clamp(100, 3_600_000) as u64);
    let app2 = app.clone();
    let pool2 = pool.clone();
    let engine2 = engine.clone();
    let node2 = node.clone();
    let protocol2 = protocol.clone();

    let handle = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            if let Err(e) = udp::send_datagram(
                &app2,
                &pool2,
                &engine2,
                &node2,
                &protocol2,
                &HashMap::new(),
                None,
            )
            .await
            {
                let local = format!("{}:{}", node2.local_ip, node2.local_port);
                let remote = format!("{}:{}", node2.remote_ip, node2.remote_port);
                let _ = udp::push_log(
                    &app2,
                    &pool2,
                    Direction::Out,
                    Some(&node2),
                    &local,
                    &remote,
                    Some(&protocol2),
                    &[],
                    Some(format!("心跳发送失败: {e}")),
                    "error",
                )
                .await;
            }
        }
    });

    engine.heartbeats.insert(
        binding.id,
        HeartbeatTask {
            node_id: node.id,
            protocol_ref: binding.protocol_ref.clone(),
            handle,
        },
    );
    udp::emit_status(&app, &engine);
    Ok(())
}

/// 停止单条心跳任务
pub fn stop_heartbeat<R: Runtime>(app: &AppHandle<R>, engine: &EngineState, binding_id: i64) {
    if let Some((_, t)) = engine.heartbeats.remove(&binding_id) {
        t.handle.abort();
    }
    udp::emit_status(app, engine);
}

/// 停止某节点全部心跳任务（停监听 / 删节点时联动）
pub fn stop_node_heartbeats<R: Runtime>(app: &AppHandle<R>, engine: &EngineState, node_id: i64) {
    let ids: Vec<i64> = engine
        .heartbeats
        .iter()
        .filter(|r| r.value().node_id == node_id)
        .map(|r| *r.key())
        .collect();
    let had = !ids.is_empty();
    for id in ids {
        if let Some((_, t)) = engine.heartbeats.remove(&id) {
            t.handle.abort();
        }
    }
    if had {
        udp::emit_status(app, engine);
    }
}

/// 停止引用某协议的全部心跳任务（删协议时联动）
pub fn stop_protocol_heartbeats<R: Runtime>(app: &AppHandle<R>, engine: &EngineState, protocol_ref: &str) {
    let ids: Vec<i64> = engine
        .heartbeats
        .iter()
        .filter(|r| r.value().protocol_ref == protocol_ref)
        .map(|r| *r.key())
        .collect();
    let had = !ids.is_empty();
    for id in ids {
        if let Some((_, t)) = engine.heartbeats.remove(&id) {
            t.handle.abort();
        }
    }
    if had {
        udp::emit_status(app, engine);
    }
}

// ==================== 周期发送（控制台, 不落库） ====================

/// 启动某协议的周期发送（入网/心跳等, 按间隔重复调用发送渲染）
pub async fn start_periodic_send<R: Runtime>(
    app: AppHandle<R>,
    pool: SqlitePool,
    engine: Arc<EngineState>,
    node_id: i64,
    protocol_id: &str,
    interval_ms: i64,
) -> AppResult<()> {
    start_periodic_send_with_params(app, pool, engine, node_id, protocol_id, interval_ms, HashMap::new()).await
}

pub async fn start_periodic_send_with_params<R: Runtime>(
    app: AppHandle<R>, pool: SqlitePool, engine: Arc<EngineState>, node_id: i64,
    protocol_id: &str, interval_ms: i64, params: HashMap<String,String>,
) -> AppResult<()> {
    use crate::engine::state::PeriodicKey;

    let node = crate::db::get_node(&pool, node_id).await?;
    let protocol = crate::db::get_protocol(&pool, protocol_id).await?;
    let identity = params.get("__terminal_id").or_else(||params.get("__device_key")).cloned().unwrap_or_default();
    let key = format!("{node_id}:{protocol_id}:{identity}");

    if let Some((_, (_, h))) = engine.periodic.remove(&key) {
        h.abort();
    }
    udp::ensure_server(&app, &pool, &engine, &node).await?;

    let interval = Duration::from_millis(interval_ms.clamp(100, 3_600_000) as u64);
    let app2 = app.clone();
    let pool2 = pool.clone();
    let engine2 = engine.clone();
    let node2 = node.clone();
    let protocol2 = protocol.clone();
    let key2 = key.clone();

    let handle = tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            if let Err(e) =
                udp::send_datagram(&app2, &pool2, &engine2, &node2, &protocol2, &params, None)
                    .await
            {
                let local = format!("{}:{}", node2.local_ip, node2.local_port);
                let remote = format!("{}:{}", node2.remote_ip, node2.remote_port);
                let _ = udp::push_log(
                    &app2,
                    &pool2,
                    Direction::Out,
                    Some(&node2),
                    &local,
                    &remote,
                    Some(&protocol2),
                    &[],
                    Some(format!("周期发送失败: {e}")),
                    "error",
                )
                .await;
            }
        }
    });

    engine.periodic.insert(key2, (PeriodicKey { node_id, protocol_id: protocol_id.to_string() }, handle));
    udp::emit_status(&app, &engine);
    Ok(())
}

/// 停止周期发送
pub fn stop_periodic_send<R: Runtime>(app: &AppHandle<R>, engine: &EngineState, node_id: i64, protocol_id: &str) {
    let keys: Vec<String> = engine.periodic.iter().filter(|x| x.value().0.node_id == node_id && x.value().0.protocol_id == protocol_id).map(|x|x.key().clone()).collect();
    for key in keys { if let Some((_, (_, h))) = engine.periodic.remove(&key) { h.abort(); } }
    udp::emit_status(app, engine);
}
