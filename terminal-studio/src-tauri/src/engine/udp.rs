//! UDP 收发核心：socket 生命周期管理、接收/规则引擎循环、日志落盘与事件推送

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

/// Values extracted as decimal/text must be converted back to their wire identity.
fn identity_hex(f: &ProtocolField, value: &str) -> Option<String> {
    match f.enc {
        simterm_core::models::FieldEnc::Uint if f.dict_key().is_none() => {
            let n = if let Some(h)=value.trim().strip_prefix("0x") {u128::from_str_radix(h,16).ok()?} else {value.trim().parse::<u128>().ok()?};
            Some(format!("{:0width$X}",n,width=f.length as usize*2))
        }
        simterm_core::models::FieldEnc::Ascii if f.dict_key().is_none() => Some(format!("{:<0width$}",hex::encode_upper(value.as_bytes()),width=f.length as usize*2)),
        _ => Some(compact_hex(value)),
    }
}

use sqlx::SqlitePool;
use tauri::{AppHandle, Emitter, Runtime};

use crate::db;
use simterm_core::rules;
use crate::engine::state::{EngineState, resolve_addr};
use simterm_core::fields::{
    dedupe_field_names, extract_field_values, merge_header_specs, parse_fields_summary,
    render_fields, req_ref_name,
};
use simterm_core::template::render_payload;
use simterm_core::error::{AppError, AppResult};
use simterm_core::models::{
    DefMode, Direction, FieldRole, LoginSuccessEvent, MsgType,
    Node, Protocol, ProtocolField, SendResult, UdpLogEvent,
};
use simterm_core::render::{compact_hex, DictMap, RenderCtx};

/// 紧凑 hex 串转空格分隔展示: "A1B2" → "A1 B2"(捕获日志可读用)
fn spaced_hex_like(compact: &str) -> String {
    let chars: Vec<char> = compact.chars().collect();
    chars
        .chunks(2)
        .map(|c| c.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join(" ")
}

/// 组装一帧的完整字段视图: 分组公共头(协议字段经 hdr_ref 显式覆盖取值) + 纯消息体。
/// 重名字段自动加 `#2/#3` 唯一键（协议内允许中文字段名重复, 参数/回填互不踩踏）。
///
/// 用于收包解析与自动应答参数提取, 与 send_datagram 的渲染顺序保持一致。
pub fn frame_specs(protocol: &Protocol, header: &[ProtocolField]) -> AppResult<Vec<ProtocolField>> {
    let specs = protocol.field_specs();
    let (head, body_only) = merge_header_specs(header, &specs)?;
    let mut all = head;
    all.extend(body_only);
    dedupe_field_names(&mut all);
    Ok(all)
}

const EVENT_UDP_LOG: &str = "udp-log";
const EVENT_ENGINE_STATUS: &str = "engine-status";
const EVENT_LOGIN_SUCCESS: &str = "login-success";
const EVENT_SESSION_UPDATED: &str = "session-updated";

/// 会话更新事件（收到报文中 Session 字段的值已存入节点）
#[derive(serde::Serialize, Clone)]
pub struct SessionUpdatedEvent {
    pub node_name: String,
    pub protocol_name: String,
    pub session_hex: String,
    /// 该会话归属的终端标识(源设备/序列号值); "_" = 单终端
    pub device: String,
}

/// 设备档案同步事件（配置应答成功、本批设备已写入/移出 device_sn 字典时推送）
#[derive(serde::Serialize, Clone)]
pub struct DeviceSavedEvent {
    pub node_name: String,
    pub protocol_name: String,
    /// 本次操作: 新增 / 删除
    pub op_label: String,
    /// 本批设备数
    pub count: usize,
    /// 本批序列号的可读形式（如 SN000009; 非可打印序列号用紧凑 hex）
    pub devices: Vec<String>,
}

/// 协议收包上下文: 整帧字段表 + 分组作用域字典（字段断言/解析共用）
async fn protocol_ctx(pool: &SqlitePool, p: &Protocol) -> AppResult<(Vec<ProtocolField>, DictMap)> {
    let header = db::protocol_headers(pool, p).await?;
    let frame = frame_specs(p, &header)?;
    let dicts = db::dict_map_for_group(pool, p.group_ref)
        .await
        .unwrap_or_default();
    Ok((frame, dicts))
}


/// 写入一条日志到 SQLite 并推送到前端
#[allow(clippy::too_many_arguments)]
pub async fn push_log<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    direction: Direction,
    node: Option<&Node>,
    local_addr: &str,
    remote_addr: &str,
    protocol: Option<&Protocol>,
    bytes: &[u8],
    parsed: Option<String>,
    status: &str,
) -> AppResult<UdpLogEvent> {
    push_log_with_boundary(app,pool,direction,node,local_addr,remote_addr,protocol,bytes,parsed,status,None,None).await
}

#[allow(clippy::too_many_arguments)]
async fn push_log_with_boundary<R: Runtime>(app: &AppHandle<R>, pool: &SqlitePool, direction: Direction, node: Option<&Node>, local_addr: &str, remote_addr: &str, protocol: Option<&Protocol>, bytes: &[u8], parsed: Option<String>, status: &str, send_after_log_id: Option<i64>, sent_at: Option<String>) -> AppResult<UdpLogEvent> {
    let hex = rules::normalize_hex(bytes);
    let text = String::from_utf8_lossy(bytes).to_string();
    let protocol_id = protocol.map(|p| p.protocol_id.clone());
    let protocol_name = protocol.map(|p| p.name.clone());
    let node_id = node.map(|n| n.id);
    let node_name = node.map(|n| n.name.clone());
    let ts = sent_at.unwrap_or_else(||chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string());

    let id = db::insert_log(
        pool,
        direction,
        node_id,
        local_addr,
        remote_addr,
        protocol_id.as_deref(),
        Some(hex.as_str()),
        Some(text.as_str()),
        parsed.as_deref(),
        status,
        send_after_log_id,
        &ts,
    )
    .await?;

    let event = UdpLogEvent {
        id,
        ts,
        direction,
        node_id,
        node_name,
        local_addr: local_addr.to_string(),
        remote_addr: remote_addr.to_string(),
        protocol_id,
        protocol_name,
        raw_hex: hex,
        raw_text: text,
        parsed,
        status: status.to_string(),
    };
    let _ = app.emit(EVENT_UDP_LOG, &event);
    Ok(event)
}

/// 推送引擎状态变更事件
pub fn emit_status<R: Runtime>(app: &AppHandle<R>, engine: &EngineState) {
    let _ = app.emit(EVENT_ENGINE_STATUS, engine.emit_status_event());
}

/// 确保节点的 UDP 服务已启动（未启动则绑定端口并进入监听循环）。
/// 返回装箱 Future: 该函数与 listen_loop/process_incoming/send_datagram 相互调用,
/// 若为普通 async fn 会形成不透明类型环(E0391), 在定义处装箱以打断。
pub fn ensure_server<'a, R: Runtime>(
    app: &'a AppHandle<R>,
    pool: &'a SqlitePool,
    engine: &'a Arc<EngineState>,
    node: &'a Node,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = AppResult<Arc<tokio::net::UdpSocket>>> + Send + 'a>> {
    Box::pin(async move {
        if let Some(sock) = engine.sockets.get(&node.id) {
            return Ok(sock.clone());
        }
        let local = resolve_addr(&node.local_ip, node.local_port).await?;
        let socket = tokio::net::UdpSocket::bind(local)
            .await
            .map_err(|e| simterm_core::error::AppError::msg(format!("绑定 {local} 失败(端口被占用?): {e}")))?;
        let socket = Arc::new(socket);

        engine.sockets.insert(node.id, socket.clone());
        let handle = tokio::spawn(listen_loop(
            app.clone(),
            pool.clone(),
            engine.clone(),
            node.clone(),
            socket.clone(),
        ));
        engine.listeners.insert(node.id, handle);
        emit_status(app, engine);
        Ok(socket)
    })
}

/// 停止节点监听：中止接收任务、释放端口、停掉该节点全部心跳与挂起入网
pub async fn stop_server<R: Runtime>(app: &AppHandle<R>, engine: &EngineState, node_id: i64) {
    let _flow_start = engine.flow_start_lock.lock().await;
    let runs: Vec<i64> = engine.flow_runs.iter().filter(|r|r.value().1==node_id).map(|r|*r.key()).collect();
    for run in runs { super::flow_trace::stop(engine,run); }
    if let Some((_, h)) = engine.listeners.remove(&node_id) {
        h.abort();
    }
    engine.sockets.remove(&node_id);
    engine.pending_logins.remove(&node_id);
    engine.used_save_sequences.lock().await.retain(|k| !k.starts_with(&format!("{node_id}|")));
    engine.pending_saves.lock().await.retain(|k,_| !k.starts_with(&format!("{node_id}|")));
    engine.pending_configs.retain(|k, _| k.node != node_id);
    if let Ok(mut applied) = engine.applied_configs.try_lock() { applied.retain(|(node,_,_),_| *node != node_id); }
    super::heartbeat::stop_node_heartbeats(app, engine, node_id);
    let keys: Vec<String> = engine.periodic.iter().filter(|x| x.value().0.node_id == node_id).map(|x| x.key().clone()).collect();
    for key in keys { if let Some((_, (_, h))) = engine.periodic.remove(&key) { h.abort(); } }
    emit_status(app, engine);
}

/// 渲染模板并发送一帧报文（自动确保 socket 已就绪），写入 OUT 日志
pub async fn send_datagram<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    engine: &Arc<EngineState>,
    node: &Node,
    protocol: &Protocol,
    params: &HashMap<String, String>,
    dest_override: Option<SocketAddr>,
) -> AppResult<SendResult> {
    // Delayed replies and periodic tasks must not render an obsolete session snapshot.
    let fresh_node = db::get_node(pool, node.id).await?;
    let node = &fresh_node;
    let seq = engine.seq_next(node.id);
    // 自动应答透传的终端标识(保留键): 命中收包时引擎把该帧的终端写进来,
    // 应答就带那一台的安全信息/会话; 手动发送没有这个键
    let mut params = params.clone();
    let mut selected_profile = None;
    if let Some(id) = params.remove("__terminal_id") {
        let profile: crate::commands::terminals::TerminalProfile = sqlx::query_as("SELECT * FROM terminal_profiles WHERE id=? AND node_ref=? AND system_ref=?")
            .bind(id.parse::<i64>().map_err(|_| AppError::msg("终端 ID 无效"))?).bind(node.id).bind(protocol.system_ref.unwrap_or(-1))
            .fetch_optional(pool).await?.ok_or_else(|| AppError::msg("终端不属于当前通道/协议系统"))?;
        params.insert("__device_key".into(), profile.device_key.clone());
        selected_profile = Some(profile);
    }
    let inherited_device = params.remove("__device_key");
    // 字典按协议所属分组解析: 全局共享字典 + 该分组专属字典（同 Key 时后者覆盖前者）。
    // 字典不再混入 params, 因此不会与字段名、内置变量抢命名空间。
    let dicts = db::dict_map_for_group(pool, protocol.group_ref)
        .await
        .unwrap_or_default();
    // 分组公共 UDP 头: 该协议归属分组启用公共头时, 头部字段拼在报文最前面;
    // 协议字段经 hdr_ref 显式覆盖公共头取值后不再拼入消息体
    let header: Vec<ProtocolField> = db::protocol_headers(pool, &protocol).await?;
    // 整帧统一去重命名（重名字段 #k 唯一键）后, 再按头部长度切回 头/体 两段分别渲染
    let (head_fields, body_only): (Vec<ProtocolField>, Vec<ProtocolField>) =
        if protocol.def_mode == DefMode::Fields {
            let mut all = frame_specs(protocol, &header)?;
            let head_len = header.len();
            let body = all.split_off(head_len.min(all.len()));
            (all, body)
        } else {
            (header, Vec::new())
        };
    let all_fields: Vec<ProtocolField> = head_fields.iter().chain(body_only.iter()).cloned().collect();
    if let Some(profile) = &selected_profile {
        let properties = serde_json::from_str(&profile.fields_json).map_err(|_|AppError::msg("设备档案内容无效"))?;
        let mut mapped = simterm_core::profile::params_for(&all_fields, &properties)?;
        mapped.extend(params); // Explicit edits win over stored properties.
        params = mapped;
        for f in all_fields.iter().filter(|f| f.role == FieldRole::Session) {
            let raw = profile.session_hex.clone().unwrap_or_else(|| "00".repeat(f.length as usize));
            params.insert(f.name.clone(), crate::devices::identity_param(f, &raw)?);
        }
    }
    // 一遍渲染（tlen 占位）; {tlen} 变量取真实总长, 需先渲染一遍求总长再重渲染
    // 终端标识(多终端单通道): 自动应答透传的收包终端优先;
    // 其次取第一个 源设备/设备序列号 字段本次发送的实际取值(参数优先, 其次定义值;
    // 含 {变量} 的取不到 → 回退单终端桶 "_")。
    // {session}/{last:字段} 按该终端的捕获桶取值, 注册过哪台就带哪台的安全信息
    let raw_device_key = inherited_device.clone().or_else(|| {
        head_fields
            .iter()
            .chain(body_only.iter())
            .find(|f| f.role == FieldRole::SrcId || f.role == FieldRole::DeviceSn)
            .and_then(|f| {
                let v = params
                    .get(f.name.as_str())
                    .map(|s| s.trim())
                    .filter(|s| !s.is_empty())
                    .unwrap_or(f.value.trim());
                if v == "{node_id}" {
                    let mut field=f.clone();field.when=None;
                    return render_fields(&[field], &RenderCtx::new(node,seq,&params,&dicts)).ok().map(hex::encode_upper);
                }
                if v.is_empty() || v.contains('{') { None } else { identity_hex(f, v) }
            })
    });
    let device_key = Some(format!("s{}:{}", protocol.system_ref.unwrap_or(-1), raw_device_key.as_deref().unwrap_or("_")));
    // A selected terminal supplies the sending identity even when protocols use different field names.
    // 继承的终端上下文(自动应答/自动心跳的 __device_key)同样要回填身份字段,
    // 否则多终端单通道下自动帧的源地址会落回公共头默认值, 令牌与身份错位被对端拒绝;
    // 继承路径用 or_insert: 显式传入的取值(如 {req:} 回显)优先。
    if selected_profile.is_some() || inherited_device.is_some() {
        if let Some(f) = crate::devices::identity_field(&all_fields) {
            let raw = raw_device_key.clone().unwrap_or_default();
            let v = match f.enc {
                simterm_core::models::FieldEnc::Uint if f.dict_key().is_none() => u128::from_str_radix(&raw,16).map(|n| n.to_string()).map_err(|_|AppError::msg("终端标识超出整数范围"))?,
                simterm_core::models::FieldEnc::Ascii if f.dict_key().is_none() => String::from_utf8(hex::decode(&raw).map_err(|_|AppError::msg("终端标识不是 Hex"))?).map_err(|_|AppError::msg("终端标识不是有效文本"))?.trim_end_matches('\0').to_string(),
                _ => raw,
            };
            if selected_profile.is_some() {
                params.insert(f.name.clone(), v);
            } else {
                params.entry(f.name.clone()).or_insert(v);
            }
        }
    }
    // 字段模式: 头+体一次性按帧序渲染 —— 分支条件可跨 头/体 引用
    // (体字段的 when 引用公共头字段时, 分段渲染会丢失头字段已取值导致分支误跳)
    let render = |tlen: u64| -> AppResult<Vec<u8>> {
        let ctx =
            RenderCtx::new(node, seq, &params, &dicts).with_tlen(tlen).with_device_key(device_key.clone());
        if protocol.def_mode == DefMode::Fields {
            let mut all = head_fields.clone();
            all.extend(body_only.iter().cloned());
            return render_fields(&all, &ctx);
        }
        let body_bytes = render_payload(protocol, &ctx)?;
        if head_fields.is_empty() {
            return Ok(body_bytes);
        }
        let head_bytes = render_fields(&head_fields, &ctx)?;
        let mut full = Vec::with_capacity(head_bytes.len() + body_bytes.len());
        full.extend_from_slice(&head_bytes);
        full.extend_from_slice(&body_bytes);
        Ok(full)
    };
    let mut bytes = render(0)?;
    let uses_tlen = head_fields.iter().any(|f| f.value.contains("{tlen"))
        || body_only.iter().any(|f| f.value.contains("{tlen"))
        || protocol.payload_template.contains("{tlen");
    if uses_tlen {
        bytes = render(bytes.len() as u64)?;
    }
    if bytes.is_empty() {
        return Err(AppError::msg(
            "报文内容为空（协议无字段且系统未启用公共头）",
        ));
    }
    // 发送目标: 自动应答与手动发送一致, 均为通道配置的远端地址
    let dest = match dest_override {
        Some(d) => d,
        None => resolve_addr(&node.remote_ip, node.remote_port).await?,
    };
    let socket = ensure_server(app, pool, engine, node).await?;
    let pending = crate::business::track(pool, engine, node, protocol, &all_fields, &bytes, &dicts, dest).await?;
    let send_after_log_id: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(id),0) FROM logs").fetch_one(pool).await?;
    let sent_at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();
    let n = match socket.send_to(&bytes, dest).await {
        Ok(n) => n,
        Err(e) => { if let Some(key) = pending { engine.pending_saves.lock().await.remove(&key); } return Err(e.into()); }
    };
    let hex = rules::normalize_hex(&bytes);
    let local = socket
        .local_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| format!("{}:{}", node.local_ip, node.local_port));

    let sent = push_log_with_boundary(
        app,
        pool,
        Direction::Out,
        Some(node),
        &local,
        &dest.to_string(),
        Some(protocol),
        &bytes,
        None,
        "sent",
        Some(send_after_log_id),
        Some(sent_at),
    )
    .await?;
    Ok(SendResult { bytes: n, hex, log_id: sent.id })
}

/// 接收循环：持续 recv_from 并交给规则引擎处理
async fn listen_loop<R: Runtime>(
    app: AppHandle<R>,
    pool: SqlitePool,
    engine: Arc<EngineState>,
    node: Node,
    socket: Arc<tokio::net::UdpSocket>,
) {
    let mut buf = vec![0u8; 65535];
    loop {
        match socket.recv_from(&mut buf).await {
            Ok((n, src)) => {
                let data = buf[..n].to_vec();
                if let Err(e) =
                    process_incoming(&app, &pool, &engine, &node, &socket, &data, src).await
                {
                    eprintln!("[engine] 节点[{}]处理收包失败: {e}", node.name);
                }
            }
            // Windows 上对不可达端口发送后, 下一次 recv 会返回 ConnectionReset, 需忽略
            Err(e) if e.kind() == std::io::ErrorKind::ConnectionReset => {
                continue;
            }
            Err(e) => {
                // socket 已关闭等致命错误: 记录后退出循环
                eprintln!("[engine] 节点[{}]接收错误, 监听退出: {e}", node.name);
                break;
            }
        }
    }
    // 兜底清理: 若 socket 尚在表中(异常退出路径), 同步移除并通知前端
    if engine.sockets.contains_key(&node.id) {
        engine.sockets.remove(&node.id);
        engine.listeners.remove(&node.id);
        emit_status(&app, &engine);
    }
}

/// 规则引擎处理一帧收包：匹配协议 → 记录日志 → 入网判定 → 自动应答
async fn process_incoming<R: Runtime>(
    app: &AppHandle<R>,
    pool: &SqlitePool,
    engine: &Arc<EngineState>,
    node: &Node,
    socket: &Arc<tokio::net::UdpSocket>,
    data: &[u8],
    src: SocketAddr,
) -> AppResult<()> {
    // 每包重取节点: 监听循环持有的 Node 是启动时的快照, 注册应答写入的
    // session 等最新状态要传导给自动应答渲染({session}), 否则会带过期 token 被对端拒绝
    let node = &db::get_node(pool, node.id)
        .await
        .unwrap_or_else(|_| node.clone());
    let local = socket
        .local_addr()
        .map(|a| a.to_string())
        .unwrap_or_else(|_| format!("{}:{}", node.local_ip, node.local_port));
    let remote = src.to_string();

    let protocols = db::matchable_protocols(pool).await.unwrap_or_default();
    // 两阶段匹配: ① 正则候选（无正则但有字段断言的协议直接候选）
    //             ② 字段断言: 按协议帧布局解析收包, 全部「字段=期望值」条件满足才算命中;
    //                多个候选同时满足断言时优先取「帧长与布局一致」的（变长布局豁免）,
    //                避免短布局截断长帧造成跨协议误命中（如 呼叫 vs 记录数据）

    let mut matched: Option<&Protocol> = None;
    let mut loose: Option<&Protocol> = None;
    for cand in &protocols {
        if cand.match_rule.as_deref().is_some_and(|rule| !rule.trim().is_empty())
            && rules::match_candidates(&engine.regex_cache, data, std::slice::from_ref(cand)).is_empty() {
            continue;
        }
        let Ok((frame, dicts)) = protocol_ctx(pool, cand).await else {
            continue; // 协议帧布局配置错误（如 hdr_ref 指向不存在的头字段）, 跳过
        };
        // 信令类型码(role=信令类型)自动并入断言: 身份码进协议数据, 收包按码识别;
        // 无信令类型且无手写断言的协议无从识别, 不参与匹配
        let conds = rules::effective_conds(&frame, &cand.match_conds());
        if conds.is_empty() && !cand.match_rule.as_deref().is_some_and(|rule| !rule.trim().is_empty()) {
            continue;
        }
        if !rules::check_match_conds(&frame, &dicts, &conds, data) {
            continue;
        }
        if loose.is_none() {
            loose = Some(cand); // 兜底: 断言满足但帧长不一致
        }
        if rules::frame_len_matches(&frame, data) {
            matched = Some(cand);
            break;
        }
    }
    let p = match matched.or(loose) {
        Some(p) => p,
        None => {
            push_log(
                app,
                pool,
                Direction::In,
                Some(node),
                &local,
                &remote,
                None,
                data,
                None,
                "no-match",
            )
            .await?;
            return Ok(());
        }
    };

    // 请求报文各字段的原始值（字段模式才有）; 供自动应答按字段回填（重名字段带 #k 唯一键）
    let mut req_values: HashMap<String, String> = HashMap::new();
    let mut fields_view: Vec<ProtocolField> = Vec::new();
    // 本帧终端标识: 源设备/设备序列号字段的实际值(多终端单通道按它分桶捕获/回包)
    let mut frame_device_key: Option<String> = None;

    let parsed = if p.def_mode == DefMode::Fields {
        // 带上字典, 绑定字段可直接显示条目含义, 如 设备类型=02 (湿度传感器);
        // 分组公共头在前, 协议字段经 hdr_ref 覆盖头取值, 与发送渲染的字节顺序一致
        let (specs, dicts) = protocol_ctx(pool, p).await?;
        fields_view = specs.clone();
        req_values = extract_field_values(&specs, data, &dicts);
        // 本帧终端归属按通道角色(显式配置)判定, 不按协议类型猜:
        // 模拟设备通道收到的都是服务端下发(应答/来电/配置数据)→ 目标设备即终端;
        // 服务端/平台通道收到的是设备上报(注册/心跳)→ 源设备即终端。
        let terminal_field = |role: FieldRole| {
            fields_view.iter().find(|f| f.role == role).and_then(|f| {
                req_values
                    .get(&f.name)
                    .and_then(|v| identity_hex(f, v))
                    .filter(|v| !v.is_empty())
            })
        };
        let prefer_dst = node.role == simterm_core::models::NodeRole::Simulator;
        frame_device_key = if prefer_dst {
            terminal_field(FieldRole::DstId)
                .or_else(|| terminal_field(FieldRole::SrcId))
                .or_else(|| terminal_field(FieldRole::DeviceSn))
        } else {
            terminal_field(FieldRole::SrcId)
                .or_else(|| terminal_field(FieldRole::DeviceSn))
                .or_else(|| terminal_field(FieldRole::DstId))
        };
        format!("命中 {} | {}", p.name, parse_fields_summary(&specs, data, &dicts))
    } else {
        format!("命中 {}", p.name)
    };
    // 捕获动作在写日志前完成, 命中摘要里直接可见「从这条协议抓了什么进哪个终端的桶」
    // —— 捕获跟着协议声明走: 只有本协议标了 会话/响应捕获 语义的字段才抓,
    //    同名帧(如两条通道配置)未标则不参与, 不存在"从配置还是指令取"的歧义
    let mut captured_note = String::new();
    let ack_dicts = db::dict_map_for_group(pool, p.group_ref).await?;
    let save_note = match crate::business::accept(pool, engine, node, p, &fields_view, data, &ack_dicts, src).await {
        Ok(note) => note,
        Err(error) => format!("｜业务资料未保存：{error}"),
    };
    if p.def_mode == DefMode::Fields {
        let raw_device_key = frame_device_key.clone().unwrap_or_else(|| "_".to_string());
        let device_key = format!("s{}:{}", p.system_ref.unwrap_or(-1), raw_device_key);
        let mut got: Vec<String> = Vec::new();
        for f in fields_view.iter().filter(|f| {
            (p.msg_type == MsgType::Response && f.role == FieldRole::Session) || f.role == FieldRole::Capture
        }) {
            let Some(v) = req_values.get(&f.name) else { continue };
            let compact = identity_hex(f, v).ok_or_else(|| AppError::msg(format!("捕获字段 {} 无法编码",f.name)))?;
            let meaningful = !compact.is_empty() && compact.chars().any(|c| c != '0');
            if !meaningful {
                continue; // 全零/空 = 未分配, 不覆盖已有值
            }
            let show_name = if f.role == FieldRole::Session {
                format!("{}(会话)", simterm_core::fields::display_field_name(&f.name))
            } else {
                simterm_core::fields::display_field_name(&f.name)
            };
            got.push(format!("{}={}", show_name, spaced_hex_like(&compact)));
            // Store only in the scoped terminal bucket and matching terminal profile.
            if f.role == FieldRole::Session {
                db::update_node_capture(pool, node.id, &device_key, "__session", &compact).await?;
                if raw_device_key != "_" {
                    sqlx::query("UPDATE terminal_profiles SET session_hex=? WHERE node_ref=? AND system_ref=? AND device_key=?")
                        .bind(&compact).bind(node.id).bind(p.system_ref.unwrap_or(-1)).bind(&raw_device_key).execute(pool).await?;
                }
                {
                    {
                        let _ = app.emit(
                            EVENT_SESSION_UPDATED,
                            &SessionUpdatedEvent {
                                node_name: node.name.clone(),
                                protocol_name: p.name.clone(),
                                session_hex: compact,
                                device: device_key.clone(),
                            },
                        );
                    }
                }
            } else {
                db::update_node_capture(pool, node.id, &device_key, &f.name, &compact).await?;
            }
        }
        if !got.is_empty() {
            let dev = if device_key == "_" {
                String::new()
            } else {
                format!(" · 终端 {device_key}")
            };
            captured_note = format!("｜已捕获 {}{}", got.join("、"), dev);
        }
    }
    // ---- 触发定时发送(保活): 协议声明了触发(如 注册响应→心跳)时, 命中即自动开始
    //      周期发送指定协议; 同一任务已在跑不重复 ----
    let mut trigger_note = String::new();
    if let Some(tp) = p
        .auto_periodic
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let interval = if p.auto_periodic_interval_ms >= 100 {
            p.auto_periodic_interval_ms
        } else {
            30_000
        };
        let key = format!("{}:{tp}:{}", node.id, frame_device_key.as_deref().unwrap_or(""));
        if engine.periodic.contains_key(&key) {
            // 已在跑: 不重复启动
        } else if let Ok(target) = db::get_protocol(pool, tp).await {
            let tname = target.name.clone();
            let mut periodic_params = HashMap::new();
            if let Some(k) = &frame_device_key { periodic_params.insert("__device_key".into(), k.clone()); }
            match super::heartbeat::start_periodic_send_with_params(
                app.clone(),
                pool.clone(),
                engine.clone(),
                node.id,
                tp,
                interval,
                periodic_params,
            )
            .await
            {
                Ok(()) => {
                    emit_status(app, engine);
                    trigger_note = format!("｜已启动定时发送「{tname}」·{}秒", interval / 1000);
                }
                Err(e) => trigger_note = format!("｜定时发送「{tname}」启动失败: {e}"),
            }
        } else {
            trigger_note = format!("｜触发定时发送的协议「{tp}」不存在");
        }
    }
    let parsed = format!("{parsed}{captured_note}{trigger_note}{save_note}");
    push_log(
        app,
        pool,
        Direction::In,
        Some(node),
        &local,
        &remote,
        Some(p),
        data,
        Some(parsed),
        "ok",
    )
    .await?;

    if save_note.contains("业务资料已保存") || save_note.contains("业务资料删除处理完成") {
        let _ = app.emit("business-records-updated", ());
        let _ = app.emit("devices-updated", ());
    }

    // ---- 业务流程自动触发: 命中协议出现在某流程的触发列表即启动(已在跑不重复) ----
    super::flow::trigger_flows_for(app, pool, engine, &p.protocol_id).await;

    // ---- 入网判定：命中的协议正是挂起入网所等待的应答 ----
    let login_hit = engine
        .pending_logins
        .get(&node.id)
        .map(|pl| pl.expect_protocol_id == p.protocol_id)
        .unwrap_or(false);
    if login_hit {
        engine.pending_logins.remove(&node.id);
        let ev = LoginSuccessEvent {
            node_id: node.id,
            node_name: node.name.clone(),
            protocol_id: p.protocol_id.clone(),
            protocol_name: p.name.clone(),
        };
        let _ = app.emit(EVENT_LOGIN_SUCCESS, &ev);
        // 自动启动该节点 auto_start 的心跳绑定
        if let Ok(bindings) = db::auto_start_heartbeats(pool, node.id).await {
            for b in bindings {
                let _ =
                    super::heartbeat::spawn_heartbeat(app.clone(), pool.clone(), engine.clone(), &b)
                        .await;
            }
            emit_status(app, engine);
        }
    }

    // ---- 自动应答：按预配置协议延迟/立即回发给收包源地址 ----
    if let Some(resp_pid) = p.auto_respond.clone() {
        if resp_pid.is_empty() {
            return Ok(());
        }
        let resp = match db::get_protocol(pool, &resp_pid).await {
            Ok(r) => r,
            Err(e) => {
                push_log(
                    app,
                    pool,
                    Direction::In,
                    Some(node),
                    &local,
                    &remote,
                    Some(p),
                    &[],
                    Some(format!("命中 [{}] 但自动应答协议 [{resp_pid}] 不存在: {e}", p.name)),
                    "error",
                )
                .await?;
                return Ok(());
            }
        };
        let delay_ms = p.respond_delay_ms.max(0) as u64;
        // 应答参数回填（仅字段模式应答协议）:
        //   1. 值为 {req:字段名} → 显式取请求报文中该字段的实际值（如 ACK 回显请求序号）
        //   2. 同名字段定义值留空 → 隐式回填请求值; 定义了值的字段（如固定命令字）保持不变
        // 字段名含重名唯一键（#k）, 与请求提取侧一致
        let mut echo_params: HashMap<String, String> = HashMap::new();
        // 透传本帧终端标识: 应答渲染 {session}/{last:} 取这一台终端的捕获桶
        if let Some(k) = &frame_device_key {
            echo_params.insert("__device_key".into(), k.clone());
        }
        if resp.def_mode == DefMode::Fields && !req_values.is_empty() {
            let resp_specs = match db::protocol_headers(pool, &resp).await.and_then(|header| frame_specs(&resp, &header))
            {
                Ok(s) => s,
                Err(e) => {
                    push_log(
                        app,
                        pool,
                        Direction::In,
                        Some(node),
                        &local,
                        &remote,
                        Some(p),
                        &[],
                        Some(format!("自动应答协议 [{}] 帧布局错误: {e}", resp.name)),
                        "error",
                    )
                    .await?;
                    return Ok(());
                }
            };
            for f in &resp_specs {
                let hit = match req_ref_name(&f.value) {
                    Some(name) => req_values.get(name),
                    None if f.value.trim().is_empty() => req_values.get(f.name.as_str()),
                    None => None,
                };
                if let Some(v) = hit {
                    echo_params.insert(f.name.clone(), v.clone());
                }
            }
        }
        let app2 = app.clone();
        let pool2 = pool.clone();
        let engine2 = engine.clone();
        let node2 = node.clone();
        let p2 = p.clone();
        let resp_name = resp.name.clone();
        // 应答目标 = 通道配置的远端地址(与手动发送一致), 不回包给收包源端口 ——
        // 对端常用临时端口发包, 回源会让应答发到对端没监听的端口上
        let remote = format!("{}:{}", node2.remote_ip, node2.remote_port);
        tokio::spawn(async move {
            if delay_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            }
            if let Err(e) =
                send_datagram(&app2, &pool2, &engine2, &node2, &resp, &echo_params, None)
                    .await
            {
                let local2 = format!("{}:{}", node2.local_ip, node2.local_port);
                let _ = push_log(
                    &app2,
                    &pool2,
                    Direction::Out,
                    Some(&node2),
                    &local2,
                    &remote,
                    Some(&p2),
                    &[],
                    Some(format!(
                        "命中 [{}] → 自动应答 [{}] 发送/渲染失败: {e}",
                        p2.name, resp_name
                    )),
                    "error",
                )
                .await;
            }
        });
    }
    Ok(())
}
