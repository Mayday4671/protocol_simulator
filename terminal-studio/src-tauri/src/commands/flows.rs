//! 业务流程: OA 式步骤图编排(画布拖拽), 三种触发(手动/收包自动/定时)。

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime, State};

use simterm_core::error::AppResult;

use crate::engine::flow::{self, FlowStep};
use crate::AppState;

#[derive(Serialize)]
pub struct FlowRow {
    pub id: i64,
    pub name: String,
    pub system_ref: Option<i64>,
    pub group_ref: Option<i64>,
    pub description: Option<String>,
    pub steps: Vec<FlowStep>,
    pub trigger_protocols: Vec<String>,
    pub schedule_interval_ms: i64,
}

#[derive(Deserialize)]
pub struct FlowPayload {
    pub id: i64,
    pub name: String,
    pub system_ref: Option<i64>,
    #[serde(default)]
    pub group_ref: Option<i64>,
    pub description: Option<String>,
    pub steps: Vec<FlowStep>,
    #[serde(default)]
    pub trigger_protocols: Vec<String>,
    #[serde(default)]
    pub schedule_interval_ms: i64,
}

#[allow(clippy::too_many_arguments)]
fn row_of(
    id: i64,
    name: String,
    system_ref: Option<i64>,
    group_ref: Option<i64>,
    description: Option<String>,
    steps_json: String,
    trigger_json: Option<String>,
    schedule: i64,
) -> FlowRow {
    FlowRow {
        id,
        name,
        system_ref,
        group_ref,
        description,
        steps: serde_json::from_str(&steps_json).unwrap_or_default(),
        trigger_protocols: trigger_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default(),
        schedule_interval_ms: schedule,
    }
}

#[tauri::command]
pub async fn list_flows(pool: tauri::State<'_, AppState>) -> AppResult<Vec<FlowRow>> {
    let rows = sqlx::query_as::<_, (i64, String, Option<i64>, Option<i64>, Option<String>, String, Option<String>, i64)>(
        "SELECT id,name,system_ref,group_ref,description,steps_json,trigger_protocols_json,schedule_interval_ms FROM business_flows ORDER BY id",
    )
    .fetch_all(&pool.pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(a, b, c, d, e, f, g, h)| row_of(a, b, c, d, e, f, g, h))
        .collect())
}

#[tauri::command]
pub async fn upsert_flow(pool: tauri::State<'_, AppState>, payload: FlowPayload) -> AppResult<FlowRow> {
    if payload.name.trim().is_empty() {
        return Err(simterm_core::error::AppError::msg("请填写流程名称"));
    }
    crate::db::validate_system_scope(&pool.pool, payload.system_ref, payload.group_ref).await?;
    let steps_json = serde_json::to_string(&payload.steps)
        .map_err(|e| simterm_core::error::AppError::msg(format!("步骤内容无效: {e}")))?;
    let trigger_json = if payload.trigger_protocols.is_empty() {
        None
    } else {
        serde_json::to_string(&payload.trigger_protocols).ok()
    };
    let mut dup = std::collections::HashSet::new();
    for s in &payload.steps {
        if s.id.trim().is_empty() || !dup.insert(s.id.trim().to_string()) {
            return Err(simterm_core::error::AppError::msg("步骤标识不能为空且不能重复"));
        }
    }
    if payload.id > 0 {
        sqlx::query(
            "UPDATE business_flows SET name=?,system_ref=?,group_ref=?,description=?,steps_json=?,trigger_protocols_json=?,schedule_interval_ms=?,updated_at=datetime('now','localtime') WHERE id=?",
        )
        .bind(payload.name.trim())
        .bind(payload.system_ref)
        .bind(payload.group_ref)
        .bind(&payload.description)
        .bind(&steps_json)
        .bind(&trigger_json)
        .bind(payload.schedule_interval_ms)
        .bind(payload.id)
        .execute(&pool.pool)
        .await?;
    } else {
        sqlx::query(
            "INSERT INTO business_flows(name,system_ref,group_ref,description,steps_json,trigger_protocols_json,schedule_interval_ms) VALUES(?,?,?,?,?,?,?)",
        )
        .bind(payload.name.trim())
        .bind(payload.system_ref)
        .bind(payload.group_ref)
        .bind(&payload.description)
        .bind(&steps_json)
        .bind(&trigger_json)
        .bind(payload.schedule_interval_ms)
        .execute(&pool.pool)
        .await?;
    }
    let id = if payload.id > 0 { payload.id } else {
        sqlx::query_scalar::<_, i64>("SELECT MAX(id) FROM business_flows")
            .fetch_one(&pool.pool)
            .await?
    };
    // 内嵌信令同步: 生成/更新协议库协议并回填步骤引用
    let mut def = flow::FlowDef {
        id,
        name: payload.name.trim().to_string(),
        system_ref: payload.system_ref,
        group_ref: payload.group_ref,
        description: payload.description.clone(),
        steps: payload.steps.clone(),
        trigger_protocols: payload.trigger_protocols.clone(),
        schedule_interval_ms: payload.schedule_interval_ms,
    };
    flow::sync_inline_signals(&pool.pool, &mut def).await?;
    let steps_json = serde_json::to_string(&def.steps)
        .map_err(|e| simterm_core::error::AppError::msg(format!("步骤序列化失败: {e}")))?;
    Ok(row_of(id, def.name, def.system_ref, def.group_ref, def.description, steps_json, trigger_json, def.schedule_interval_ms))
}

#[tauri::command]
pub async fn delete_flow(state: State<'_, AppState>, id: i64) -> AppResult<()> {
    let runs: Vec<i64> = state.engine.flow_runs.iter().filter(|r|r.value().0==id).map(|r|*r.key()).collect();
    for run in runs { flow::stop_run(&state.engine,run); }
    sqlx::query("DELETE FROM business_flows WHERE id=?")
        .bind(id)
        .execute(&state.pool)
        .await?;
    Ok(())
}

#[tauri::command]
pub async fn start_flow_run<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    flow_id: i64,
    terminal_ids: Vec<i64>,
    node_id: Option<i64>,
    manual: Option<bool>,
) -> AppResult<i64> {
    crate::engine::flow_trace::start(app, state.pool.clone(), state.engine.clone(), flow_id, terminal_ids, node_id, manual.unwrap_or(false)).await
}

#[tauri::command]
pub fn flow_traces(state: State<'_, AppState>, flow_id: i64) -> Vec<crate::engine::flow_trace::FlowTrace> {
    crate::engine::flow_trace::traces(&state.engine,flow_id)
}

#[tauri::command]
pub async fn stop_flow_run(state: State<'_, AppState>, run_id: i64) -> AppResult<()> {
    if !flow::stop_run(&state.engine, run_id) {
        return Err(simterm_core::error::AppError::msg("该流程运行已结束"));
    }
    Ok(())
}

#[derive(Serialize)]
pub struct FlowRunInfo {
    pub run_id: i64,
    pub flow_id: i64,
    pub node_id: i64,
}

#[tauri::command]
pub async fn flow_runs(state: State<'_, AppState>) -> AppResult<Vec<FlowRunInfo>> {
    Ok(state
        .engine
        .flow_runs
        .iter()
        .map(|r| FlowRunInfo { run_id: *r.key(), flow_id: r.value().0, node_id: r.value().1 })
        .collect())
}

/// 定时触发扫描: 间隔到点且未在运行的定时流程自动启动(应用启动时拉起, 常驻)
pub fn spawn_flow_scheduler<R: Runtime + 'static>(app: AppHandle<R>, pool: sqlx::SqlitePool) {
    use tauri::Manager;
    // setup 阶段处于 block_on 上下文, 必须经 tauri 运行时句柄起任务
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(15)).await;
            let rows: Vec<(i64, i64)> = sqlx::query_as(
                "SELECT id, schedule_interval_ms FROM business_flows WHERE schedule_interval_ms>0",
            )
            .fetch_all(&pool)
            .await
            .unwrap_or_default();
            if rows.is_empty() {
                continue;
            }
            let state = app.state::<crate::AppState>();
            for (fid, interval) in rows {
                if state.engine.flow_runs.iter().any(|r| r.value().0 == fid) {
                    continue;
                }
                let key = format!("sched:{fid}");
                let now = std::time::Instant::now();
                let ready = {
                    let mut guard = state.engine.flow_schedule_marks.lock().await;
                    let last = guard
                        .get(&key)
                        .copied()
                        .unwrap_or(now - std::time::Duration::from_millis(interval.max(1000) as u64));
                    if now.duration_since(last) >= std::time::Duration::from_millis(interval.max(1000) as u64) {
                        guard.insert(key, now);
                        true
                    } else {
                        false
                    }
                };
                if ready {
                    let _ = flow::start_flow(app.clone(), pool.clone(), state.engine.clone(), fid, Vec::new()).await;
                }
            }
        }
    });
}
