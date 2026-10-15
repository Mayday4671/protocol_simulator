//! One Tokio socket per endpoint. Bounded event queue; overflow is counted.
use serde::Serialize;
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::{
    net::UdpSocket,
    sync::{Mutex, mpsc},
    task::JoinHandle,
};
use tokio_util::sync::CancellationToken;

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Direction {
    Tx,
    Rx,
}

#[derive(Clone, Debug)]
pub struct Datagram {
    pub timestamp: u64,
    pub endpoint_id: String,
    pub protocol_id: String,
    pub project_id: String,
    pub generation: String,
    pub cancel: CancellationToken,
    pub remote: String,
    pub direction: Direction,
    pub bytes: Vec<u8>,
    pub context: Option<PacketContext>,
}

/// Local provenance supplied by an executing flow, never inferred from wire data.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PacketContext {
    pub run_id: String,
    pub node_id: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointInfo {
    pub id: String,
    pub name: String,
    pub bind: String,
    pub protocol_id: String,
    pub project_id: String,
    pub generation: String,
    pub running: bool,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Metrics {
    pub tx: u64,
    pub rx: u64,
    pub queue_dropped: u64,
    pub socket_errors: u64,
}
#[derive(Default)]
struct Counters {
    tx: AtomicU64,
    rx: AtomicU64,
    dropped: AtomicU64,
    errors: AtomicU64,
}
struct Endpoint {
    info: EndpointInfo,
    socket: Arc<UdpSocket>,
    cancel: CancellationToken,
    task: JoinHandle<()>,
}

#[derive(Clone)]
pub struct UdpHub {
    endpoints: Arc<Mutex<HashMap<String, Endpoint>>>,
    events: mpsc::Sender<Datagram>,
    counters: Arc<Counters>,
}
impl UdpHub {
    pub fn new(capacity: usize) -> (Self, mpsc::Receiver<Datagram>) {
        let (events, rx) = mpsc::channel(capacity.clamp(1, 65536));
        (
            Self {
                endpoints: Arc::default(),
                events,
                counters: Arc::default(),
            },
            rx,
        )
    }
    pub async fn start(
        &self,
        id: String,
        name: String,
        bind: &str,
        protocol_id: String,
    ) -> Result<EndpointInfo, String> {
        self.start_scoped(id, name, bind, protocol_id, String::new())
            .await
    }
    pub async fn start_scoped(
        &self,
        id: String,
        name: String,
        bind: &str,
        protocol_id: String,
        project_id: String,
    ) -> Result<EndpointInfo, String> {
        let addr: SocketAddr = bind.parse().map_err(|_| "监听地址必须为 IP:端口")?;
        let mut endpoints = self.endpoints.lock().await;
        if endpoints.contains_key(&id) {
            return Err("端点已启动".into());
        }
        let socket = Arc::new(
            UdpSocket::bind(addr)
                .await
                .map_err(|e| format!("UDP绑定失败: {e}"))?,
        );
        let generation = uuid::Uuid::new_v4().to_string();
        let info = EndpointInfo {
            id: id.clone(),
            name,
            bind: socket.local_addr().map_err(|e| e.to_string())?.to_string(),
            protocol_id: protocol_id.clone(),
            project_id: project_id.clone(),
            generation: generation.clone(),
            running: true,
        };
        let cancel = CancellationToken::new();
        let token = cancel.clone();
        let sock = socket.clone();
        let queue = self.events.clone();
        let counters = self.counters.clone();
        let endpoint_id = id.clone();
        let task = tokio::spawn(async move {
            let mut buffer = vec![0; 65536];
            loop {
                tokio::select! {
                    biased;
                    _=token.cancelled()=>break,
                    received=sock.recv_from(&mut buffer)=>match received {
                        Ok((len,remote))=> {
                            counters.rx.fetch_add(1,Ordering::Relaxed);
                            let event=Datagram{timestamp:now_ms(),endpoint_id:endpoint_id.clone(),protocol_id:protocol_id.clone(),project_id:project_id.clone(),generation:generation.clone(),cancel:token.clone(),remote:remote.to_string(),direction:Direction::Rx,bytes:buffer[..len].to_vec(),context:None};
                            if queue.try_send(event).is_err() { counters.dropped.fetch_add(1,Ordering::Relaxed); }
                        }
                        Err(error)=>{
                            counters.errors.fetch_add(1,Ordering::Relaxed);
                            // Windows reports an ICMP unreachable response as a receive error.
                            // It describes the previous peer, not a failed listening socket.
                            if matches!(error.kind(),std::io::ErrorKind::ConnectionReset|std::io::ErrorKind::ConnectionRefused|std::io::ErrorKind::Interrupted){continue;}
                            break;
                        }
                    }
                }
            }
        });
        endpoints.insert(
            id,
            Endpoint {
                info: info.clone(),
                socket,
                cancel,
                task,
            },
        );
        Ok(info)
    }
    pub async fn send(&self, id: &str, remote: &str, bytes: &[u8]) -> Result<usize, String> {
        self.send_generation(id, remote, bytes, None).await
    }
    pub async fn send_generation(
        &self,
        id: &str,
        remote: &str,
        bytes: &[u8],
        expected: Option<&str>,
    ) -> Result<usize, String> {
        self.send_with_context(id, remote, bytes, expected, None)
            .await
    }
    pub async fn send_with_context(
        &self,
        id: &str,
        remote: &str,
        bytes: &[u8],
        expected: Option<&str>,
        context: Option<PacketContext>,
    ) -> Result<usize, String> {
        if bytes.len() > 65507 {
            return Err("UDP数据报超过65507字节".into());
        }
        let target: SocketAddr = remote.parse().map_err(|_| "远端必须为 IP:端口")?;
        let (socket, info, cancel) = {
            let endpoints = self.endpoints.lock().await;
            let e = endpoints.get(id).ok_or("端点未启动")?;
            if expected.is_some_and(|g| g != e.info.generation) {
                return Err("端点代次已过期".into());
            }
            (e.socket.clone(), e.info.clone(), e.cancel.clone())
        };
        let sent=tokio::select!{biased;_=cancel.cancelled()=>return Err("端点已停止".into()),sent=socket.send_to(bytes,target)=>sent}.map_err(|e|{self.counters.errors.fetch_add(1,Ordering::Relaxed);format!("UDP发送失败: {e}")})?;
        self.counters.tx.fetch_add(1, Ordering::Relaxed);
        if self
            .events
            .try_send(Datagram {
                timestamp: now_ms(),
                endpoint_id: id.into(),
                protocol_id: info.protocol_id,
                project_id: info.project_id,
                generation: info.generation,
                cancel,
                remote: target.to_string(),
                direction: Direction::Tx,
                bytes: bytes.to_vec(),
                context,
            })
            .is_err()
        {
            self.counters.dropped.fetch_add(1, Ordering::Relaxed);
        }
        Ok(sent)
    }
    pub async fn stop(&self, id: &str) -> Result<(), String> {
        let e = self.endpoints.lock().await.remove(id).ok_or("端点未启动")?;
        e.cancel.cancel();
        let _ = e.task.await;
        drop(e.socket);
        Ok(())
    }
    pub async fn stop_all(&self) {
        let ids: Vec<_> = self.endpoints.lock().await.keys().cloned().collect();
        for id in ids {
            let _ = self.stop(&id).await;
        }
    }
    pub async fn list(&self) -> Vec<EndpointInfo> {
        self.endpoints
            .lock()
            .await
            .values()
            .map(|e| {
                let mut info = e.info.clone();
                info.running = !e.task.is_finished();
                info
            })
            .collect()
    }
    pub fn metrics(&self) -> Metrics {
        Metrics {
            tx: self.counters.tx.load(Ordering::Relaxed),
            rx: self.counters.rx.load(Ordering::Relaxed),
            queue_dropped: self.counters.dropped.load(Ordering::Relaxed),
            socket_errors: self.counters.errors.load(Ordering::Relaxed),
        }
    }
}
