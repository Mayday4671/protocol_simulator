//! A run is a frozen flow plus an ordered journal. UI events are notifications;
//! querying the journal recovers events emitted before IPC returned or while hidden.
use std::{collections::HashMap, sync::Arc, time::Duration};
use serde::Serialize;
use sqlx::SqlitePool;
use tauri::{AppHandle, Emitter, Runtime};
use simterm_core::{error::{AppError, AppResult}, models::{Direction, FieldRole, LogRecord, Protocol, ProtocolField}, render::{compact_hex, DictMap}, flow_match};
use crate::{db, devices};
use super::{flow::{self, FlowDef, FlowStep}, state::EngineState, udp};

#[derive(Clone, Serialize)]
pub struct TraceEvent {
    pub index: usize, pub at: i64,
    pub run_id: i64, pub flow_id: i64, pub flow_name: String,
    pub step_id: String, pub step_name: String, pub attempt: usize,
    pub terminal: Option<String>, pub status: String, pub note: String,
    pub packet: Option<LogRecord>, pub next_step: Option<String>, pub branch: Option<String>,
}
#[derive(Clone, Serialize)]
pub struct FlowTrace {
    pub run_id: i64, pub flow: FlowDef, pub node_id: i64, pub node_name: String,
    pub manual: bool, pub started_at: i64, pub ended_at: Option<i64>,
    pub terminal_ids: Vec<i64>,
    pub status: String, pub events: Vec<TraceEvent>, pub omitted: usize,
    pub step_states: HashMap<String, TraceEvent>,
    pub transitions: Vec<TraceEvent>,
}
pub fn traces(engine: &EngineState, flow_id: i64) -> Vec<FlowTrace> {
    engine.flow_traces.lock().unwrap().values().rev().filter(|t| t.flow.id == flow_id).cloned().collect()
}
fn record(engine: &EngineState, run: i64, step: Option<&FlowStep>, attempt: usize, terminal: Option<&str>, status: &str, note: &str, packet: Option<LogRecord>, next: Option<String>, branch: Option<String>) -> Option<TraceEvent> {
    let mut journal = engine.flow_traces.lock().unwrap();
    let trace = journal.get_mut(&run)?;
    if trace.ended_at.is_some() { return None; }
    let ev = TraceEvent {
        index: trace.events.len() + trace.omitted, at: chrono::Utc::now().timestamp_millis(), run_id: run,
        flow_id: trace.flow.id, flow_name: trace.flow.name.clone(),
        step_id: step.map(|s| s.id.clone()).unwrap_or_default(), step_name: step.map(|s| s.name.clone()).unwrap_or_default(),
        attempt, terminal: terminal.map(str::to_string), status: status.into(), note: note.into(), packet, next_step: next, branch,
    };
    if step.is_none() { trace.status = status.into(); if status != "running" { trace.ended_at = Some(ev.at); } }
    if step.is_some() && !["unmatched","sent"].contains(&status) {
        let mut state = ev.clone(); state.packet=None;
        if status=="transition" {
            state.status=note.into();
            if !trace.transitions.iter().any(|e|e.step_id==ev.step_id&&e.next_step==ev.next_step&&e.branch==ev.branch) { trace.transitions.push(ev.clone()); }
        }
        trace.step_states.insert(state.step_id.clone(),state);
    }
    if trace.events.len() >= 2000 { trace.events.remove(0); trace.omitted += 1; }
    trace.events.push(ev.clone());
    Some(ev)
}
fn emit<R: Runtime>(app: &AppHandle<R>, engine: &EngineState, run: i64, step: Option<&FlowStep>, attempt: usize, terminal: Option<&str>, status: &str, note: &str, packet: Option<LogRecord>) {
    if let Some(ev) = record(engine, run, step, attempt, terminal, status, note, packet, None, None) { let _ = app.emit(flow::EVENT_FLOW_RUN, ev); }
}
pub fn stop(engine: &EngineState, run: i64) -> bool {
    if let Some((_, (_, _, task))) = engine.flow_runs.remove(&run) {
        task.abort();
        record(engine, run, None, 0, None, "aborted", "已停止", None, None, None);
        true
    } else { false }
}

struct Schema { protocol: Protocol, fields: Vec<ProtocolField>, dicts: DictMap }
impl Schema {
    async fn load(pool: &SqlitePool, id: &str, system: Option<i64>) -> AppResult<Self> {
        let protocol = db::get_protocol(pool, id).await?;
        if protocol.system_ref != system { return Err(AppError::msg("步骤协议与流程所属系统不一致")); }
        let fields = devices::fields_for(pool, &protocol).await?;
        let dicts = db::dict_map_for_group(pool, protocol.group_ref).await?;
        Ok(Self { protocol, fields, dicts })
    }
    fn values(&self, log: &LogRecord) -> AppResult<HashMap<String,String>> {
        let bytes = hex::decode(compact_hex(log.raw_hex.as_deref().unwrap_or_default())).map_err(|_| AppError::msg("报文 HEX 无效"))?;
        // Template payloads are opaque; only their declared header has fields.
        let defined = if self.protocol.def_mode == simterm_core::models::DefMode::Template {
            simterm_core::datagram::parse_field_details(&self.fields,0,&bytes,&self.dicts).defined_len
        } else { bytes.len() };
        let rows = simterm_core::business::snapshot(&self.fields, &bytes[..defined], &self.dicts, "")?;
        Ok(rows[0].iter().map(|(k,v)| (k.strip_prefix("frame.").unwrap_or(k).to_string(),v.hex.clone())).collect())
    }
}
async fn watermark(pool: &SqlitePool) -> AppResult<i64> {
    Ok(sqlx::query_scalar("SELECT COALESCE(MAX(id),0) FROM logs").fetch_one(pool).await?)
}
async fn log_by_id(pool: &SqlitePool, id: i64) -> AppResult<LogRecord> {
    Ok(sqlx::query_as("SELECT * FROM logs WHERE id=?").bind(id).fetch_one(pool).await?)
}

pub async fn start<R: Runtime>(app: AppHandle<R>, pool: SqlitePool, engine: Arc<EngineState>, flow_id: i64, terminal_ids: Vec<i64>, node_id: Option<i64>, manual: bool) -> AppResult<i64> {
    let _start_lock = engine.flow_start_lock.lock().await;
    let flow = flow::get_flow(&pool, flow_id).await?;
    if flow.steps.is_empty() { return Err(AppError::msg("请先添加并保存步骤")); }
    let node = if let Some(id) = node_id { db::get_node(&pool,id).await? } else { flow::node_for_system(&pool,flow.system_ref).await? };
    let groups: Vec<i64> = sqlx::query_scalar("SELECT id FROM protocol_groups WHERE system_ref=?").bind(flow.system_ref).fetch_all(&pool).await?;
    let binds: Vec<i64> = serde_json::from_str(node.group_bind_json.as_deref().unwrap_or("[]")).unwrap_or_default();
    if !groups.iter().any(|g| binds.contains(g)) { return Err(AppError::msg("所选通道未绑定此流程的系统")); }
    // One observer/executor per channel prevents serial ACKs being consumed twice.
    if engine.flow_runs.iter().any(|r| r.value().1 == node.id) { return Err(AppError::msg("此通道已有流程在测试，请先停止当前测试")); }
    for s in &flow.steps {
        if !["", "send", "receive"].contains(&s.action.as_str()) { return Err(AppError::msg("步骤动作无效")); }
        if !(200..=300000).contains(&s.timeout_ms) || !(0..=300000).contains(&s.delay_ms) { return Err(AppError::msg("超时应为 200～300000 毫秒；延时应为 0～300000 毫秒")); }
        let req = Schema::load(&pool, &s.protocol_id, flow.system_ref).await?;
        let ack = if let Some(id) = s.expect_protocol.as_deref().filter(|v|!v.is_empty()) {
            if s.action == "receive" { return Err(AppError::msg("被动接收步骤请通过下一步配置后续交互，不再设置期待应答")); }
            let ack = Schema::load(&pool,id,flow.system_ref).await?;
            flow_match::pairs(&req.fields,&ack.fields,&s.correlation,s.allow_unkeyed_reply)?;
            Some(ack)
        } else { None };
        if !s.result_field.is_empty() {
            let schema = ack.as_ref().unwrap_or(&req);
            let field = schema.fields.iter().find(|f|f.name==s.result_field).ok_or_else(||AppError::msg("成功判定字段不存在"))?;
            let bytes = hex::decode(compact_hex(&s.success_hex)).map_err(|_|AppError::msg("成功值不是合法 HEX"))?;
            if bytes.len()!=field.length as usize || bytes.is_empty() { return Err(AppError::msg("成功值长度与字段不一致")); }
        }
        for to in [&s.next,&s.on_success,&s.on_failure,&s.on_timeout].into_iter().flatten().chain(s.routes.iter().map(|r|&r.to)) {
            if !flow.steps.iter().any(|v| &v.id==to) { return Err(AppError::msg("步骤分支指向了不存在的步骤")); }
        }
    }
    let device_rows: Vec<(i64,String)> = sqlx::query_as("SELECT id,device_key FROM terminal_profiles WHERE node_ref=? AND system_ref=? ORDER BY id").bind(node.id).bind(flow.system_ref).fetch_all(&pool).await?;
    if terminal_ids.iter().any(|id| !device_rows.iter().any(|d|d.0==*id)) { return Err(AppError::msg("所选设备不属于测试通道和系统")); }
    let terminals: Vec<_> = device_rows.into_iter().filter(|d|terminal_ids.contains(&d.0) || (node_id.is_none() && terminal_ids.is_empty())).collect();
    if flow.steps.iter().any(|s|s.terminal_mode=="each") && terminals.is_empty() { return Err(AppError::msg("逐台执行步骤需要选择设备")); }
    for s in &flow.steps { if s.terminal_id.is_some_and(|id|!terminals.iter().any(|d|d.0==id)) { return Err(AppError::msg("步骤固定设备不在本次选择中")); } }
    udp::ensure_server(&app,&pool,&engine,&node).await?;
    let base = watermark(&pool).await?;
    let run = engine.next_flow_run_id();
    {
        let mut traces = engine.flow_traces.lock().unwrap();
        let ended: Vec<i64> = traces.iter().filter(|(_,t)|t.ended_at.is_some()).map(|(id,_)|*id).collect();
        for id in ended.iter().take(ended.len().saturating_sub(49)) { traces.remove(id); }
        traces.insert(run, FlowTrace { run_id:run, flow:flow.clone(), node_id:node.id, node_name:node.name.clone(), manual, terminal_ids:terminals.iter().map(|d|d.0).collect(), started_at:chrono::Utc::now().timestamp_millis(), ended_at:None, status:"running".into(), events:vec![], omitted:0, step_states:HashMap::new(), transitions:vec![] });
    }
    let (ready_tx,ready_rx) = tokio::sync::oneshot::channel();
    let engine2 = engine.clone(); let node_id = node.id;
    let task = tokio::spawn(async move {
        if ready_rx.await.is_err() { return; }
        let result = execute(&app,&pool,&engine2,run,&flow,node_id,&terminals,manual,base).await;
        let (status,note) = match result { Ok(true)=>("done","流程完成".to_string()), Ok(false)=>("issues","流程结束，存在失败或超时步骤".into()), Err(e)=>("error",e.to_string()) };
        emit(&app,&engine2,run,None,0,None,status,&note,None);
        engine2.flow_runs.remove(&run);
    });
    engine.flow_runs.insert(run,(flow_id,node_id,task));
    let _ = ready_tx.send(());
    Ok(run)
}

struct WaitFor<'a> {
    schema: &'a Schema, direction: Direction, peer: &'a str,
    keys: &'a [flow_match::FieldPair], request: &'a HashMap<String,String>,
    device: Option<&'a str>,
}
impl WaitFor<'_> {
    fn accepts(&self, log: &LogRecord) -> Result<(), String> {
        if log.direction != self.direction { return Err("方向不符 / 发送或重发".into()); }
        if log.remote_addr != self.peer { return Err("其他对端".into()); }
        if log.protocol_id.as_deref() != Some(self.schema.protocol.protocol_id.as_str()) { return Err("其他信令".into()); }
        if !["sent", "ok"].contains(&log.status.as_str()) { return Err(format!("报文状态：{}", log.status)); }
        let values = self.schema.values(log).map_err(|e|e.to_string())?;
        if !flow_match::matches(self.keys,self.request,&values) { return Err("关联字段不符".into()); }
        if let Some(key) = self.device {
            let role = if self.direction == Direction::Out { FieldRole::SrcId } else { FieldRole::DstId };
            let field = self.schema.fields.iter().find(|f|f.role==role).or_else(|| self.schema.fields.iter().find(|f|f.role==FieldRole::DeviceSn));
            let Some(field) = field else { return Err("协议未定义当前设备身份字段".into()); };
            if values.get(&field.name) != Some(&compact_hex(key)) { return Err("其他设备".into()); }
        }
        Ok(())
    }
}

async fn wait<R: Runtime>(app: &AppHandle<R>,pool: &SqlitePool,engine: &EngineState,run: i64,step: &FlowStep,attempt: usize,terminal: Option<&str>, node: i64, cursor: &mut i64, skip: Option<i64>, target: WaitFor<'_>, seen: &mut std::collections::HashSet<i64>, consumed: &std::collections::HashSet<i64>) -> AppResult<Option<LogRecord>> {
    let deadline = tokio::time::Instant::now()+Duration::from_millis(step.timeout_ms as u64);
    loop {
        let logs: Vec<LogRecord> = sqlx::query_as("SELECT * FROM logs WHERE node_ref=? AND id>? ORDER BY id LIMIT 100")
            .bind(node).bind(*cursor).fetch_all(pool).await?;
        let empty = logs.is_empty();
        for log in logs {
            *cursor=log.id;
            if Some(log.id)==skip || consumed.contains(&log.id) { continue; }
            match target.accepts(&log) {
                Ok(()) => { return Ok(Some(log)); }
                Err(note) => {
                    if seen.insert(log.id) { emit(app,engine,run,Some(step),attempt,terminal,"unmatched",&note,Some(log)); }
                }
            }
        }
        if tokio::time::Instant::now() >= deadline { return Ok(None); }
        if empty { tokio::time::sleep(Duration::from_millis(40)).await; }
    }
}

async fn execute<R: Runtime>(app: &AppHandle<R>,pool: &SqlitePool,engine: &Arc<EngineState>,run: i64,flow: &FlowDef,node_id:i64,terminals:&[(i64,String)],manual:bool,mut cursor:i64) -> AppResult<bool> {
    let mut current=Some(flow.steps[0].id.clone()); let mut attempt=0; let mut all_ok=true;
    let mut seen=std::collections::HashSet::new();
    let mut consumed=std::collections::HashSet::new();
    while let Some(id)=current {
        let step=flow.steps.iter().find(|s|s.id==id).ok_or_else(||AppError::msg("分支步骤不存在"))?;
        let selected:Vec<Option<&(i64,String)>> = if step.terminal_mode=="each" { terminals.iter().map(Some).collect() }
            else { vec![step.terminal_id.and_then(|id|terminals.iter().find(|d|d.0==id)).or_else(||terminals.first())] };
        let mut outcome="success"; let mut content_route=None;
        for device in selected {
            attempt+=1;
            if attempt>1000 { return Err(AppError::msg("执行超过 1000 步，已停止循环")); }
            let terminal=device.map(|d|d.1.as_str());
            let req=Schema::load(pool,&step.protocol_id,flow.system_ref).await?;
            let node=db::get_node(pool,node_id).await?;
            let peer=tokio::net::lookup_host((node.remote_ip.as_str(),node.remote_port as u16)).await?.next().ok_or_else(||AppError::msg("对端地址无效"))?.to_string();
            let passive=step.action=="receive";
            if step.delay_ms>0 {
                emit(app,engine,run,Some(step),attempt,terminal,"delay","前置延时",None);
                tokio::time::sleep(Duration::from_millis(step.delay_ms as u64)).await;
            }
            let start_cursor=if manual || passive { cursor } else { watermark(pool).await? };
            let mut scan=start_cursor;
            let packet = if manual || passive {
                emit(app,engine,run,Some(step),attempt,terminal,if passive {"waiting"} else {"waiting_send"},if passive {"等待对端发送"} else {"等待手动发送"},None);
                wait(app,pool,engine,run,step,attempt,terminal,node_id,&mut scan,None,WaitFor {schema:&req,direction:if passive {Direction::In} else {Direction::Out},peer:&peer,keys:&[],request:&HashMap::new(),device:terminal},&mut seen,&consumed).await?
            } else {
                emit(app,engine,run,Some(step),attempt,terminal,"running","发送中",None);
                let mut params=step.params.clone();
                if let Some((tid,_))=device { params.insert("__terminal_id".into(),tid.to_string()); }
                match udp::send_datagram(app,pool,engine,&node,&req.protocol,&params,None).await {
                    Ok(sent)=>Some(log_by_id(pool,sent.log_id).await?),
                    Err(e)=>{
                        emit(app,engine,run,Some(step),attempt,terminal,"failure",&format!("发送失败：{e}"),None);
                        outcome="failure"; all_ok=false; continue;
                    }
                }
            };
            let Some(packet)=packet else {
                cursor=scan; emit(app,engine,run,Some(step),attempt,terminal,"timeout","等待超时",None); outcome="timeout";all_ok=false;continue;
            };
            cursor=packet.id; seen.insert(packet.id); consumed.insert(packet.id);
            let expect=step.expect_protocol.as_deref().filter(|v|!v.is_empty());
            let (reply,schema) = if let Some(expect)=expect {
                let ack=Schema::load(pool,expect,flow.system_ref).await?;
                let keys=flow_match::pairs(&req.fields,&ack.fields,&step.correlation,step.allow_unkeyed_reply)?;
                let values=req.values(&packet)?;
                emit(app,engine,run,Some(step),attempt,terminal,"sent","已发送",Some(packet.clone()));
                emit(app,engine,run,Some(step),attempt,terminal,"waiting",if step.allow_unkeyed_reply && step.correlation.is_empty() {"等待应答（串行匹配）"} else {"等待关联应答"},None);
                // Read from the pre-send watermark: a fast reply can precede the OUT log commit.
                scan=packet.send_after_log_id.unwrap_or(packet.id);
                let reply=wait(app,pool,engine,run,step,attempt,terminal,node_id,&mut scan,Some(packet.id),WaitFor {schema:&ack,direction:Direction::In,peer:&packet.remote_addr,keys:&keys,request:&values,device:None},&mut seen,&consumed).await?;
                cursor=scan;
                (reply,ack)
            } else if passive { (Some(packet),req) }
            else {
                emit(app,engine,run,Some(step),attempt,terminal,"success","已发送",Some(packet));
                continue;
            };
            match reply {
                None=>{ emit(app,engine,run,Some(step),attempt,terminal,"timeout","等待应答超时",None); outcome="timeout";all_ok=false; }
                Some(reply)=>{
                    consumed.insert(reply.id);
                    let (mut ok,mut note,values)=flow::judge_reply(pool,&schema.protocol.protocol_id,reply.raw_hex.as_deref().unwrap_or_default()).await?;
                    if !step.result_field.is_empty() {
                        let raw=schema.values(&reply)?;
                        ok=raw.get(&step.result_field)==Some(&compact_hex(&step.success_hex));
                        note=if ok {"结果符合配置".into()} else {format!("{}={}，期望 {}",step.result_field,raw.get(&step.result_field).map(String::as_str).unwrap_or("缺失"),step.success_hex)};
                    }
                    if ok { content_route=flow::match_route(pool,&schema.protocol.protocol_id,&step.routes,&values).await?; }
                    else { outcome="failure";all_ok=false; }
                    emit(app,engine,run,Some(step),attempt,terminal,if ok {"success"} else {"failure"},&note,Some(reply));
                }
            }
        }
        let (next,branch)=if outcome=="success" {
            if let Some(to)=content_route {(Some(to),"route")} else if step.on_success.is_some(){(step.on_success.clone(),"on_success")}else{(step.next.clone(),"next")}
        } else if outcome=="failure" && step.on_failure.is_some() {(step.on_failure.clone(),"on_failure")}
        else if outcome=="timeout" && step.on_timeout.is_some() {(step.on_timeout.clone(),"on_timeout")}
        else {(step.next.clone(),"next")};
        if let Some(ev)=record(engine,run,Some(step),attempt,None,"transition",outcome,None,next.clone(),Some(branch.into())) { let _=app.emit(flow::EVENT_FLOW_RUN,ev); }
        current=next;
    }
    Ok(all_ok)
}
