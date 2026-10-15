//! Event-driven graph execution. No network operation is performed inside a
//! state lock. Actions contain resolved template fields; the host runs the codec
//! and UDP send, then acknowledges the action. Waits exist before actions escape.
use super::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
fn default_duration() -> u64 {
    60_000
}
fn default_steps() -> u64 {
    100_000
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowDefinition {
    pub id: String,
    #[serde(default)]
    pub name_zh: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub protocol: Value,
    #[serde(default)]
    pub default_variables: Value,
    #[serde(default)]
    pub config: FlowConfig,
    pub nodes: Vec<FlowNode>,
    pub edges: Vec<FlowEdge>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FlowConfig {
    pub max_duration_ms: u64,
    pub max_steps: u64,
    pub max_concurrent_instances: usize,
}
impl Default for FlowConfig {
    fn default() -> Self {
        Self {
            max_duration_ms: default_duration(),
            max_steps: default_steps(),
            max_concurrent_instances: 200,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowNode {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub data: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowEdge {
    pub source: String,
    pub target: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub source_handle: Option<String>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FlowStatus {
    Running,
    Waiting,
    Paused,
    Completed,
    Failed,
    Cancelled,
    Interrupted,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowTrace {
    pub time_ms: u64,
    pub node_id: String,
    pub event: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlowRun {
    pub id: String,
    pub definition_id: String,
    pub definition_version: String,
    pub status: FlowStatus,
    pub started_ms: u64,
    pub ended_ms: Option<u64>,
    pub variables: Value,
    pub trace: Vec<FlowTrace>,
    pub failure: Option<String>,
    pub steps: u64,
    #[serde(default)]
    pub trace_dropped: u64,
    #[serde(skip)]
    snapshots: BTreeMap<String, FlowDefinition>,
    #[serde(skip)]
    threads: BTreeMap<String, ExecutionThread>,
    #[serde(skip)]
    groups: BTreeMap<String, JoinGroup>,
    #[serde(skip)]
    events: BTreeMap<String, Value>,
}
#[derive(Clone, Debug)]
struct ExecutionThread {
    id: String,
    definition_id: String,
    node_id: String,
    state: ThreadState,
    response: Value,
    step: Value,
    loops: BTreeMap<String, u64>,
    forks: Vec<String>,
    returns: Vec<(String, String)>,
}
#[derive(Clone, Debug)]
enum ThreadState {
    Ready,
    Pending(String),
    Action(String),
    Delay(u64),
    Event(String, u64),
    Join,
}
#[derive(Clone, Debug)]
struct JoinGroup {
    expected: usize,
    arrived: usize,
    node_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendAction {
    pub action_id: String,
    pub run_id: String,
    pub node_id: String,
    pub pending_id: Option<String>,
    pub endpoint_id: String,
    pub protocol_version_id: String,
    pub remote: String,
    pub msg_code: u8,
    pub values: Value,
    pub attempt: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FlowAction {
    Send(SendAction),
    DomainUpdate {
        action_id: String,
        run_id: String,
        node_id: String,
        kind: String,
        data: Value,
    },
    Event {
        run_id: String,
        name: String,
        value: Value,
    },
    Finished {
        run_id: String,
        status: FlowStatus,
        error: Option<String>,
    },
}
#[derive(Clone, Debug)]
struct ActionWait {
    run_id: String,
    thread_id: String,
    pending_id: Option<String>,
}
#[derive(Default)]
pub struct FlowEngine {
    pub pending: PendingRegistry,
    pub runs: BTreeMap<String, FlowRun>,
    pub definitions: BTreeMap<String, FlowDefinition>,
    action_waits: BTreeMap<String, ActionWait>,
    pending_waits: BTreeMap<String, (String, String)>,
    sends: BTreeMap<String, SendAction>,
    next_transaction: u32,
}
impl FlowDefinition {
    pub fn validate(&self) -> Result<(), String> {
        if self.id.is_empty() || self.nodes.is_empty() {
            return Err("流程ID或节点为空".into());
        }
        if self.config.max_duration_ms == 0 || self.config.max_steps == 0 {
            return Err("流程必须设置执行时长和步骤上限".into());
        }
        let ids: BTreeSet<_> = self.nodes.iter().map(|n| n.id.as_str()).collect();
        if ids.len() != self.nodes.len() {
            return Err("流程节点ID重复".into());
        }
        if self.nodes.iter().filter(|n| n.kind == "START").count() != 1 {
            return Err("流程必须有且只有一个START".into());
        }
        if !self.nodes.iter().any(|n| n.kind == "END") {
            return Err("流程缺少END".into());
        }
        for e in &self.edges {
            if !ids.contains(e.source.as_str()) || !ids.contains(e.target.as_str()) {
                return Err("连线引用不存在的节点".into());
            }
        }
        for n in &self.nodes {
            if ![
                "START",
                "END",
                "SEND",
                "WAIT_PACKET",
                "SEND_AND_WAIT",
                "CONDITION",
                "ASSIGN",
                "DELAY",
                "LOOP",
                "PARALLEL",
                "JOIN",
                "INVOKE_SUBFLOW",
                "UPDATE_DEVICE",
                "UPDATE_SESSION",
                "UPDATE_CALL",
                "UPDATE_RESOURCE",
                "RAISE_EVENT",
                "WAIT_EVENT",
                "ERROR",
            ]
            .contains(&n.kind.as_str())
            {
                return Err(format!("不支持节点类型 {}", n.kind));
            }
            if ["SEND_AND_WAIT", "WAIT_PACKET", "WAIT_EVENT"].contains(&n.kind.as_str())
                && self.edge(&n.id, Some("timeout")).is_none()
            {
                return Err(format!("节点 {} 缺少timeout出口", n.id));
            }
            if n.kind == "CONDITION"
                && (self.edge(&n.id, Some("true")).is_none()
                    || self.edge(&n.id, Some("false")).is_none())
            {
                return Err(format!("条件节点 {} 缺少true/false出口", n.id));
            }
            if n.kind == "LOOP"
                && n.data
                    .get("maxIterations")
                    .and_then(Value::as_u64)
                    .is_none_or(|v| v == 0 || v > 100_000)
            {
                return Err("LOOP必须设置1..100000的maxIterations".into());
            }
            if n.kind == "PARALLEL" {
                let join = n
                    .data
                    .get("joinId")
                    .and_then(Value::as_str)
                    .ok_or("PARALLEL缺少joinId")?;
                if !self.nodes.iter().any(|n| n.id == join && n.kind == "JOIN") {
                    return Err("PARALLEL的joinId必须指向JOIN".into());
                }
            }
            let outgoing: Vec<_> = self.edges.iter().filter(|e| e.source == n.id).collect();
            if !["END", "ERROR", "PARALLEL"].contains(&n.kind.as_str()) {
                if outgoing.is_empty() {
                    return Err(format!("节点 {} 缺少后续出口", n.id));
                }
                let mut labels = BTreeSet::new();
                for edge in &outgoing {
                    let label = edge
                        .source_handle
                        .as_deref()
                        .or(edge.label.as_deref())
                        .unwrap_or("");
                    if !labels.insert(label) {
                        return Err(format!("节点 {} 的 {} 出口不唯一", n.id, label));
                    }
                }
            }
            if [
                "START",
                "ASSIGN",
                "DELAY",
                "JOIN",
                "INVOKE_SUBFLOW",
                "RAISE_EVENT",
            ]
            .contains(&n.kind.as_str())
                && outgoing.len() != 1
            {
                return Err(format!("节点 {} 必须有一个后续出口", n.id));
            }
            if n.kind == "LOOP"
                && (self.edge(&n.id, Some("body")).is_none()
                    || self.edge(&n.id, Some("done")).is_none())
            {
                return Err("LOOP缺少body/done出口".into());
            }
            if n.kind == "WAIT_EVENT" && self.edge(&n.id, Some("event")).is_none() {
                return Err("WAIT_EVENT缺少event出口".into());
            }
            if n.kind == "PARALLEL" && outgoing.len() < 2 {
                return Err("PARALLEL至少需要两个分支".into());
            }
            if ["END", "ERROR"].contains(&n.kind.as_str()) && !outgoing.is_empty() {
                return Err("终止节点不得包含后续出口".into());
            }
            if n.kind == "CONDITION" {
                parse_expression(
                    n.data
                        .get("expression")
                        .and_then(Value::as_str)
                        .ok_or("CONDITION缺少expression")?,
                    0,
                )?;
            }
        }
        Ok(())
    }
    fn edge(&self, node: &str, branch: Option<&str>) -> Option<String> {
        let edges: Vec<_> = self.edges.iter().filter(|e| e.source == node).collect();
        if let Some(branch) = branch {
            edges
                .iter()
                .find(|e| {
                    e.source_handle.as_deref() == Some(branch) || e.label.as_deref() == Some(branch)
                })
                .map(|e| e.target.clone())
        } else {
            edges
                .iter()
                .find(|e| e.source_handle.is_none() && e.label.is_none())
                .or_else(|| {
                    if edges.len() == 1 {
                        edges.first()
                    } else {
                        None
                    }
                })
                .map(|e| e.target.clone())
        }
    }
}
impl FlowEngine {
    pub fn publish(&mut self, definition: FlowDefinition) -> Result<(), String> {
        definition.validate()?;
        self.definitions.insert(definition.id.clone(), definition);
        Ok(())
    }
    pub fn start(
        &mut self,
        definition_id: &str,
        variables: Value,
        now_ms: u64,
    ) -> Result<String, String> {
        let definition = self.definitions.get(definition_id).ok_or("流程未发布")?;
        definition.validate()?;
        let active = self
            .runs
            .values()
            .filter(|r| {
                r.definition_id == definition_id
                    && matches!(
                        r.status,
                        FlowStatus::Running | FlowStatus::Waiting | FlowStatus::Paused
                    )
            })
            .count();
        if active >= definition.config.max_concurrent_instances {
            return Err("流程并发实例达到上限".into());
        }
        let mut snapshots = BTreeMap::new();
        let mut queue = vec![definition_id.to_owned()];
        while let Some(next) = queue.pop() {
            if snapshots.contains_key(&next) {
                continue;
            }
            let def = self
                .definitions
                .get(&next)
                .ok_or_else(|| format!("子流程 {next} 未发布"))?;
            for node in &def.nodes {
                if node.kind == "INVOKE_SUBFLOW" {
                    queue.push(
                        node.data
                            .get("flowId")
                            .and_then(Value::as_str)
                            .ok_or("子流程缺少flowId")?
                            .to_owned(),
                    );
                }
            }
            snapshots.insert(next, def.clone());
        }
        let run_id = id();
        let thread_id = id();
        let node_id = definition
            .nodes
            .iter()
            .find(|n| n.kind == "START")
            .unwrap()
            .id
            .clone();
        let thread = ExecutionThread {
            id: thread_id.clone(),
            definition_id: definition_id.into(),
            node_id,
            state: ThreadState::Ready,
            response: Value::Null,
            step: json!({}),
            loops: BTreeMap::new(),
            forks: vec![],
            returns: vec![],
        };
        self.runs.insert(
            run_id.clone(),
            FlowRun {
                id: run_id.clone(),
                definition_id: definition_id.into(),
                definition_version: definition.version.clone(),
                status: FlowStatus::Running,
                started_ms: now_ms,
                ended_ms: None,
                variables,
                trace: vec![],
                failure: None,
                steps: 0,
                trace_dropped: 0,
                snapshots,
                threads: BTreeMap::from([(thread_id, thread)]),
                groups: BTreeMap::new(),
                events: BTreeMap::new(),
            },
        );
        Ok(run_id)
    }
    pub fn poll(&mut self, now_ms: u64) -> Vec<FlowAction> {
        let mut actions = vec![];
        for event in self.pending.tick(now_ms) {
            match event {
                PendingEvent::Retry(p) => {
                    if let Some(original) = self.sends.get(&p).cloned() {
                        let mut send = original;
                        send.action_id = id();
                        send.attempt = self.pending.requests[&p].attempts;
                        if let Some((run, thread)) = self.pending_waits.get(&p) {
                            self.action_waits.insert(
                                send.action_id.clone(),
                                ActionWait {
                                    run_id: run.clone(),
                                    thread_id: thread.clone(),
                                    pending_id: Some(p.clone()),
                                },
                            );
                            actions.push(FlowAction::Send(send));
                        }
                    }
                }
                PendingEvent::TimedOut(p) => self.resume_pending(&p, "timeout", None, now_ms),
            }
        }
        let ids: Vec<_> = self.runs.keys().cloned().collect();
        for id in ids {
            actions.extend(self.drive(&id, now_ms, false));
        }
        actions.retain(|action| match action {
            FlowAction::Send(s) => self.is_active(&s.run_id),
            FlowAction::DomainUpdate { run_id, .. } => self.is_active(run_id),
            _ => true,
        });
        actions
    }
    pub fn on_packet(
        &mut self,
        packet: &PacketMeta,
        now_ms: u64,
    ) -> (MatchOutcome, Vec<FlowAction>) {
        let outcome = self.pending.on_packet(packet, now_ms);
        let mut actions = vec![];
        if let MatchOutcome::Matched(p) = &outcome {
            let run = self.pending_waits.get(p).map(|(r, _)| r.clone());
            let mut response = packet.payload.clone();
            if !response.is_object() {
                response = json!({"body":response});
            }
            if response.get("header").is_none() {
                response["header"] = json!({"sessionId":packet.session_id,"transactionId":packet.transaction_id,"msgCode":packet.msg_code,"srcAddr":packet.src_addr,"dstAddr":packet.dst_addr});
            }
            if let Some(result) = response
                .get("body")
                .and_then(|b| b.get("resultCode"))
                .cloned()
            {
                response["resultCode"] = result;
            }
            self.resume_pending(p, "matched", Some(response), now_ms);
            if let Some(run) = run {
                actions.extend(self.drive(&run, now_ms, false));
            }
        }
        (outcome, actions)
    }
    pub fn complete_action(
        &mut self,
        action_id: &str,
        result: Result<Value, String>,
        now_ms: u64,
    ) -> Vec<FlowAction> {
        let Some(wait) = self.action_waits.remove(action_id) else {
            return vec![];
        };
        if let Some(p) = wait.pending_id {
            if let Err(error) = result
                && self.pending.send_failed(&p, now_ms)
            {
                self.resume_pending(&p, "error", Some(json!({"error":error})), now_ms);
            }
        } else if let Some(mut run) = self.runs.remove(&wait.run_id) {
            if let Some(mut thread) = run.threads.remove(&wait.thread_id) {
                if matches!(&thread.state,ThreadState::Action(id) if id==action_id) {
                    match result {
                        Ok(value) => {
                            thread.step = value;
                            self.advance(&mut run, &mut thread, None, now_ms);
                        }
                        Err(error) => {
                            thread.step = json!({"error":error});
                            self.advance(&mut run, &mut thread, Some("error"), now_ms);
                        }
                    }
                }
                run.threads.insert(thread.id.clone(), thread);
            }
            self.runs.insert(run.id.clone(), run);
        }
        self.drive(&wait.run_id, now_ms, false)
    }
    pub fn emit_event(
        &mut self,
        run_id: &str,
        name: &str,
        value: Value,
        now_ms: u64,
    ) -> Vec<FlowAction> {
        if let Some(run) = self.runs.get_mut(run_id) {
            run.events.insert(name.into(), value);
        }
        self.drive(run_id, now_ms, false)
    }
    pub fn pause(&mut self, run_id: &str) -> Result<(), String> {
        let r = self.runs.get_mut(run_id).ok_or("流程实例不存在")?;
        if matches!(r.status, FlowStatus::Running | FlowStatus::Waiting) {
            r.status = FlowStatus::Paused;
            Ok(())
        } else {
            Err("流程不能暂停".into())
        }
    }
    pub fn resume(&mut self, run_id: &str, now_ms: u64) -> Vec<FlowAction> {
        if let Some(r) = self.runs.get_mut(run_id)
            && r.status == FlowStatus::Paused
        {
            r.status = FlowStatus::Running;
        }
        self.drive(run_id, now_ms, false)
    }
    pub fn single_step(&mut self, run_id: &str, now_ms: u64) -> Vec<FlowAction> {
        self.drive(run_id, now_ms, true)
    }
    /// The host can reject a decoded business result even after packet matching.
    /// A just-completed run can therefore become FAILED before its queued actions
    /// are executed; the host must discard the earlier on_packet action batch.
    pub fn fail_run(&mut self, run_id: &str, error: String, now_ms: u64) -> Vec<FlowAction> {
        let Some(run) = self.runs.get_mut(run_id) else {
            return vec![];
        };
        if matches!(
            run.status,
            FlowStatus::Cancelled | FlowStatus::Failed | FlowStatus::Interrupted
        ) {
            return vec![];
        }
        run.status = FlowStatus::Failed;
        run.failure = Some(error.clone());
        run.ended_ms = Some(now_ms);
        run.threads.clear();
        self.pending.cancel_run(run_id, now_ms);
        self.action_waits.retain(|_, w| w.run_id != run_id);
        self.pending_waits.retain(|_, (r, _)| r != run_id);
        self.sends.retain(|_, s| s.run_id != run_id);
        vec![FlowAction::Finished {
            run_id: run_id.into(),
            status: FlowStatus::Failed,
            error: Some(error),
        }]
    }
    fn is_active(&self, run_id: &str) -> bool {
        self.runs.get(run_id).is_some_and(|r| {
            matches!(
                r.status,
                FlowStatus::Running | FlowStatus::Waiting | FlowStatus::Paused
            )
        })
    }
    pub fn cancel(&mut self, run_id: &str, now_ms: u64) -> Vec<FlowAction> {
        let Some(run) = self.runs.get_mut(run_id) else {
            return vec![];
        };
        if !matches!(
            run.status,
            FlowStatus::Running | FlowStatus::Waiting | FlowStatus::Paused
        ) {
            return vec![];
        }
        run.status = FlowStatus::Cancelled;
        run.ended_ms = Some(now_ms);
        run.threads.clear();
        self.pending.cancel_run(run_id, now_ms);
        self.action_waits.retain(|_, w| w.run_id != run_id);
        self.pending_waits.retain(|_, (r, _)| r != run_id);
        self.sends.retain(|_, s| s.run_id != run_id);
        vec![FlowAction::Finished {
            run_id: run_id.into(),
            status: FlowStatus::Cancelled,
            error: None,
        }]
    }
    fn resume_pending(
        &mut self,
        pending_id: &str,
        branch: &str,
        response: Option<Value>,
        now_ms: u64,
    ) {
        let Some((run_id, thread_id)) = self.pending_waits.remove(pending_id) else {
            return;
        };
        self.sends.remove(pending_id);
        if let Some(mut run) = self.runs.remove(&run_id) {
            if let Some(mut thread) = run.threads.remove(&thread_id) {
                if let Some(value) = response {
                    thread.response = value;
                }
                if matches!(&thread.state,ThreadState::Pending(p) if p==pending_id) {
                    self.advance(&mut run, &mut thread, Some(branch), now_ms);
                }
                run.threads.insert(thread_id, thread);
            }
            self.runs.insert(run_id, run);
        }
    }
    fn advance(
        &self,
        run: &mut FlowRun,
        thread: &mut ExecutionThread,
        branch: Option<&str>,
        now_ms: u64,
    ) {
        let definition = &run.snapshots[&thread.definition_id];
        let next = definition.edge(&thread.node_id, branch).or_else(|| {
            if branch == Some("matched") {
                definition.edge(&thread.node_id, None)
            } else {
                None
            }
        });
        if let Some(next) = next {
            thread.node_id = next;
            thread.state = ThreadState::Ready;
        } else {
            run.failure = Some(format!(
                "节点 {} 缺少 {} 出口",
                thread.node_id,
                branch.unwrap_or("next")
            ));
            run.status = FlowStatus::Failed;
            run.ended_ms = Some(now_ms);
        }
    }
    fn drive(&mut self, run_id: &str, now_ms: u64, single: bool) -> Vec<FlowAction> {
        let Some(mut run) = self.runs.remove(run_id) else {
            return vec![];
        };
        let mut actions = vec![];
        if !matches!(
            run.status,
            FlowStatus::Running | FlowStatus::Waiting | FlowStatus::Paused
        ) || (run.status == FlowStatus::Paused && !single)
        {
            self.runs.insert(run_id.into(), run);
            return actions;
        }
        let was_paused = run.status == FlowStatus::Paused;
        run.status = FlowStatus::Running;
        let config = run.snapshots[&run.definition_id].config.clone();
        if now_ms.saturating_sub(run.started_ms) >= config.max_duration_ms {
            run.failure = Some("流程总时长超限".into());
            run.status = FlowStatus::Failed;
        }
        let budget = if single { 1 } else { 256 };
        let mut executed = 0;
        while executed < budget && run.status == FlowStatus::Running {
            // Wake timers/events atomically with checking retained event state.
            let ids: Vec<_> = run.threads.keys().cloned().collect();
            for tid in ids {
                let mut t = run.threads.remove(&tid).unwrap();
                match t.state.clone() {
                    ThreadState::Delay(deadline) if now_ms >= deadline => {
                        self.advance(&mut run, &mut t, None, now_ms)
                    }
                    ThreadState::Event(name, deadline) => {
                        if let Some(value) = run.events.get(&name).cloned() {
                            t.step = value;
                            self.advance(&mut run, &mut t, Some("event"), now_ms);
                        } else if now_ms >= deadline {
                            self.advance(&mut run, &mut t, Some("timeout"), now_ms);
                        }
                    }
                    _ => {}
                }
                run.threads.insert(tid, t);
            }
            if run.status != FlowStatus::Running {
                break;
            }
            let Some(tid) = run
                .threads
                .values()
                .find(|t| matches!(t.state, ThreadState::Ready))
                .map(|t| t.id.clone())
            else {
                break;
            };
            let mut thread = run.threads.remove(&tid).unwrap();
            run.steps += 1;
            executed += 1;
            if run.steps > config.max_steps {
                run.failure = Some("流程步骤上限，可能存在无界循环".into());
                run.status = FlowStatus::Failed;
                break;
            }
            let node = run.snapshots[&thread.definition_id]
                .nodes
                .iter()
                .find(|n| n.id == thread.node_id)
                .cloned()
                .unwrap();
            if run.trace.len() >= 10_000 {
                run.trace.remove(0);
                run.trace_dropped += 1;
            }
            run.trace.push(FlowTrace {
                time_ms: now_ms,
                node_id: node.id.clone(),
                event: node.kind.clone(),
            });
            match self.execute(&mut run, &mut thread, &node, now_ms, &mut actions) {
                Ok(keep) => {
                    if keep {
                        run.threads.insert(tid, thread);
                    }
                }
                Err(error) => {
                    run.failure = Some(error);
                    run.status = FlowStatus::Failed;
                }
            }
        }
        if run.status == FlowStatus::Running {
            run.status = if run.threads.is_empty() {
                FlowStatus::Completed
            } else if was_paused || single {
                FlowStatus::Paused
            } else if run
                .threads
                .values()
                .any(|t| matches!(t.state, ThreadState::Ready))
            {
                FlowStatus::Running
            } else {
                FlowStatus::Waiting
            };
        }
        if matches!(run.status, FlowStatus::Completed | FlowStatus::Failed) {
            run.ended_ms = Some(now_ms);
            self.pending.cancel_run(&run.id, now_ms);
            run.threads.clear();
            self.action_waits.retain(|_, w| w.run_id != run.id);
            self.pending_waits.retain(|_, (r, _)| r != &run.id);
            self.sends.retain(|_, s| s.run_id != run.id);
            actions.retain(|a| !matches!(a, FlowAction::Send(_) | FlowAction::DomainUpdate { .. }));
            actions.push(FlowAction::Finished {
                run_id: run.id.clone(),
                status: run.status,
                error: run.failure.clone(),
            });
        }
        self.runs.insert(run_id.into(), run);
        actions
    }
    fn execute(
        &mut self,
        run: &mut FlowRun,
        t: &mut ExecutionThread,
        n: &FlowNode,
        now: u64,
        actions: &mut Vec<FlowAction>,
    ) -> Result<bool, String> {
        let mut env = run.variables.clone();
        if !env.is_object() {
            env = json!({});
        }
        env["response"] = t.response.clone();
        env["step"] = t.step.clone();
        match n.kind.as_str() {
            "START" => self.advance(run, t, None, now),
            "END" => {
                if n.data.get("result").and_then(Value::as_str) == Some("FAILED") {
                    return Err(format!("子流程或流程 {} 进入失败结束节点", t.definition_id));
                }
                if let Some((def, next)) = t.returns.pop() {
                    t.definition_id = def;
                    t.node_id = next;
                    t.state = ThreadState::Ready;
                } else if !t.forks.is_empty() {
                    return Err("并行分支必须经过JOIN再结束".into());
                } else {
                    if n.data.get("result").and_then(Value::as_str) == Some("FAILED") {
                        run.failure = Some("流程失败分支".into());
                        run.status = FlowStatus::Failed;
                    }
                    return Ok(false);
                }
            }
            "CONDITION" => {
                let expression = n
                    .data
                    .get("expression")
                    .and_then(Value::as_str)
                    .ok_or("条件缺少expression")?;
                let yes = eval_condition(expression, &env)?;
                self.advance(run, t, Some(if yes { "true" } else { "false" }), now);
            }
            "ASSIGN" => {
                if let Some(sets) = n.data.get("set").and_then(Value::as_array) {
                    for assignment in sets {
                        let path = assignment
                            .get("path")
                            .and_then(Value::as_str)
                            .ok_or("赋值缺少path")?;
                        let value = if let Some(from) =
                            assignment.get("from").and_then(Value::as_str)
                        {
                            resolve_path(&env, from)
                                .cloned()
                                .ok_or_else(|| format!("变量不存在: {from}"))?
                        } else {
                            resolve_value(assignment.get("value").unwrap_or(&Value::Null), &env)?
                        };
                        set_path(&mut run.variables, path, value)?;
                    }
                } else {
                    let path = n
                        .data
                        .get("path")
                        .and_then(Value::as_str)
                        .ok_or("ASSIGN缺少path/set")?;
                    let value = resolve_value(n.data.get("value").unwrap_or(&Value::Null), &env)?;
                    set_path(&mut run.variables, path, value)?;
                }
                self.advance(run, t, None, now);
            }
            "DELAY" => {
                let duration = n
                    .data
                    .get("durationMs")
                    .or_else(|| n.data.get("delayMs"))
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                t.state = ThreadState::Delay(now.saturating_add(duration));
            }
            "WAIT_EVENT" => {
                let name = n
                    .data
                    .get("event")
                    .and_then(Value::as_str)
                    .ok_or("等待事件缺少event")?
                    .to_owned();
                t.state = ThreadState::Event(
                    name,
                    now.saturating_add(
                        n.data
                            .get("timeoutMs")
                            .and_then(Value::as_u64)
                            .unwrap_or(3000),
                    ),
                );
            }
            "RAISE_EVENT" => {
                let name = n
                    .data
                    .get("event")
                    .and_then(Value::as_str)
                    .ok_or("事件缺少event")?
                    .to_owned();
                let value = resolve_value(n.data.get("value").unwrap_or(&Value::Null), &env)?;
                run.events.insert(name.clone(), value.clone());
                actions.push(FlowAction::Event {
                    run_id: run.id.clone(),
                    name,
                    value,
                });
                self.advance(run, t, None, now);
            }
            "LOOP" => {
                let max = n.data["maxIterations"]
                    .as_u64()
                    .ok_or("循环缺少maxIterations")?;
                let count = *t.loops.get(&n.id).unwrap_or(&0);
                let items = if let Some(items) = n.data.get("items") {
                    Some(resolve_value(items, &env)?)
                } else {
                    None
                };
                let desired = items
                    .as_ref()
                    .and_then(Value::as_array)
                    .map(|v| v.len() as u64)
                    .or_else(|| n.data.get("count").and_then(Value::as_u64))
                    .unwrap_or(max);
                let condition = if let Some(expr) = n.data.get("expression").and_then(Value::as_str)
                {
                    eval_condition(expr, &env)?
                } else {
                    count < desired
                };
                if !condition || count >= desired {
                    t.loops.remove(&n.id);
                    self.advance(run, t, Some("done"), now);
                } else {
                    if count >= max {
                        return Err("循环迭代次数超过上限".into());
                    }
                    t.loops.insert(n.id.clone(), count + 1);
                    t.step = json!({"index":count});
                    if let Some(item) = items
                        .and_then(|v| v.as_array().and_then(|a| a.get(count as usize)).cloned())
                    {
                        t.step["item"] = item;
                    }
                    self.advance(run, t, Some("body"), now);
                }
            }
            "PARALLEL" => {
                let edges: Vec<_> = run.snapshots[&t.definition_id]
                    .edges
                    .iter()
                    .filter(|e| e.source == n.id)
                    .cloned()
                    .collect();
                if edges.len() < 2 {
                    return Err("并行节点至少需要两个分支".into());
                }
                let group = id();
                let join = n.data["joinId"]
                    .as_str()
                    .ok_or("并行节点缺少joinId")?
                    .to_owned();
                run.groups.insert(
                    group.clone(),
                    JoinGroup {
                        expected: edges.len(),
                        arrived: 0,
                        node_id: join,
                    },
                );
                for edge in edges {
                    let mut branch = t.clone();
                    branch.id = id();
                    branch.node_id = edge.target;
                    branch.forks.push(group.clone());
                    branch.state = ThreadState::Ready;
                    run.threads.insert(branch.id.clone(), branch);
                }
                return Ok(false);
            }
            "JOIN" => {
                let group = t.forks.last().cloned().ok_or("JOIN没有对应的并行分组")?;
                let g = run.groups.get_mut(&group).ok_or("并行分组已关闭")?;
                if g.node_id != n.id {
                    return Err("分支到达了错误JOIN".into());
                }
                g.arrived += 1;
                if g.arrived == g.expected {
                    run.groups.remove(&group);
                    t.forks.pop();
                    self.advance(run, t, None, now);
                } else {
                    t.state = ThreadState::Join;
                    return Ok(false);
                }
            }
            "INVOKE_SUBFLOW" => {
                if t.returns.len() >= 32 {
                    return Err("子流程深度超过32".into());
                }
                let sub = n.data["flowId"].as_str().ok_or("子流程缺少flowId")?;
                let def = run.snapshots.get(sub).ok_or("子流程不在运行快照中")?;
                let start = def
                    .nodes
                    .iter()
                    .find(|n| n.kind == "START")
                    .ok_or("子流程缺少START")?
                    .id
                    .clone();
                let next = run.snapshots[&t.definition_id]
                    .edge(&n.id, None)
                    .ok_or("子流程节点缺少返回连线")?;
                t.returns.push((t.definition_id.clone(), next));
                t.definition_id = sub.into();
                t.node_id = start;
            }
            "SEND" | "SEND_AND_WAIT" | "WAIT_PACKET" => {
                self.prepare_send(run, t, n, &env, now, actions)?
            }
            "UPDATE_DEVICE" | "UPDATE_SESSION" | "UPDATE_CALL" | "UPDATE_RESOURCE" => {
                let action_id = id();
                let data = resolve_value(&n.data, &env)?;
                self.action_waits.insert(
                    action_id.clone(),
                    ActionWait {
                        run_id: run.id.clone(),
                        thread_id: t.id.clone(),
                        pending_id: None,
                    },
                );
                t.state = ThreadState::Action(action_id.clone());
                actions.push(FlowAction::DomainUpdate {
                    action_id,
                    run_id: run.id.clone(),
                    node_id: n.id.clone(),
                    kind: n.kind.clone(),
                    data,
                });
            }
            "ERROR" => {
                return Err(n
                    .data
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("流程错误节点")
                    .into());
            }
            _ => return Err(format!("未知节点 {}", n.kind)),
        }
        Ok(true)
    }
    fn prepare_send(
        &mut self,
        run: &mut FlowRun,
        t: &mut ExecutionThread,
        n: &FlowNode,
        env: &Value,
        now: u64,
        actions: &mut Vec<FlowAction>,
    ) -> Result<(), String> {
        let data = resolve_value(&n.data, env)?;
        let endpoint = get_string(&data, "endpointId")
            .or_else(|| get_string(env, "endpointId"))
            .ok_or("发送节点缺少endpointId")?;
        let protocol = get_string(&data, "protocolVersionId")
            .or_else(|| get_string(&data, "protocolId"))
            .or_else(|| get_string(env, "protocolVersionId"))
            .ok_or("发送节点缺少固定protocolVersionId")?;
        let remote = get_string(&data, "remote")
            .or_else(|| get_string(env, "remote"))
            .ok_or("发送节点缺少remote")?;
        let code = parse_code(
            data.get("code")
                .or_else(|| data.get("requestMsgCode"))
                .unwrap_or(&json!(0)),
        )?;
        let mut values = data.get("values").cloned().unwrap_or_else(|| json!({}));
        if !values.is_object() {
            return Err("发送values必须是对象".into());
        }
        for key in ["srcAddr", "dstAddr", "sessionId", "transactionId"] {
            if values.get(key).is_none()
                && let Some(v) = values.get("header").and_then(|h| h.get(key)).cloned()
            {
                values[key] = v;
            }
        }
        if values.get("transactionId").is_none() {
            self.next_transaction = self.next_transaction.wrapping_add(1).max(1);
            values["transactionId"] = json!(self.next_transaction);
        }
        for key in ["srcAddr", "dstAddr", "sessionId"] {
            if values.get(key).is_none()
                && let Some(v) = env.get(key)
            {
                values[key] = v.clone();
            }
        }
        let action_id = id();
        let mut pending_id = None;
        if n.kind != "SEND" {
            let codes = data
                .get("expectedCodes")
                .or_else(|| data.get("expectCodes"))
                .and_then(Value::as_array)
                .ok_or("等待节点缺少expectedCodes")?
                .iter()
                .map(parse_code)
                .collect::<Result<Vec<_>, _>>()?;
            let mut request = if let Some(rule) = data.get("match") {
                serde_json::from_value::<PendingRequest>(rule.clone())
                    .map_err(|e| format!("匹配规则错误: {e}"))?
            } else {
                PendingRequest::default()
            };
            request.run_id = run.id.clone();
            request.flow_run_id = run.id.clone();
            request.wait_node_id = n.id.clone();
            request.endpoint_id = endpoint.clone();
            request.protocol_version_id = protocol.clone();
            request.expected_remote = remote.clone();
            request.request_msg_code = code;
            request.incoming_request = n.kind == "WAIT_PACKET";
            request.expected_msg_codes = codes;
            if request.expect_src_addr.is_empty() {
                request.expect_src_addr =
                    parse_address(values.get("dstAddr").ok_or("缺少dstAddr")?)?;
            }
            if request.expect_dst_addr.is_empty() {
                request.expect_dst_addr =
                    parse_address(values.get("srcAddr").ok_or("缺少srcAddr")?)?;
            }
            if data.get("noTransactionId").and_then(Value::as_bool) != Some(true) {
                request.transaction_id = values
                    .get("transactionId")
                    .and_then(Value::as_u64)
                    .map(|v| v as u32);
            }
            if data
                .get("match")
                .and_then(|m| m.get("sessionMatcher"))
                .is_none()
            {
                request.session_matcher = if code == 1 {
                    SessionMatcher::Assigned
                } else {
                    values
                        .get("sessionId")
                        .and_then(Value::as_u64)
                        .map(|v| SessionMatcher::Exact(v as u16))
                        .unwrap_or(SessionMatcher::Any)
                };
            }
            request.timeout_ms = data
                .get("timeoutMs")
                .and_then(Value::as_u64)
                .unwrap_or(3000);
            request.retries_remaining = data
                .get("retries")
                .or_else(|| data.get("maxRetries"))
                .and_then(Value::as_u64)
                .unwrap_or(0) as u32;
            request.retry_interval_ms = data
                .get("retryIntervalMs")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let pending = self.pending.register(request, now)?;
            self.pending_waits
                .insert(pending.clone(), (run.id.clone(), t.id.clone()));
            t.state = ThreadState::Pending(pending.clone());
            pending_id = Some(pending);
        } else {
            t.state = ThreadState::Action(action_id.clone());
        }
        if n.kind == "WAIT_PACKET" {
            return Ok(());
        }
        let send = SendAction {
            action_id: action_id.clone(),
            run_id: run.id.clone(),
            node_id: n.id.clone(),
            pending_id: pending_id.clone(),
            endpoint_id: endpoint,
            protocol_version_id: protocol,
            remote,
            msg_code: code,
            values,
            attempt: 1,
        };
        if let Some(p) = &pending_id {
            self.sends.insert(p.clone(), send.clone());
        }
        self.action_waits.insert(
            action_id,
            ActionWait {
                run_id: run.id.clone(),
                thread_id: t.id.clone(),
                pending_id,
            },
        );
        actions.push(FlowAction::Send(send));
        Ok(())
    }
}
fn get_string(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_owned)
}
fn parse_code(v: &Value) -> Result<u8, String> {
    let value = if let Some(n) = v.as_u64() {
        n
    } else if let Some(s) = v.as_str() {
        if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
            u64::from_str_radix(hex, 16).map_err(|_| "信令码非法")?
        } else {
            s.parse::<u64>().map_err(|_| "信令码非法")?
        }
    } else {
        return Err("信令码非法".into());
    };
    u8::try_from(value).map_err(|_| "信令码超出一字节".into())
}
fn parse_address(v: &Value) -> Result<Vec<u8>, String> {
    if let Some(a) = v.as_array() {
        a.iter()
            .map(|v| {
                v.as_u64()
                    .and_then(|n| u8::try_from(n).ok())
                    .ok_or("地址字节非法".into())
            })
            .collect()
    } else if let Some(s) = v.as_str() {
        hex::decode(s.replace([' ', ':', '-'], "")).map_err(|_| "地址HEX非法".into())
    } else {
        Err("地址必须是字节数组或HEX".into())
    }
}
fn resolve_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let path = path.trim().trim_start_matches("${").trim_end_matches('}');
    let normalized = path.replace('[', ".").replace(']', "");
    let mut v = value;
    for part in normalized.split('.').filter(|p| !p.is_empty()) {
        v = if let Ok(i) = part.parse::<usize>() {
            v.get(i)?
        } else {
            v.get(part)?
        };
    }
    Some(v)
}
fn resolve_value(value: &Value, env: &Value) -> Result<Value, String> {
    match value {
        Value::String(s) if s.starts_with("${") && s.ends_with('}') => resolve_path(env, s)
            .cloned()
            .ok_or_else(|| format!("变量不存在: {s}")),
        Value::Array(a) => a
            .iter()
            .map(|v| resolve_value(v, env))
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        Value::Object(o) => o
            .iter()
            .map(|(k, v)| Ok((k.clone(), resolve_value(v, env)?)))
            .collect::<Result<serde_json::Map<_, _>, String>>()
            .map(Value::Object),
        _ => Ok(value.clone()),
    }
}
fn set_path(root: &mut Value, path: &str, value: Value) -> Result<(), String> {
    let parts: Vec<_> = path.split('.').collect();
    if parts.is_empty() || parts.iter().any(|s| s.is_empty()) {
        return Err("变量路径为空".into());
    }
    let mut target = root;
    for part in &parts[..parts.len() - 1] {
        if !target.is_object() {
            *target = json!({});
        }
        target = target
            .as_object_mut()
            .unwrap()
            .entry(part.to_string())
            .or_insert(json!({}));
    }
    if !target.is_object() {
        *target = json!({});
    }
    target[parts[parts.len() - 1]] = value;
    Ok(())
}
#[derive(Debug)]
enum Expression {
    Literal(Value),
    Path(String),
    Not(Box<Expression>),
    Binary(&'static str, Box<Expression>, Box<Expression>),
}
/// Splits only outside strings and parentheses. No script engine or eval is used.
fn top_operator(expr: &str, ops: &[&'static str]) -> Result<Option<(usize, &'static str)>, String> {
    let mut depth = 0usize;
    let mut quoted = false;
    let mut escaped = false;
    let mut found = None;
    for (index, ch) in expr.char_indices() {
        if quoted {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = false;
            }
            continue;
        }
        match ch {
            '"' => quoted = true,
            '(' => {
                depth += 1;
                if depth > 64 {
                    return Err("表达式括号深度超过64".into());
                }
            }
            ')' => {
                depth = depth.checked_sub(1).ok_or("表达式括号不匹配")?;
            }
            _ => {}
        }
        if depth == 0 && !quoted && found.is_none() {
            for op in ops {
                if expr[index..].starts_with(op) {
                    found = Some((index, *op));
                    break;
                }
            }
        }
    }
    if quoted || depth != 0 {
        return Err("表达式引号或括号不完整".into());
    }
    Ok(found)
}
fn outer_parentheses(expr: &str) -> bool {
    if !expr.starts_with('(') || !expr.ends_with(')') {
        return false;
    }
    let mut depth = 0;
    let mut quoted = false;
    let mut escaped = false;
    for (index, ch) in expr.char_indices() {
        if quoted {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                quoted = false;
            }
            continue;
        }
        match ch {
            '"' => quoted = true,
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return index == expr.len() - 1;
                }
            }
            _ => {}
        }
    }
    false
}
fn parse_expression(expr: &str, depth: usize) -> Result<Expression, String> {
    let expr = expr.trim();
    if expr.is_empty() || expr.len() > 4096 || depth > 64 {
        return Err("表达式为空或超过长度/深度限制".into());
    }
    top_operator(expr, &[])?;
    if outer_parentheses(expr) {
        return parse_expression(&expr[1..expr.len() - 1], depth + 1);
    }
    for ops in [
        &["||"][..],
        &["&&"][..],
        &["==", "!=", ">=", "<=", ">", "<"][..],
    ] {
        if let Some((index, op)) = top_operator(expr, ops)? {
            return Ok(Expression::Binary(
                op,
                Box::new(parse_expression(&expr[..index], depth + 1)?),
                Box::new(parse_expression(&expr[index + op.len()..], depth + 1)?),
            ));
        }
    }
    if let Some(rest) = expr.strip_prefix('!') {
        return Ok(Expression::Not(Box::new(parse_expression(
            rest,
            depth + 1,
        )?)));
    }
    if let Ok(value) = serde_json::from_str::<Value>(expr)
        && !value.is_array()
        && !value.is_object()
    {
        return Ok(Expression::Literal(value));
    }
    let path = expr
        .strip_prefix("${")
        .and_then(|p| p.strip_suffix('}'))
        .unwrap_or(expr);
    let mut chars = path.chars();
    if !chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        || !chars.all(|c| c.is_ascii_alphanumeric() || "_.[]".contains(c))
    {
        return Err(format!("不支持的表达式或变量路径: {expr}"));
    }
    Ok(Expression::Path(path.to_owned()))
}
fn eval_expression(expr: &Expression, env: &Value) -> Result<Value, String> {
    match expr {
        Expression::Literal(value) => Ok(value.clone()),
        Expression::Path(path) => resolve_path(env, path)
            .cloned()
            .ok_or_else(|| format!("表达式变量不存在: {path}")),
        Expression::Not(expr) => Ok(json!(
            !eval_expression(expr, env)?
                .as_bool()
                .ok_or("逻辑非需要布尔值")?
        )),
        Expression::Binary(op, left, right) => {
            let left = eval_expression(left, env)?;
            if *op == "||" || *op == "&&" {
                let left = left.as_bool().ok_or("逻辑运算左值不是布尔值")?;
                if (*op == "||" && left) || (*op == "&&" && !left) {
                    return Ok(json!(left));
                }
                return Ok(json!(
                    eval_expression(right, env)?
                        .as_bool()
                        .ok_or("逻辑运算右值不是布尔值")?
                ));
            }
            let right = eval_expression(right, env)?;
            let result = match *op {
                "==" => left == right,
                "!=" => left != right,
                _ => {
                    let left = left.as_f64().ok_or("比较左值不是数字")?;
                    let right = right.as_f64().ok_or("比较右值不是数字")?;
                    match *op {
                        ">=" => left >= right,
                        "<=" => left <= right,
                        ">" => left > right,
                        "<" => left < right,
                        _ => return Err("未知运算符".into()),
                    }
                }
            };
            Ok(json!(result))
        }
    }
}
fn eval_condition(expr: &str, env: &Value) -> Result<bool, String> {
    eval_expression(&parse_expression(expr, 0)?, env)?
        .as_bool()
        .ok_or("条件结果必须为布尔值".into())
}
