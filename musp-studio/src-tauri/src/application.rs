//! Application services. Network I/O and decoding never run on the UI thread.
use crate::{
    network::{Datagram, Direction, UdpHub, now_ms},
    persistence::{Database, PacketInsert},
    protocol::{CompiledProtocol, compile_xml},
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    sync::{Arc, Mutex, RwLock},
};
use tokio::sync::mpsc;
use uuid::Uuid;

pub const SEED_XML: &str = include_str!("../../protocols/musp-v08.xml");
pub const DEMO_XML: &str = include_str!("../../protocols/musp-demo.xml");
pub struct Studio {
    pub db: Database,
    pub udp: UdpHub,
    pub data_dir: PathBuf,
    pub project_id: RwLock<String>,
    pub schemas: RwLock<HashMap<String, Arc<CompiledProtocol>>>,
    pub processing_errors: AtomicU64,
    pub domain: Mutex<crate::domain::DomainStore>,
    pub execution: Mutex<crate::flow_host::Execution>,
    pub peers: Mutex<HashMap<String, crate::simulator::PeerState>>,
    pub shutdown: tokio_util::sync::CancellationToken,
    pub heartbeat_timeout_ms: AtomicU64,
    pub last_maintenance_ms: AtomicU64,
    pub processed_packets: AtomicU64,
    pub request_gate: tokio::sync::Mutex<()>,
    pub project_gate: RwLock<()>,
}

impl Studio {
    pub fn open(path: &Path) -> Result<(Arc<Self>, mpsc::Receiver<Datagram>), String> {
        std::fs::create_dir_all(path).map_err(|e| e.to_string())?;
        let db = Database::open(&path.join("db/musp.db"))?;
        let projects = db.projects()?;
        let project_id = if projects.is_empty() {
            db.create_project("我的 MUSP 项目")?.id
        } else {
            projects[0].id.clone()
        };
        if db.entities(&project_id, "systems")?.is_empty() {
            let id = Uuid::new_v4().to_string();
            db.save_entity(
                &project_id,
                "systems",
                &id,
                &json!({"id":id,"projectId":project_id,"name":"本地系统"}),
            )?;
        }
        if db.protocols(&project_id)?.is_empty() {
            db.save_protocol(&project_id, None, "MUSP v0.8 业务样例", SEED_XML, true)?;
        }
        let mut schemas = HashMap::new();
        for p in db.protocols(&project_id)? {
            if p.published {
                schemas.insert(
                    p.id.clone(),
                    Arc::new(compile_xml(&db.protocol_source(&p.id)?).map_err(|e| e.to_string())?),
                );
            }
        }
        let mut domain = db
            .entities(&project_id, "domain")?
            .into_iter()
            .next()
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        recover_domain(&mut domain);
        let (udp, rx) = UdpHub::new(4096);
        let execution = load_execution(&db, &project_id)?;
        let timeout = db
            .entities(&project_id, "settings")?
            .first()
            .and_then(|s| s["heartbeatTimeoutMs"].as_u64())
            .unwrap_or(30000);
        Ok((
            Arc::new(Self {
                db,
                udp,
                data_dir: path.into(),
                project_id: RwLock::new(project_id),
                schemas: RwLock::new(schemas),
                processing_errors: AtomicU64::new(0),
                domain: Mutex::new(domain),
                execution: Mutex::new(execution),
                peers: Mutex::default(),
                shutdown: tokio_util::sync::CancellationToken::new(),
                heartbeat_timeout_ms: AtomicU64::new(timeout),
                last_maintenance_ms: AtomicU64::new(0),
                processed_packets: AtomicU64::new(0),
                request_gate: tokio::sync::Mutex::new(()),
                project_gate: RwLock::new(()),
            }),
            rx,
        ))
    }
    pub fn project(&self) -> String {
        self.project_id.read().unwrap().clone()
    }
    fn switch_project(&self, id: &str) -> Result<(), String> {
        let _gate = self.project_gate.write().unwrap();
        if self
            .execution
            .lock()
            .unwrap()
            .engine
            .runs
            .values()
            .any(|r| {
                matches!(
                    r.status,
                    crate::runtime::FlowStatus::Running
                        | crate::runtime::FlowStatus::Waiting
                        | crate::runtime::FlowStatus::Paused
                )
            })
        {
            return Err("请先停止运行中的流程再切换项目".into());
        }
        let mut schemas = HashMap::new();
        for p in self.db.protocols(id)? {
            if p.published {
                schemas.insert(
                    p.id.clone(),
                    Arc::new(
                        compile_xml(&self.db.protocol_source(&p.id)?).map_err(|e| e.to_string())?,
                    ),
                );
            }
        }
        let mut domain = self
            .db
            .entities(id, "domain")?
            .into_iter()
            .next()
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        recover_domain(&mut domain);
        let execution = load_execution(&self.db, id)?;
        *self.execution.lock().unwrap() = execution;
        self.peers.lock().unwrap().clear();
        self.heartbeat_timeout_ms.store(
            self.db
                .entities(id, "settings")?
                .first()
                .and_then(|s| s["heartbeatTimeoutMs"].as_u64())
                .unwrap_or(30000),
            Ordering::Relaxed,
        );
        *self.schemas.write().unwrap() = schemas;
        *self.domain.lock().unwrap() = domain;
        *self.project_id.write().unwrap() = id.into();
        Ok(())
    }
    pub fn schema(&self, id: &str) -> Result<Arc<CompiledProtocol>, String> {
        self.schemas
            .read()
            .unwrap()
            .get(id)
            .cloned()
            .ok_or_else(|| "请选择已发布协议版本".into())
    }
    pub fn start_receiver(self: &Arc<Self>, mut rx: mpsc::Receiver<Datagram>) {
        let app = self.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                let packet = tokio::select! {_=app.shutdown.cancelled()=>break,packet=rx.recv()=>match packet{Some(packet)=>packet,None=>break}};
                let app = app.clone();
                let result = tokio::task::spawn_blocking(move || app.record_packet(packet)).await;
                if !matches!(result, Ok(Ok(_))) {
                    tracing::error!("报文处理/落库失败: {:?}", result);
                }
            }
        });
    }
    pub fn record_packet(self: &Arc<Self>, packet: Datagram) -> Result<Value, String> {
        let _gate = self.project_gate.read().unwrap();
        let project = if packet.project_id.is_empty() {
            self.project()
        } else {
            packet.project_id.clone()
        };
        let current = project == self.project() && !packet.cancel.is_cancelled();
        let mut context = if packet.direction == Direction::Tx {
            packet.context.clone()
        } else {
            None
        };
        let schema = self.schema(&packet.protocol_id).or_else(|_| {
            self.db
                .protocol_source(&packet.protocol_id)
                .and_then(|xml| compile_xml(&xml).map(Arc::new).map_err(|e| e.to_string()))
        });
        let musp_bound = schema
            .as_ref()
            .is_ok_and(|s| s.id.to_ascii_lowercase().starts_with("musp"));
        let decoded = schema.and_then(|s| s.decode(&packet.bytes).map_err(|e| e.to_string()));
        let (status, tree, raw) = match decoded {
            Ok(d) => {
                if current {
                    self.remember_secrets(&d);
                }
                if current
                    && packet.direction == Direction::Rx
                    && crate::flow_host::is_musp_protocol(&d.protocol_id)
                {
                    let reply = {
                        let mut peers = self.peers.lock().unwrap();
                        if let Some(peer) = peers.get_mut(&packet.endpoint_id) {
                            crate::simulator::respond(
                                self.schema(&packet.protocol_id)?.as_ref(),
                                &d,
                                peer,
                                &packet.remote,
                            )?
                            .zip(peer.delivery())
                        } else {
                            None
                        }
                    };
                    if let Some((mut bytes, fault)) = reply {
                        let udp = self.udp.clone();
                        let endpoint = packet.endpoint_id.clone();
                        let remote = packet.remote.clone();
                        let cancel = packet.cancel.clone();
                        let generation = packet.generation.clone();
                        tauri::async_runtime::spawn(async move {
                            if fault.corrupt_crc
                                && let Some(last) = bytes.last_mut()
                            {
                                *last ^= 0xFF;
                            }
                            if fault.delay_ms > 0 {
                                tokio::select! {_=cancel.cancelled()=>return,_=tokio::time::sleep(std::time::Duration::from_millis(fault.delay_ms))=>{}}
                            }
                            for _ in 0..if fault.duplicate_replies { 2 } else { 1 } {
                                if let Err(error) = udp
                                    .send_generation(&endpoint, &remote, &bytes, Some(&generation))
                                    .await
                                {
                                    tracing::error!(%error,"虚拟对端发送失败");
                                }
                            }
                        });
                    }
                }
                let routing = if current {
                    self.route_packet(&packet, &d)?
                } else {
                    json!({"ignored":"端点已停止或项目已切换"})
                };
                if packet.direction == Direction::Rx {
                    context = routing["runId"]
                        .as_str()
                        .zip(routing["nodeId"].as_str())
                        .map(|(run_id, node_id)| crate::network::PacketContext {
                            run_id: run_id.into(),
                            node_id: node_id.into(),
                        });
                }
                let mut tree = serde_json::to_value(&d).map_err(|e| e.to_string())?;
                tree["routing"] = routing;
                // Redaction is applied to both raw bytes and every decoded representation before persistence.
                let mut raw: Vec<String> =
                    packet.bytes.iter().map(|b| format!("{b:02X}")).collect();
                redact_fields(&mut tree, &mut raw);
                rewrite_redacted_hex(&mut tree, &raw);
                if let Some(obj) = tree.as_object_mut() {
                    obj.remove("values");
                    obj.remove("rawBytes");
                    obj.insert("rawHex".into(), json!(raw.join(" ")));
                }
                ("已解析", tree, raw.join(" "))
            }
            Err(error) => {
                self.processing_errors.fetch_add(1, Ordering::Relaxed);
                // Unknown binary packets remain inspectable; MUSP auth TLV is conservatively masked even on CRC errors.
                let mut raw: Vec<String> =
                    packet.bytes.iter().map(|b| format!("{b:02X}")).collect();
                if packet.bytes.len() > 23
                    && (musp_bound || packet.bytes.starts_with(&[0xAA, 0x55]))
                {
                    let mut body = 23;
                    // Either header indication is enough: the other can be the damaged byte.
                    // Binding to the published MUSP schema remains useful when magic is corrupt.
                    if (packet.bytes[13] & 1 != 0 || packet.bytes[3] > 23)
                        && packet.bytes.len() > 25
                    {
                        body = 25 + packet.bytes[24] as usize;
                        for b in &mut raw[25..] {
                            *b = "**".into();
                        }
                    }
                    // A damaged message code must not expose an otherwise identifiable
                    // AUTH_ISSUE body (validity u32 + token length u8 + token + CRC16).
                    let resembles_auth_issue = musp_bound
                        && packet.bytes.get(body + 4).is_some_and(|len| {
                            *len > 0 && body + 5 + usize::from(*len) + 2 == packet.bytes.len()
                        });
                    if (packet.bytes[6] == 0x11 || resembles_auth_issue) && body + 5 < raw.len() {
                        for b in &mut raw[body + 5..] {
                            *b = "**".into();
                        }
                    }
                }
                (
                    "解析失败",
                    json!({"fields":[],"valid":false,"diagnostics":[error]}),
                    raw.join(" "),
                )
            }
        };
        let code = tree
            .get("messageCode")
            .or_else(|| tree.get("msgCode"))
            .and_then(Value::as_u64)
            .map(|n| n as u8);
        let direction = if packet.direction == Direction::Rx {
            "RX"
        } else {
            "TX"
        };
        let id = self.db.insert_packet_with_context(
            &PacketInsert {
                project_id: &project,
                timestamp: packet.timestamp,
                direction,
                endpoint_id: &packet.endpoint_id,
                remote: &packet.remote,
                protocol_id: &packet.protocol_id,
                msg_code: code,
                length: packet.bytes.len(),
                hex: &raw,
                status,
                decoded: &tree,
            },
            context.as_ref(),
        )?;
        self.processed_packets.fetch_add(1, Ordering::Relaxed);
        Ok(
            json!({"id":id.to_string(),"timestamp":packet.timestamp,"direction":direction,"endpointId":packet.endpoint_id,"remote":packet.remote,"msgCode":code,"length":packet.bytes.len(),"hex":raw,"status":status,"decoded":tree,"runId":context.as_ref().map(|c|&c.run_id),"nodeId":context.as_ref().map(|c|&c.node_id)}),
        )
    }
    pub async fn request(self: &Arc<Self>, r: Value) -> Result<Value, String> {
        let _request = self.request_gate.lock().await;
        let action = required(&r, "action")?;
        let project = self.project();
        if r["expectedProjectId"]
            .as_str()
            .is_some_and(|expected| expected != project)
        {
            return Err("项目已切换，请刷新当前页面".into());
        }
        match action {
            "snapshot" => {
                let (devices, calls, resources) =
                    crate::business::domain_views(&self.domain.lock().unwrap());
                let (runs, pending) = self.run_views();
                let sessions = json!(
                    self.domain
                        .lock()
                        .unwrap()
                        .sessions
                        .values()
                        .collect::<Vec<_>>()
                );
                let running = self.udp.list().await;
                let endpoints = self
                    .db
                    .entities(&project, "endpoints")?
                    .into_iter()
                    .map(|mut e| {
                        e["running"] = json!(
                            running
                                .iter()
                                .any(|r| Some(r.id.as_str()) == e["id"].as_str() && r.running)
                        );
                        e
                    })
                    .collect::<Vec<_>>();
                Ok(
                    json!({"projects":self.db.projects()?,"projectId":project,"systems":self.db.entities(&project,"systems")?,"devices":devices,"sessions":sessions,"endpoints":endpoints,"protocols":self.db.protocols(&project)?,"flows":self.db.entities(&project,"flows")?,"runs":runs,"pending":pending,"calls":calls,"resources":resources,"pools":self.domain.lock().unwrap().pools.values().collect::<Vec<_>>(),"stats":self.udp.metrics(),"processingErrors":self.processing_errors.load(Ordering::Relaxed),"processedPackets":self.processed_packets.load(Ordering::Relaxed),"dataDir":self.data_dir}),
                )
            }
            "device_history" => {
                let id = required(&r, "id")?;
                Ok(json!(
                    self.db
                        .entities(&project, "device_events")?
                        .into_iter()
                        .filter(|event| event["deviceId"] == id)
                        .collect::<Vec<_>>()
                ))
            }
            "device_upsert" => {
                let mut store = self.domain.lock().unwrap();
                let mut candidate = store.clone();
                if !self
                    .db
                    .entities(&project, "systems")?
                    .iter()
                    .any(|s| s["id"] == r["systemId"])
                {
                    return Err("设备系统不存在".into());
                }
                let device = crate::business::upsert_device(&mut candidate, &r)?;
                self.db
                    .save_entity(&project, "domain", "current", &json!(candidate))?;
                *store = candidate;
                Ok(device)
            }
            "device_delete" => {
                let mut store = self.domain.lock().unwrap();
                crate::business::delete_device(&mut store, required(&r, "id")?)?;
                self.db
                    .save_entity(&project, "domain", "current", &json!(*store))?;
                Ok(json!({"deleted":true}))
            }
            "call_end" => self.end_call(required(&r, "callId")?).await,
            "resource_reconcile" => self.reconcile_resource(&r),
            "settings_get" => {
                Ok(json!({"heartbeatTimeoutMs":self.heartbeat_timeout_ms.load(Ordering::Relaxed)}))
            }
            "settings_save" => {
                let timeout = r["heartbeatTimeoutMs"]
                    .as_u64()
                    .filter(|n| *n == 0 || (*n >= 1000 && *n <= 3600000))
                    .ok_or("心跳超时须为0（关闭）或1000..3600000毫秒")?;
                self.db.save_entity(
                    &project,
                    "settings",
                    "current",
                    &json!({"heartbeatTimeoutMs":timeout}),
                )?;
                self.heartbeat_timeout_ms.store(timeout, Ordering::Relaxed);
                Ok(json!({"saved":true}))
            }
            "pool_create" => {
                let mut domain = self.domain.lock().unwrap();
                let id = r["id"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| Uuid::new_v4().to_string());
                domain.add_pool(
                    &id,
                    required(&r, "name")?,
                    r["capacity"]
                        .as_u64()
                        .filter(|n| *n > 0)
                        .ok_or("容量须为正整数")?,
                    r["unit"].as_str().unwrap_or("kbps"),
                )?;
                self.db
                    .save_entity(&project, "domain", "current", &json!(*domain))?;
                Ok(json!({"id":id}))
            }
            "project_create" => {
                if !self.udp.list().await.is_empty() {
                    return Err("请先停止端点再切换项目".into());
                }
                let p = self.db.create_project(required(&r, "name")?)?;
                self.db
                    .save_protocol(&p.id, None, "MUSP v0.8 业务样例", SEED_XML, true)?;
                let sid = Uuid::new_v4().to_string();
                self.db.save_entity(
                    &p.id,
                    "systems",
                    &sid,
                    &json!({"id":sid,"projectId":p.id,"name":"本地系统"}),
                )?;
                self.switch_project(&p.id)?;
                Ok(json!(p))
            }
            "project_open" => {
                if !self.udp.list().await.is_empty() {
                    return Err("请先停止端点再切换项目".into());
                }
                let id = required(&r, "id")?;
                if !self.db.projects()?.iter().any(|p| p.id == id) {
                    return Err("项目不存在".into());
                }
                self.switch_project(id)?;
                Ok(json!({"id":id}))
            }
            "system_create" => {
                let id = Uuid::new_v4().to_string();
                let item = json!({"id":id,"projectId":project,"name":required(&r,"name")?});
                self.db.save_entity(&project, "systems", &id, &item)?;
                Ok(item)
            }
            "protocol_source" => {
                let xml = if let Some(id) = r["id"].as_str() {
                    self.db.protocol_source(id)?
                } else {
                    DEMO_XML.into()
                };
                Ok(json!({"xml":xml}))
            }
            "protocol_validate" => match compile_xml(required(&r, "xml")?) {
                Ok(ast) => Ok(json!({"valid":true,"diagnostics":[],"ast":ast})),
                Err(e) => Ok(json!({"valid":false,"diagnostics":[e]})),
            },
            "protocol_save" | "protocol_publish" => {
                let xml = required(&r, "xml")?;
                let publish = action == "protocol_publish";
                let compiled = if publish {
                    Some(compile_xml(xml).map_err(|e| e.to_string())?)
                } else {
                    None
                };
                let p = self.db.save_protocol(
                    &project,
                    r["id"].as_str(),
                    r["name"].as_str().unwrap_or("用户协议"),
                    xml,
                    publish,
                )?;
                let dir = self
                    .data_dir
                    .join("projects")
                    .join(&project)
                    .join("protocols");
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                std::fs::write(dir.join(format!("{}.xml", p.id)), xml)
                    .map_err(|e| e.to_string())?;
                if let Some(compiled) = compiled {
                    self.schemas
                        .write()
                        .unwrap()
                        .insert(p.id.clone(), Arc::new(compiled));
                }
                Ok(json!(p))
            }
            "protocol_catalog" => self.protocol_catalog(required(&r, "protocolId")?),
            "flow_preview" => self.preview_flow_node(&r),
            "protocol_encode" | "protocol_decode" => {
                let schema = if let Some(xml) = r["xml"].as_str() {
                    Arc::new(compile_xml(xml).map_err(|e| e.to_string())?)
                } else {
                    self.schema(required(&r, "protocolId")?)?
                };
                let bytes = if action == "protocol_encode" {
                    let code = r["code"]
                        .as_u64()
                        .filter(|n| *n <= 255)
                        .ok_or("信令码须为1字节")?;
                    schema
                        .encode(code as u8, &r["values"])
                        .map_err(|e| e.to_string())?
                } else {
                    parse_hex(required(&r, "hex")?)?
                };
                let decoded = schema.decode(&bytes).map_err(|e| e.to_string())?;
                Ok(json!({"hex":format_hex(&bytes),"bytes":bytes,"decoded":decoded}))
            }
            "endpoint_start" => {
                let id = r["id"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| Uuid::new_v4().to_string());
                let fault: crate::simulator::FaultPolicy =
                    serde_json::from_value(r.get("fault").cloned().unwrap_or(json!({})))
                        .map_err(|e| e.to_string())?;
                if fault.delay_ms > 60000 {
                    return Err("故障延迟不能超过60000毫秒".into());
                }
                let protocol_id = required(&r, "protocolId")?;
                let schema = self.schema(protocol_id)?;
                if r["virtualPeer"].as_bool() == Some(true)
                    && !crate::flow_host::is_musp_protocol(&schema.id)
                {
                    return Err("内置虚拟对端仅支持明确启用的MUSP协议（musp-v08 / musp-demo）；其他协议请连接实际UDP对端".into());
                }
                let ep = self
                    .udp
                    .start_scoped(
                        id,
                        r["name"].as_str().unwrap_or("UDP端点").into(),
                        required(&r, "bind")?,
                        protocol_id.into(),
                        project.clone(),
                    )
                    .await?;
                if r["virtualPeer"].as_bool() == Some(true) {
                    let mut peer = crate::simulator::PeerState::default();
                    peer.fault = fault.clone();
                    self.peers.lock().unwrap().insert(ep.id.clone(), peer);
                }
                let mut saved = json!(ep);
                saved["virtualPeer"] = json!(r["virtualPeer"].as_bool().unwrap_or(false));
                saved["fault"] = json!(fault);
                self.db.save_entity(&project, "endpoints", &ep.id, &saved)?;
                Ok(saved)
            }
            "endpoint_stop" => {
                let id = required(&r, "id")?;
                self.udp.stop(id).await?;
                self.peers.lock().unwrap().remove(id);
                Ok(json!({"stopped":true}))
            }
            "udp_send" => {
                let bytes = parse_hex(required(&r, "hex")?)?;
                let count = self
                    .udp
                    .send(required(&r, "endpointId")?, required(&r, "remote")?, &bytes)
                    .await?;
                Ok(
                    json!({"sent":count,"timestamp":now_ms(),"status":"本地发送成功，尚未确认业务结果"}),
                )
            }
            "packet_query" => self.db.packets_scoped(
                &project,
                r["offset"].as_u64().unwrap_or(0),
                r["limit"].as_u64().unwrap_or(100),
                r["filter"].as_str().unwrap_or(""),
                r["direction"]
                    .as_str()
                    .filter(|d| ["RX", "TX"].contains(d))
                    .unwrap_or(""),
                crate::persistence::PacketScope {
                    run_id: r["runId"].as_str(),
                    endpoint_id: r["endpointId"].as_str(),
                },
            ),
            "export" => self.export(&r),
            "project_import" => self.import_project(required(&r, "path")?),
            "protocol_import" => self.import_protocol(&r),
            "template_list" => Ok(json!(crate::templates::template_list())),
            "template_install" => {
                let protocols = self.db.protocols(&project)?;
                let protocol = r["protocolId"]
                    .as_str()
                    .or_else(|| {
                        protocols
                            .iter()
                            .find(|p| p.published)
                            .map(|p| p.id.as_str())
                    })
                    .ok_or("请先发布协议")?;
                self.schema(protocol)?;
                let flow = crate::templates::template_install(required(&r, "id")?, protocol)?;
                self.db
                    .save_entity(&project, "flows", required(&flow, "id")?, &flow)?;
                Ok(flow)
            }
            "flow_save" => {
                let mut flow = r["flow"].clone();
                let id = flow["id"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| Uuid::new_v4().to_string());
                flow["id"] = json!(id);
                flow["published"] = json!(false);
                self.db.save_entity(&project, "flows", &id, &flow)?;
                Ok(flow)
            }
            "flow_publish" => {
                let id = required(&r, "flowId")?;
                let mut flow = self
                    .db
                    .entities(&project, "flows")?
                    .into_iter()
                    .find(|f| f["id"] == id)
                    .ok_or("流程不存在")?;
                let protocol = flow["protocol"]["id"]
                    .as_str()
                    .ok_or("请选择固定协议发布版本")?;
                self.schema(protocol)?;
                let mut definition: crate::runtime::FlowDefinition =
                    serde_json::from_value(flow.clone()).map_err(|e| e.to_string())?;
                definition.version = Uuid::new_v4().to_string();
                self.execution
                    .lock()
                    .unwrap()
                    .engine
                    .publish(definition.clone())?;
                self.db.save_entity(
                    &project,
                    "flow_versions",
                    &definition.version,
                    &json!(definition),
                )?;
                flow["published"] = json!(true);
                self.db.save_entity(&project, "flows", id, &flow)?;
                Ok(flow)
            }
            "flow_run" => {
                let id = required(&r, "flowId")?;
                if !self
                    .db
                    .entities(&project, "flows")?
                    .iter()
                    .any(|flow| flow["id"] == id && flow["published"] == true)
                {
                    return Err("当前流程存在未发布草稿，请先校验并发布后运行".into());
                }
                let mut variables = self
                    .execution
                    .lock()
                    .unwrap()
                    .engine
                    .definitions
                    .get(id)
                    .ok_or("流程尚未发布")?
                    .default_variables
                    .clone();
                if !variables.is_object() {
                    variables = json!({});
                }
                if let Some(overrides) = r["variables"].as_object() {
                    for (key, value) in overrides {
                        variables[key] = value.clone();
                    }
                }
                variables["endpointId"] = r["endpointId"].clone();
                variables["remote"] = r["remote"].clone();
                let run = {
                    let mut x = self.execution.lock().unwrap();
                    let def = x.engine.definitions.get(id).ok_or("流程尚未发布")?;
                    variables["protocolVersionId"] = def.protocol["id"].clone();
                    let run_id = x.engine.start(id, variables, now_ms())?;
                    let mut snapshot = json!(x.engine.runs[&run_id]);
                    crate::flow_host::redact_known(&mut snapshot, &x.secrets);
                    self.db.save_entity(&project, "runs", &run_id, &snapshot)?;
                    run_id
                };
                let actions = self.execution.lock().unwrap().engine.poll(now_ms());
                self.execute_actions(actions).await;
                Ok(json!({"id":run,"runId":run}))
            }
            "flow_stop" => {
                let actions = self
                    .execution
                    .lock()
                    .unwrap()
                    .engine
                    .cancel(required(&r, "runId")?, now_ms());
                self.execute_actions(actions).await;
                Ok(json!({"stopped":true}))
            }
            "flow_pause" => {
                self.execution
                    .lock()
                    .unwrap()
                    .engine
                    .pause(required(&r, "runId")?)?;
                Ok(json!({"paused":true}))
            }
            "flow_resume" | "flow_step" => {
                let actions = {
                    let mut x = self.execution.lock().unwrap();
                    if action == "flow_resume" {
                        x.engine.resume(required(&r, "runId")?, now_ms())
                    } else {
                        x.engine.single_step(required(&r, "runId")?, now_ms())
                    }
                };
                self.execute_actions(actions).await;
                Ok(json!({"continued":true}))
            }
            _ => Err(format!("未识别操作: {action}")),
        }
    }
    fn export(&self, r: &Value) -> Result<Value, String> {
        let dir = self.data_dir.join("export");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let project = self.project();
        match required(r, "kind")? {
            "protocol" => {
                let path = dir.join(format!("protocol-{}.xml", now_ms()));
                std::fs::write(&path, self.db.protocol_source(required(r, "id")?)?)
                    .map_err(|e| e.to_string())?;
                Ok(json!({"path":path}))
            }
            "packets" => {
                let format = r["format"].as_str().unwrap_or("jsonl");
                if !["jsonl", "json", "csv"].contains(&format) {
                    return Err("不支持的报文导出格式".into());
                }
                let path = dir.join(format!("packets-{}.{}", now_ms(), format));
                let mut file = std::io::BufWriter::new(
                    std::fs::File::create(&path).map_err(|e| e.to_string())?,
                );
                let count = self.db.export_packets(&project, format, &mut file)?;
                Ok(json!({"path":path,"rows":count}))
            }
            "project" => self.export_project(),
            _ => Err("不支持的导出类型".into()),
        }
    }
}

pub fn required<'a>(r: &'a Value, key: &str) -> Result<&'a str, String> {
    r[key]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| format!("缺少字段 {key}"))
}
pub fn parse_hex(hex: &str) -> Result<Vec<u8>, String> {
    let hex: String = hex.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    if hex.len() > 131014 {
        return Err("报文过长".into());
    }
    hex::decode(&hex).map_err(|_| "HEX必须是完整双位字节，不包含掩码或非十六进制字符".into())
}
pub fn format_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}
pub fn recover_domain(domain: &mut crate::domain::DomainStore) {
    use crate::domain::*;
    for d in domain.devices.values_mut() {
        d.registration_valid = false;
        d.state = DeviceState::Offline;
        d.active_calls.clear();
        d.cleanup_until_ms = None;
    }
    for s in domain.sessions.values_mut() {
        s.active = false;
        s.registered = false;
    }
    for r in domain.resources.values_mut() {
        if r.state != ResourceState::Released {
            r.state = ResourceState::Unknown;
        }
    }
    for c in domain.calls.values_mut() {
        if !matches!(
            c.state,
            CallState::Ended | CallState::Cancelled | CallState::Failed
        ) {
            c.active_devices.clear();
            c.end_started = true;
            c.state = CallState::CleanupPending;
        }
    }
}
fn load_execution(db: &Database, project: &str) -> Result<crate::flow_host::Execution, String> {
    use crate::runtime::*;
    let mut execution = crate::flow_host::Execution::default();
    for value in db.entities(project, "flow_versions")? {
        execution
            .engine
            .publish(serde_json::from_value(value).map_err(|e| e.to_string())?)?;
    }
    for value in db.entities(project, "runs")? {
        let mut run: FlowRun = serde_json::from_value(value).map_err(|e| e.to_string())?;
        if matches!(
            run.status,
            FlowStatus::Running | FlowStatus::Waiting | FlowStatus::Paused
        ) {
            run.status = FlowStatus::Interrupted;
            run.ended_ms = Some(now_ms());
            run.failure = Some("应用关闭，中断实例不自动重发".into());
            db.save_entity(project, "runs", &run.id, &json!(run))?;
        }
        execution.engine.runs.insert(run.id.clone(), run);
    }
    Ok(execution)
}
fn redact_fields(value: &mut Value, raw: &mut [String]) {
    if let Some(obj) = value.as_object_mut() {
        if obj.get("secret").and_then(Value::as_bool) == Some(true) {
            let start = obj.get("startOffset0").and_then(Value::as_u64).unwrap_or(0) as usize;
            let len = obj.get("byteLength").and_then(Value::as_u64).unwrap_or(0) as usize;
            for cell in raw.iter_mut().skip(start).take(len) {
                *cell = "**".into();
            }
            for key in ["rawHex", "decodedValue", "decimal", "binary"] {
                if obj.contains_key(key) {
                    obj.insert(key.into(), json!("[已脱敏]"));
                }
            }
        }
        for child in obj.values_mut() {
            redact_fields(child, raw);
        }
    } else if let Some(items) = value.as_array_mut() {
        for child in items {
            redact_fields(child, raw);
        }
    }
}
fn rewrite_redacted_hex(value: &mut Value, raw: &[String]) {
    if let Some(obj) = value.as_object_mut() {
        if let (Some(start), Some(len)) = (
            obj.get("startOffset0").and_then(Value::as_u64),
            obj.get("byteLength").and_then(Value::as_u64),
        ) {
            let cells = raw
                .iter()
                .skip(start as usize)
                .take(len as usize)
                .cloned()
                .collect::<Vec<_>>();
            if cells.iter().any(|b| b == "**") {
                obj.insert("rawHex".into(), json!(cells.join(" ")));
                for key in ["decodedValue", "decimal", "binary"] {
                    obj.insert(key.into(), Value::Null);
                }
            }
        }
        for child in obj.values_mut() {
            rewrite_redacted_hex(child, raw);
        }
    } else if let Some(items) = value.as_array_mut() {
        for child in items {
            rewrite_redacted_hex(child, raw);
        }
    }
}
