//! 引擎全局状态：活跃 socket、监听/心跳任务句柄、待入网标记等

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use dashmap::DashMap;
use sqlx::SqlitePool;
use tokio::net::UdpSocket;
use tokio::task::JoinHandle;

use simterm_core::rules::RegexCache;

/// 等待入网应答的挂起状态
pub struct PendingLogin {
    /// 期望命中的应答协议 protocol_id
    pub expect_protocol_id: String,
    pub sent_at: Instant,
}

/// 运行中的心跳任务描述
pub struct HeartbeatTask {
    pub node_id: i64,
    pub protocol_ref: String,
    pub handle: JoinHandle<()>,
}

/// 周期发送任务键（控制台「周期发送」, 不落库）
#[derive(Debug, Clone)]
pub struct PeriodicKey {
    pub node_id: i64,
    pub protocol_id: String,
}

pub struct EngineState {
    /// node_id -> 已绑定 socket（监听与发送共用）
    pub sockets: DashMap<i64, Arc<UdpSocket>>,
    /// node_id -> 接收/规则引擎循环任务
    pub listeners: DashMap<i64, JoinHandle<()>>,
    /// 心跳绑定 id -> 运行中的心跳任务
    pub heartbeats: DashMap<i64, HeartbeatTask>,
    /// "node_id:protocol_id" -> 周期发送任务（协议的周期发送, 含心跳类）
    pub periodic: DashMap<String, (PeriodicKey, JoinHandle<()>)>,
    /// node_id -> 待入网状态
    pub pending_logins: DashMap<i64, PendingLogin>,
    pub pending_configs: DashMap<crate::devices::ConfigKey, crate::devices::PendingConfig>,
    pub used_save_sequences: tokio::sync::Mutex<std::collections::HashSet<String>>,
    pub pending_saves: tokio::sync::Mutex<crate::business::PendingSaves>,
    pub applied_configs: tokio::sync::Mutex<std::collections::HashMap<(i64, i64, String), Instant>>,
    /// node_id -> 自增序号（{seq} 变量）
    seq_counters: DashMap<i64, u64>,
    /// match_rule 正则编译缓存
    pub regex_cache: RegexCache,
    /// 业务流程运行: run_id -> (flow_id, node_id, 任务句柄)
    pub flow_runs: DashMap<i64, (i64, i64, tokio::task::JoinHandle<()>)>,
    pub flow_traces: std::sync::Mutex<std::collections::BTreeMap<i64, super::flow_trace::FlowTrace>>,
    pub flow_start_lock: tokio::sync::Mutex<()>,
    /// 定时流程的最近运行时刻(节流: 间隔未到不重复启动)
    pub flow_schedule_marks: tokio::sync::Mutex<std::collections::HashMap<String, std::time::Instant>>,
    flow_run_seq: std::sync::atomic::AtomicI64,
}

impl EngineState {
    pub fn new() -> Self {
        Self {
            sockets: DashMap::new(),
            listeners: DashMap::new(),
            heartbeats: DashMap::new(),
            periodic: DashMap::new(),
            pending_logins: DashMap::new(),
            pending_configs: DashMap::new(),
            used_save_sequences: tokio::sync::Mutex::new(std::collections::HashSet::new()),
            pending_saves: tokio::sync::Mutex::new(std::collections::BTreeMap::new()),
            applied_configs: tokio::sync::Mutex::new(std::collections::HashMap::new()),
            seq_counters: DashMap::new(),
            regex_cache: RegexCache::new(),
            flow_runs: DashMap::new(),
            flow_traces: std::sync::Mutex::new(std::collections::BTreeMap::new()),
            flow_start_lock: tokio::sync::Mutex::new(()),
            flow_schedule_marks: tokio::sync::Mutex::new(std::collections::HashMap::new()),
            flow_run_seq: std::sync::atomic::AtomicI64::new(0),
        }
    }

    /// 业务流程运行号, 从 1 开始
    pub fn next_flow_run_id(&self) -> i64 {
        self.flow_run_seq.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1
    }

    /// 节点内自增序号, 从 1 开始
    pub fn seq_next(&self, node_id: i64) -> u64 {
        let mut entry = self.seq_counters.entry(node_id).or_insert(0);
        *entry += 1;
        *entry
    }

    pub fn emit_status_event(&self) -> simterm_core::models::EngineStatus {
        let mut listening_nodes: Vec<i64> = self.sockets.iter().map(|r| *r.key()).collect();
        listening_nodes.sort_unstable();
        let mut running_heartbeats: Vec<i64> =
            self.heartbeats.iter().map(|r| *r.key()).collect();
        running_heartbeats.sort_unstable();
        let mut periodic_sends: Vec<simterm_core::models::PeriodicSendKey> = self
            .periodic
            .iter()
            .map(|r| simterm_core::models::PeriodicSendKey {
                node_id: r.value().0.node_id,
                protocol_id: r.value().0.protocol_id.clone(),
            })
            .collect();
        periodic_sends.sort_by(|a, b| (a.node_id, &a.protocol_id).cmp(&(b.node_id, &b.protocol_id)));
        simterm_core::models::EngineStatus {
            listening_nodes,
            running_heartbeats,
            periodic_sends,
        }
    }
}

impl Default for EngineState {
    fn default() -> Self {
        Self::new()
    }
}

/// 由 Tauri manage 的全局应用状态
pub struct AppState {
    pub pool: SqlitePool,
    pub engine: Arc<EngineState>,
}

/// 解析 ip:port（支持 IP 直填与域名解析）
pub async fn resolve_addr(ip: &str, port: u16) -> simterm_core::error::AppResult<SocketAddr> {
    let text = format!("{ip}:{port}");
    if let Ok(sa) = text.parse::<SocketAddr>() {
        return Ok(sa);
    }
    let addrs: Vec<SocketAddr> = tokio::net::lookup_host(text.clone())
        .await
        .map_err(|e| simterm_core::error::AppError::msg(format!("地址解析失败 {text}: {e}")))?
        .collect();
    addrs
        .into_iter()
        .next()
        .ok_or_else(|| simterm_core::error::AppError::msg(format!("地址解析无结果: {text}")))
}
