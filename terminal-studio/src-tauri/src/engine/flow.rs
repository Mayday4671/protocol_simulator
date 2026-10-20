//! 业务流程执行器: OA 式步骤图驱动协议交互链。
//!
//! 每个步骤 = 发送某协议(带终端上下文/档案参数) → 期待某应答(超时) → 按结果分支:
//!   成功 = 应答命中且 role=Result 字段值 == 绑定字典第一条(引擎既有口径);
//!   失败 = 应答命中但结果不符; 超时 = 期望时间内未收到。
//! 逐台遍历模式对每台终端各执行一遍, 按"最差结果"路由(任一失败走失败边, 不中断其余终端)。
//! 捕获(令牌/会话/字段)由收包引擎按 role 语义自动完成, 流程零配置。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use tauri::{AppHandle, Emitter, Runtime};

use simterm_core::error::{AppError, AppResult};
use simterm_core::fields::extract_field_values;
use simterm_core::models::{FieldRole, Node};
use simterm_core::render::{compact_hex, DictMap};

use crate::db;
use crate::engine::state::EngineState;
use crate::engine::udp;

pub const EVENT_FLOW_RUN: &str = "flow-run";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FlowStep {
    pub id: String,
    pub name: String,
    /// 画布坐标(仅前端使用)
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
    /// 发送的协议
    pub protocol_id: String,
    /// single = 单台; each = 逐台遍历
    #[serde(default)]
    pub terminal_mode: String,
    /// single 模式固定设备(空 = 运行时选中的第一台)
    #[serde(default)]
    pub terminal_id: Option<i64>,
    /// 字段覆盖值(字段名 → 值)
    #[serde(default)]
    pub params: HashMap<String, String>,
    /// 期待应答协议(空 = 只发不等)
    #[serde(default)]
    pub expect_protocol: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout_ms: i64,
    #[serde(default)]
    pub delay_ms: i64,
    /// 无期待应答时的下一跳
    pub next: Option<String>,
    #[serde(default)]
    pub on_success: Option<String>,
    #[serde(default)]
    pub on_failure: Option<String>,
    #[serde(default)]
    pub on_timeout: Option<String>,
    /// 内嵌发送信令定义(保存时自动同步为协议库协议并回填 protocol_id)
    #[serde(default)]
    pub request_def: Option<SignalDef>,
    /// 内嵌期待应答信令定义(保存时自动同步为协议库应答协议)
    #[serde(default)]
    pub expect_def: Option<SignalDef>,
    /// 按接收内容分流: 应答成功后按字段值路由(字典含义或 hex, 首条命中生效; 无命中走 on_success)
    #[serde(default)]
    pub routes: Vec<RouteRule>,
}

/// 内嵌信令定义: 名称 + 完整字段表(复用协议字段模型, 含 when/unit 等全部能力)
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct SignalDef {
    #[serde(default)]
    pub header_refs: Option<Vec<String>>,
    pub name: String,
    #[serde(default)]
    pub fields: Vec<simterm_core::models::ProtocolField>,
}

/// 内容分流规则: 字段=值 → 去向
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RouteRule {
    pub field: String,
    pub value: String,
    pub to: String,
}

fn default_timeout() -> i64 {
    3000
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FlowDef {
    pub id: i64,
    pub name: String,
    pub system_ref: Option<i64>,
    /// 归属分组(内嵌信令生成的协议挂此分组)
    #[serde(default)]
    pub group_ref: Option<i64>,
    #[serde(default)]
    pub description: Option<String>,
    pub steps: Vec<FlowStep>,
    #[serde(default)]
    pub trigger_protocols: Vec<String>,
    #[serde(default)]
    pub schedule_interval_ms: i64,
}

/// 从库行解析流程定义
pub fn parse_flow(row: &(i64, String, Option<i64>, Option<i64>, Option<String>, String, Option<String>, i64)) -> AppResult<FlowDef> {
    let (_, name, system_ref, group_ref, description, steps_json, trigger_json, schedule) = row;
    Ok(FlowDef {
        id: row.0,
        name: name.clone(),
        system_ref: *system_ref,
        group_ref: *group_ref,
        description: description.clone(),
        steps: serde_json::from_str(steps_json).unwrap_or_default(),
        trigger_protocols: trigger_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default(),
        schedule_interval_ms: *schedule,
    })
}

pub async fn get_flow(pool: &SqlitePool, id: i64) -> AppResult<FlowDef> {
    let row = sqlx::query_as::<_, (i64, String, Option<i64>, Option<i64>, Option<String>, String, Option<String>, i64)>(
        "SELECT id,name,system_ref,group_ref,description,steps_json,trigger_protocols_json,schedule_interval_ms FROM business_flows WHERE id=?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::msg(format!("业务流程 {id} 不存在")))?;
    parse_flow(&row)
}

/// 系统派生通道: 绑定了该系统任一分组的第一个通道(与控制台同口径)
async fn node_for_system(pool: &SqlitePool, system_ref: Option<i64>) -> AppResult<Node> {
    let Some(sid) = system_ref else {
        return Err(AppError::msg("流程未归属协议系统, 无法确定通道"));
    };
    let group_ids: Vec<i64> =
        sqlx::query_scalar("SELECT id FROM protocol_groups WHERE system_ref=?")
            .bind(sid)
            .fetch_all(pool)
            .await?;
    for n in db::list_nodes(pool).await? {
        let binds: Vec<i64> = serde_json::from_str(&n.group_bind_json.clone().unwrap_or_default())
            .unwrap_or_default();
        if binds.iter().any(|b| group_ids.contains(b)) {
            return Ok(n);
        }
    }
    Err(AppError::msg("该协议系统的分组未绑定任何通道, 请先在通道管理中绑定"))
}


/// 内嵌信令同步入库: 步骤带 request_def/expect_def 且未引用协议时,
/// 按 流程+步骤 派生稳定 protocol_id 生成(或更新)协议, 并回填步骤引用。
/// 再次保存按同 id 更新, 不会重复建协议。
pub async fn sync_inline_signals(pool: &SqlitePool, flow: &mut FlowDef) -> AppResult<()> {
    use simterm_core::models::{MsgType, ProtocolPayload};
    let Some(system_ref) = flow.system_ref else {
        return Err(AppError::msg("流程未归属系统, 无法保存内嵌信令"));
    };
    let group_ref = match flow.group_ref {
        Some(g) => Some(g),
        None => sqlx::query_scalar::<_, i64>("SELECT MIN(id) FROM protocol_groups WHERE system_ref=?")
            .bind(system_ref)
            .fetch_optional(pool)
            .await?,
    };
    let Some(group_ref) = group_ref else {
        return Err(AppError::msg("该系统下没有分组, 请先在协议管理中创建分组(内嵌信令需挂在分组下)"));
    };
    let sanitize = |src: &str| -> String {
        src.chars().filter(|c| c.is_ascii_alphanumeric()).take(24).collect()
    };
    for step in flow.steps.iter_mut() {
        if step.protocol_id.trim().is_empty() {
            if let Some(def) = &step.request_def {
                let pid = format!("flw{}_{}req", sanitize(&flow.name), sanitize(&step.id));
                upsert_signal_protocol(pool, &pid, def, MsgType::Request, system_ref, group_ref, &flow.name).await?;
                step.protocol_id = pid;
            }
        }
        if step.expect_protocol.as_deref().map(str::trim).unwrap_or("").is_empty() {
            if let Some(def) = &step.expect_def {
                let pid = format!("flw{}_{}ack", sanitize(&flow.name), sanitize(&step.id));
                upsert_signal_protocol(pool, &pid, def, MsgType::Response, system_ref, group_ref, &flow.name).await?;
                step.expect_protocol = Some(pid);
            }
        }
    }
    let steps_json = serde_json::to_string(&flow.steps)
        .map_err(|e| AppError::msg(format!("步骤序列化失败: {e}")))?;
    sqlx::query("UPDATE business_flows SET steps_json=?,group_ref=? WHERE id=?")
        .bind(&steps_json)
        .bind(group_ref)
        .bind(flow.id)
        .execute(pool)
        .await?;
    flow.group_ref = Some(group_ref);
    Ok(())
}

async fn upsert_signal_protocol(
    pool: &SqlitePool,
    pid: &str,
    def: &SignalDef,
    msg_type: simterm_core::models::MsgType,
    system_ref: i64,
    group_ref: i64,
    flow_name: &str,
) -> AppResult<()> {
    use simterm_core::models::{DefMode, PayloadFormat, ProtocolPayload};
    let payload = ProtocolPayload {
        header_refs: def.header_refs.clone(),
        protocol_id: pid.to_string(),
        name: if def.name.trim().is_empty() { pid.to_string() } else { def.name.trim().to_string() },
        format: PayloadFormat::Hex,
        msg_type,
        def_mode: DefMode::Fields,
        fields: def.fields.clone(),
        payload_template: String::new(),
        match_rule: None,
        match_fields: Vec::new(),
        auto_respond: None,
        respond_delay_ms: 0,
        auto_periodic: None,
        auto_periodic_interval_ms: 0,
        description: Some(format!("由业务流程「{flow_name}」内嵌定义自动生成")),
        system_ref: Some(system_ref),
        group_ref: Some(group_ref),
    };
    crate::commands::protocols::validate_and_upsert(pool, &payload).await
}

#[derive(Serialize, Clone)]
pub struct FlowRunEvent {
    pub run_id: i64,
    pub flow_id: i64,
    pub flow_name: String,
    pub step_id: String,
    pub step_name: String,
    /// 终端设备标识(空 = 不针对特定终端)
    pub terminal: Option<String>,
    /// running | success | failure | timeout | done | aborted | error
    pub status: String,
    pub note: String,
}

fn emit_step<R: Runtime>(app: &AppHandle<R>, ev: FlowRunEvent) {
    let _ = app.emit(EVENT_FLOW_RUN, &ev);
}

/// 启动一次流程运行; terminal_ids 为运行时选中的设备(空 = each 模式取该系统全部设备)
pub async fn start_flow<R: Runtime>(
    app: AppHandle<R>,
    pool: SqlitePool,
    engine: Arc<EngineState>,
    flow_id: i64,
    terminal_ids: Vec<i64>,
) -> AppResult<i64> {
    let flow = get_flow(&pool, flow_id).await?;
    if flow.steps.is_empty() {
        return Err(AppError::msg("流程没有任何步骤"));
    }
    let node = node_for_system(&pool, flow.system_ref).await?;
    udp::ensure_server(&app, &pool, &engine, &node).await?;
    let run_id = engine.next_flow_run_id();
    let app2 = app.clone();
    let pool2 = pool.clone();
    let engine2 = engine.clone();
    let node_id = node.id;
    let system_ref = flow.system_ref.unwrap_or(-1);
    let flow_name = flow.name.clone();
    let handle = tokio::spawn(async move {
        let result = run_steps(app2.clone(), pool2.clone(), engine2.clone(), run_id, flow_id, flow_name, system_ref, node_id, terminal_ids, &flow.steps).await;
        // 无论成功失败, 运行结束都要摘除句柄并广播收尾
        engine2.flow_runs.remove(&run_id);
        let (status, note) = match result {
            Ok(()) => ("done", "流程执行完成".to_string()),
            Err(e) => ("error", format!("流程中止: {e}")),
        };
        emit_step(&app2, FlowRunEvent {
            run_id, flow_id, flow_name: String::new(), step_id: String::new(), step_name: String::new(),
            terminal: None, status: status.into(), note,
        });
    });
    engine.flow_runs.insert(run_id, (flow_id, node_id, handle));
    Ok(run_id)
}

pub fn stop_run(engine: &EngineState, run_id: i64) -> bool {
    engine.flow_runs.remove(&run_id).map(|(_, (_, _, h))| h.abort()).is_some()
}

#[derive(Clone, Copy, PartialEq)]
enum Outcome {
    Success,
    Failure,
    Timeout,
}

async fn run_steps<R: Runtime>(
    app: AppHandle<R>,
    pool: SqlitePool,
    engine: Arc<EngineState>,
    run_id: i64,
    flow_id: i64,
    flow_name: String,
    system_ref: i64,
    node_id: i64,
    terminal_ids: Vec<i64>,
    steps: &[FlowStep],
) -> AppResult<()> {
    let ev = |step: &FlowStep, terminal: Option<String>, status: &str, note: String| FlowRunEvent {
        run_id, flow_id, flow_name: flow_name.clone(), step_id: step.id.clone(),
        step_name: step.name.clone(), terminal, status: status.into(), note,
    };
    // 运行设备集合: 选中的优先, 否则该系统全部设备
    let mut terminals: Vec<(i64, String)> = Vec::new();
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, device_key FROM terminal_profiles WHERE node_ref=? AND system_ref=? ORDER BY id",
    )
    .bind(node_id)
    .bind(system_ref)
    .fetch_all(&pool)
    .await?;
    for (id, key) in rows {
        if terminal_ids.is_empty() || terminal_ids.contains(&id) {
            terminals.push((id, key));
        }
    }
    let mut current = Some(&steps[0]);
    let mut guard = 0;
    while let Some(step) = current {
        guard += 1;
        if guard > 1000 {
            return Err(AppError::msg("流程步数超限(疑似循环), 已中止"));
        }
        let step_terminals: Vec<(i64, String)> = if step.terminal_mode == "each" {
            terminals.clone()
        } else {
            let pick = step
                .terminal_id
                .and_then(|id| terminals.iter().find(|(tid, _)| *tid == id).cloned())
                .or_else(|| terminals.first().cloned());
            match pick {
                Some(t) => vec![t],
                None => {
                    emit_step(&app, ev(step, None, "skipped", "没有可用设备, 跳过".into()));
                    current = step.next.as_deref().and_then(|id| steps.iter().find(|s| s.id == id));
                    continue;
                }
            }
        };
        let mut worst = Outcome::Success;
        let mut routed: Option<String> = None;
        for (tid, tkey) in step_terminals {
            if step.delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(step.delay_ms as u64)).await;
            }
            emit_step(&app, ev(step, Some(tkey.clone()), "running", String::new()));
            let protocol = match db::get_protocol(&pool, &step.protocol_id).await {
                Ok(p) => p,
                Err(e) => {
                    emit_step(&app, ev(step, Some(tkey), "failure", format!("协议不存在: {e}")));
                    worst = Outcome::Failure;
                    continue;
                }
            };
            let mut params = step.params.clone();
            params.insert("__terminal_id".into(), tid.to_string());
            // 与控制台命令同口径: 每次发送前重取节点(拿到最新捕获桶/会话)
            let node = db::get_node(&pool, node_id).await?;
            if let Err(e) = udp::send_datagram(&app, &pool, &engine, &node, &protocol, &params, None).await {
                emit_step(&app, ev(step, Some(tkey), "failure", format!("发送失败: {e}")));
                worst = Outcome::Failure;
                continue;
            }
            let out_base: i64 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(id),0) FROM logs WHERE node_ref=? AND direction='out'",
            )
            .bind(node_id)
            .fetch_one(&pool)
            .await?;
            let expect = step.expect_protocol.as_deref().map(str::trim).filter(|s| !s.is_empty());
            match expect {
                None => {
                    emit_step(&app, ev(step, Some(tkey), "success", "已发送".into()));
                }
                Some(expect_id) => {
                    match wait_reply(&pool, node_id, out_base, expect_id, step.timeout_ms).await? {
                        None => {
                            emit_step(&app, ev(step, Some(tkey), "timeout", format!("超时未收到「{}」", expect_id)));
                            worst = Outcome::Timeout;
                        }
                        Some(raw_hex) => {
                            let (ok, note, values) = judge_reply(&pool, expect_id, &raw_hex).await?;
                            let status = if ok { "success" } else { "failure" };
                            let route_note = if ok && !step.routes.is_empty() {
                                match match_route(&pool, expect_id, &step.routes, &values).await? {
                                    Some(to) => {
                                        routed = Some(to.clone());
                                        let to_name = steps.iter().find(|x| x.id == to).map(|x| x.name.clone()).unwrap_or(to);
                                        format!(" → 按内容分流至「{to_name}」")
                                    }
                                    None => String::new(),
                                }
                            } else {
                                String::new()
                            };
                            emit_step(&app, ev(step, Some(tkey), status, format!("{note}{route_note}")));
                            if !ok {
                                worst = Outcome::Failure;
                            }
                        }
                    }
                }
            }
        }
        // 路由: 内容分流命中优先, 其次三态分支(逐台遍历取最差结果)
        current = routed
            .or(match worst {
                Outcome::Success => step.on_success.clone().or(step.next.clone()),
                Outcome::Failure => step.on_failure.clone().or(step.next.clone()),
                Outcome::Timeout => step.on_timeout.clone().or(step.next.clone()),
            })
            .and_then(|id| steps.iter().find(|s| s.id == id));
    }
    Ok(())
}

/// 轮询日志等待应答帧, 返回原始 hex(无则超时)
async fn wait_reply(pool: &SqlitePool, node_id: i64, base: i64, expect_id: &str, timeout_ms: i64) -> AppResult<Option<String>> {
    let timeout_ms = timeout_ms.clamp(200, 60_000) as u64;
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout_ms);
    loop {
        let hit: Option<String> = sqlx::query_scalar(
            "SELECT raw_hex FROM logs WHERE id>? AND node_ref=? AND direction='in' AND protocol_id=? ORDER BY id LIMIT 1",
        )
        .bind(base)
        .bind(node_id)
        .bind(expect_id)
        .fetch_optional(pool)
        .await?;
        if let Some(raw) = hit {
            return Ok(Some(raw));
        }
        if tokio::time::Instant::now() >= deadline {
            return Ok(None);
        }
        tokio::time::sleep(Duration::from_millis(80)).await;
    }
}

/// 内容分流匹配: 规则值支持 字典含义 或 hex(与断言值同口径), 首条命中返回去向
async fn match_route(
    pool: &SqlitePool,
    expect_id: &str,
    routes: &[RouteRule],
    values: &HashMap<String, String>,
) -> AppResult<Option<String>> {
    let p = db::get_protocol(pool, expect_id).await?;
    let fields = crate::devices::fields_for(pool, &p).await?;
    let dicts: DictMap = db::dict_map_for_group(pool, p.group_ref).await?;
    for r in routes {
        let Some(f) = fields.iter().find(|f| f.name == r.field) else { continue };
        let Some(got) = values.get(&r.field) else { continue };
        let got_c = compact_hex(got);
        let expect_c = if let Some(key) = f.dict_key() {
            match dicts.get(&key).and_then(|set| set.find_value(r.value.trim())) {
                Some(item_hex) => compact_hex(&item_hex),
                None => compact_hex(r.value.trim()),
            }
        } else {
            compact_hex(r.value.trim())
        };
        if !expect_c.is_empty() && got_c == expect_c {
            return Ok(Some(r.to.clone()));
        }
    }
    Ok(None)
}

/// 应答判定: role=Result 字段值 == 绑定字典第一条 → 成功; 无结果字段 → 收到即成功
async fn judge_reply(pool: &SqlitePool, expect_id: &str, raw_hex: &str) -> AppResult<(bool, String, HashMap<String, String>)> {
    let p = db::get_protocol(pool, expect_id).await?;
    let fields = crate::devices::fields_for(pool, &p).await?;
    let dicts: DictMap = db::dict_map_for_group(pool, p.group_ref).await?;
    let compact = compact_hex(raw_hex);
    let bytes = hex::decode(&compact).map_err(|_| AppError::msg("应答帧不是合法 Hex"))?;
    let values = extract_field_values(&fields, &bytes, &dicts);
    let Some(result) = fields.iter().find(|f| f.role == FieldRole::Result) else {
        return Ok((true, "收到应答".into(), values));
    };
    let Some(v) = values.get(&result.name) else {
        return Ok((false, "应答缺少结果字段".into(), values));
    };
    let got = compact_hex(v);
    let expect_value = result
        .dict_key()
        .and_then(|k| dicts.get(&k))
        .and_then(|s| s.default_value())
        .map(|d| compact_hex(d));
    let ok = match expect_value {
        Some(e) => got == e,
        None => got == "00",
    };
    let note = if ok { "应答成功".to_string() } else { format!("应答结果不符: {got}") };
    Ok((ok, note, values))
}

/// 收包自动触发: 命中协议出现在某流程的触发列表且该流程未在运行
pub async fn trigger_flows_for<R: Runtime>(app: &AppHandle<R>, pool: &SqlitePool, engine: &Arc<EngineState>, protocol_id: &str) {
    let rows: Vec<(i64, Option<String>)> = sqlx::query_as(
        "SELECT id, trigger_protocols_json FROM business_flows WHERE trigger_protocols_json IS NOT NULL",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();
    for (fid, tj) in rows {
        let triggers: Vec<String> = tj.as_deref().and_then(|s| serde_json::from_str(s).ok()).unwrap_or_default();
        if !triggers.iter().any(|t| t == protocol_id) {
            continue;
        }
        if engine.flow_runs.iter().any(|r| r.value().0 == fid) {
            continue; // 同流程已在跑, 不重复
        }
        let _ = start_flow(app.clone(), pool.clone(), engine.clone(), fid, Vec::new()).await;
    }
}
