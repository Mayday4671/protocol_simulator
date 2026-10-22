//! 业务流程引擎 E2E: PD 套件「设备接入全流程」(逐台注册→心跳→参数→状态→注销)。
//!
//! 前置: `python scripts/pd_server_gui.py 4002`、已导入 PD_1.0.xml(含 <flow>)
//! 运行: `cargo run --features e2e --bin e2e_flow`
//! 断言: 流程驱动 3 台终端各自注册领取令牌(档案 session 写入),
//!       首台走完 心跳→参数设置→状态上报→注销 全链(平台逐条成功应答),
//!       流程自然结束(运行句柄摘除)。
#![cfg(feature = "e2e")]

use simulated_terminal_lib::engine::flow;
use simulated_terminal_lib::engine::state::EngineState;
use simulated_terminal_lib::{db, AppState};
use sqlx::sqlite::SqlitePoolOptions;
use std::sync::Arc;

type Row = (i64, String, String, Option<String>);

async fn recent_logs(pool: &sqlx::SqlitePool) -> Vec<Row> {
    sqlx::query_as::<_, Row>(
        "SELECT id, direction, protocol_id, raw_hex FROM logs ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

fn slice_hex(raw: &str, lo: usize, hi: usize) -> String {
    let compact: String = raw.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    compact
        .get(lo * 2..hi * 2)
        .unwrap_or_default()
        .to_ascii_uppercase()
}

fn fail(msg: &str) -> ! {
    println!("[E2E-FAIL] {msg}");
    std::process::exit(1);
}

#[tokio::main]
async fn main() {
    match tokio::net::UdpSocket::bind("0.0.0.0:4002").await {
        Ok(sock) => {
            drop(sock);
            fail("本机 4002 无 py 平台服务端, 先运行: python scripts/pd_server_gui.py 4002");
        }
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            println!("[E2E] py 平台在线(4002 已监听)")
        }
        Err(e) => fail(&format!("4002 探测异常: {e}")),
    }

    let appdata = std::env::var("APPDATA").unwrap();
    let src = format!("{appdata}\\com.simterm.desktop\\simterm.db");
    let dst = std::env::temp_dir().join(format!("simterm_e2e_flow_{}.db", std::process::id()));
    let dst_str = dst.to_str().unwrap().to_string();
    let _ = std::fs::remove_file(&dst);
    {
        let opts = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&src)
            .read_only(true);
        let src_pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();
        sqlx::query(&format!(
            "VACUUM INTO '{}'",
            dst_str.replace('\'', "''")
        ))
        .execute(&src_pool)
        .await
        .unwrap();
        src_pool.close().await;
    }
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect(&format!("sqlite://{dst_str}"))
        .await
        .unwrap();

    let app = tauri::test::mock_app();
    let handle = app.handle().clone();
    let engine = Arc::new(EngineState::new());

    let flow_id: i64 = sqlx::query_scalar("SELECT id FROM business_flows WHERE name='设备接入全流程'")
        .fetch_optional(&pool)
        .await
        .unwrap()
        .unwrap_or_else(|| fail("库中缺少流程「设备接入全流程」(先导入 PD_1.0.xml)"));

    // 清一下库中旧日志, 断言只看本次运行的帧
    sqlx::query("DELETE FROM logs").execute(&pool).await.unwrap();
    // 令牌从零开始: 清空设备会话, 平台沿用旧分配也无所谓(注册沿用)
    sqlx::query("UPDATE terminal_profiles SET session_hex=NULL")
        .execute(&pool)
        .await
        .unwrap();

    let run_id = flow::start_flow(handle.clone(), pool.clone(), engine.clone(), flow_id, Vec::new())
        .await
        .unwrap();
    println!("[E2E] 流程已启动 run_id={run_id}");

    // 等流程自然结束(句柄摘除)
    let mut done = false;
    for _ in 0..600 {
        if !engine.flow_runs.contains_key(&run_id) {
            done = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    if !done {
        fail("流程 60 秒未结束(疑似卡死)");
    }
    println!("[E2E] 流程执行结束, 校验帧序列…");

    let logs = recent_logs(&pool).await;
    let count = |dir: &str, pid: &str, pred: &dyn Fn(&str) -> bool| {
        logs.iter()
            .filter(|(_, d, p, raw)| d == dir && p == pid && pred(raw.as_deref().unwrap_or("")))
            .count()
    };

    // ① 三台终端各自注册成功(逐台遍历)
    let grants = count("in", "pd_reg_grant", &|r| slice_hex(r, 16, 17) == "00");
    if grants < 3 {
        fail(&format!("逐台注册应有 3 条成功许可, 实际 {grants}"));
    }
    // ② 全部设备档案拿到令牌(只看智能配电系统的设备)
    let sessions: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT device_key, session_hex FROM terminal_profiles WHERE system_ref=(SELECT id FROM protocol_systems WHERE name='智能配电接入协议') ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    for (key, s) in &sessions {
        if s.as_deref().unwrap_or("0000") == "0000" {
            fail(&format!("终端 {key} 注册后未拿到令牌"));
        }
    }
    // ③ 首台(A=00000001)走完 心跳→参数→状态→注销, 平台逐条成功
    for (out_pid, in_pid) in [
        ("pd_heartbeat", "pd_heartbeat_resp"),
        ("pd_param_set", "pd_param_set_resp"),
        ("pd_status_report", "pd_report_ack"),
        ("pd_logout", "pd_logout_resp"),
    ] {
        let src_ok = count("out", out_pid, &|r| slice_hex(r, 2, 6) == "00000001");
        if src_ok == 0 {
            fail(&format!("{out_pid} 未按首台终端发送"));
        }
        let ack_ok = count("in", in_pid, &|r| {
            slice_hex(r, 6, 10) == "00000001" && slice_hex(r, 16, 17) == "00"
        });
        if ack_ok == 0 {
            fail(&format!("{in_pid} 平台未回成功(令牌认证未通过?)"));
        }
    }
    let token_a = sessions
        .iter()
        .find(|(k, _)| k == "00000001")
        .and_then(|(_, s)| s.clone())
        .unwrap();
    let heart_out = logs
        .iter()
        .find(|(_, d, p, _)| d == "out" && p == "pd_heartbeat");
    if let Some((_, _, _, raw)) = heart_out {
        let raw = raw.as_deref().unwrap_or("");
        if slice_hex(raw, 12, 14) != token_a {
            fail(&format!("流程心跳帧头令牌 {}/{token_a} 不符", slice_hex(raw, 12, 14)));
        }
    }

    // ================= 设备通信协议(COMM): 配置→注册→通信→结束 =================
    let comm_flow: i64 = sqlx::query_scalar("SELECT id FROM business_flows WHERE name='设备通信全流程'")
        .fetch_optional(&pool)
        .await
        .unwrap()
        .unwrap_or_else(|| fail("库中缺少流程「设备通信全流程」(先导入 COMM_1.0.xml)"));
    sqlx::query("DELETE FROM logs").execute(&pool).await.unwrap();
    sqlx::query("UPDATE terminal_profiles SET session_hex=NULL").execute(&pool).await.unwrap();
    let run2 = flow::start_flow(handle.clone(), pool.clone(), engine.clone(), comm_flow, Vec::new())
        .await
        .unwrap();
    println!("[E2E] 设备通信流程已启动 run_id={run2}");
    let mut done = false;
    for _ in 0..600 {
        if !engine.flow_runs.contains_key(&run2) {
            done = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    if !done {
        fail("设备通信流程 60 秒未结束(疑似卡死)");
    }
    let logs = recent_logs(&pool).await;
    let has = |dir: &str, pid: &str, min: usize, what: &str| {
        let n = logs.iter().filter(|(_, d, p, _)| d == dir && p == pid).count();
        if n < min {
            fail(&format!("{what}: 期望≥{min} 实际 {n}"));
        }
    };
    // ① 配置设备(逐台): 两台各收到配置数据, 引擎自动回配置应答
    has("in", "dev_cfg_data", 2, "逐台配置数据");
    has("out", "dev_cfg_ack", 2, "配置应答自动接续");
    // ② 注册(逐台): 两台各领安全信息
    has("in", "dev_reg_resp", 2, "逐台注册响应");
    let sessions: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT device_key, session_hex FROM terminal_profiles WHERE system_ref=(SELECT id FROM protocol_systems WHERE name='设备通信协议') ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    for (key, sess) in &sessions {
        if sess.as_deref().unwrap_or("") == "" {
            fail(&format!("终端 {key} 注册后未存安全信息"));
        }
    }
    // ③ 通信: A主叫B → 平台推来电(B) → 引擎自动应答 → 平台回呼叫确认(A)
    has("out", "dev_call", 1, "呼叫请求");
    has("in", "dev_incoming", 1, "来电推送到B");
    has("out", "dev_call_ack", 1, "被叫自动应答");
    has("in", "dev_call_cfm", 1, "呼叫确认");
    // ④ 通信保活: 心跳 + 应答
    has("out", "dev_heartbeat", 1, "流程心跳");
    has("in", "dev_heartbeat_resp", 1, "心跳应答");
    println!("[E2E] 设备通信全流程验证通过 ✅ (逐台配置→逐台注册领安全信息→A主叫B自动接续来电应答→呼叫确认→保活→结束)");

    // ================= 内嵌信令定义 + 内容分流 =================
    // 自包含测试系统(独立分组无公共头/独立通道), e2e 自任对端:
    // 流程直接携带信令字段定义(不经协议管理), 保存时自动生成协议; 应答成功后按接收字段值路由
    use simulated_terminal_lib::engine::flow::{RouteRule, SignalDef};
    use simterm_core::models::{FieldEnc, FieldRole, ProtocolField};
    let base = |name: &str, len: u32, value: &str| ProtocolField {
        header_key: None, profile_key: None, section: None, section_name: None, name: name.into(), length: len, enc: FieldEnc::Hex, value: value.into(),
        dict: None, unit_fields: Vec::new(), role: FieldRole::None, when: None, hdr_ref: None,
    };
    let code = |name: &str, value: &str| ProtocolField {
        role: FieldRole::TypeCode, ..base(name, 1, value)
    };
    // 测试系统 + 无头分组 + 通道
    let t_sys: i64 = sqlx::query_scalar(
        "INSERT INTO protocol_systems(name) VALUES('内嵌信令测试系统') RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let t_grp: i64 = sqlx::query_scalar(
        "INSERT INTO protocol_groups(system_ref,name,header_enabled) VALUES(?,?,'0') RETURNING id",
    )
    .bind(t_sys)
    .bind("测试分组")
    .fetch_one(&pool)
    .await
    .unwrap();
    let t_node: i64 = sqlx::query_scalar(
        "INSERT INTO nodes(name,role,local_ip,local_port,remote_ip,remote_port,group_bind_json,enabled) VALUES('内嵌测试通道','simulator','0.0.0.0',9151,'127.0.0.1',9152,'[]',1) RETURNING id",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE nodes SET group_bind_json=? WHERE id=?")
        .bind(format!("[{t_grp}]"))
        .bind(t_node)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO terminal_profiles(name,node_ref,system_ref,protocol_ref,device_key,fields_json) VALUES('内嵌终端',?,?, 'flw_s1req','000001','{}')")
        .bind(t_node)
        .bind(t_sys)
        .execute(&pool)
        .await
        .unwrap();

    let mut def = simulated_terminal_lib::engine::flow::FlowDef {
        id: 0,
        name: "inline_e2e".into(),
        system_ref: Some(t_sys),
        group_ref: Some(t_grp),
        description: None,
        trigger_protocols: Vec::new(),
        schedule_interval_ms: 0,
        steps: vec![
            simulated_terminal_lib::engine::flow::FlowStep {
                id: "s1".into(), name: "内嵌注册".into(), x: 360.0, y: 60.0,
                protocol_id: String::new(), terminal_mode: "each".into(), terminal_id: None,
                params: Default::default(),
                expect_protocol: None, timeout_ms: 5000, delay_ms: 0,
                next: None, on_success: Some("s2".into()), on_failure: None, on_timeout: None,
                request_def: Some(SignalDef { header_refs: None,
                    name: "内嵌注册请求".into(),
                    fields: vec![code("信令", "31"), base("型号", 1, "01")],
                }),
                expect_def: Some(SignalDef { header_refs: None,
                    name: "内嵌注册许可".into(),
                    fields: vec![code("信令", "32"), base("注册结果", 1, "00")],
                }),
                // 路由命中验证: 注册结果=00 → 分流至 s3(内嵌保活); 未命中才走默认成功边 s2(内嵌注销)
                routes: vec![RouteRule { field: "注册结果".into(), value: "00".into(), to: "s3".into() }], allow_unkeyed_reply: true, ..Default::default()
            },
            simulated_terminal_lib::engine::flow::FlowStep {
                id: "s2".into(), name: "默认边注销".into(), x: 700.0, y: 240.0,
                protocol_id: String::new(), terminal_mode: "single".into(), terminal_id: None,
                params: Default::default(), expect_protocol: None, timeout_ms: 3000, delay_ms: 0,
                next: None, on_success: None, on_failure: None, on_timeout: None,
                request_def: Some(SignalDef { header_refs: None, name: "内嵌注销".into(), fields: vec![code("信令", "36")] }),
                expect_def: None, routes: Vec::new(), ..Default::default()
            },
            simulated_terminal_lib::engine::flow::FlowStep {
                id: "s3".into(), name: "分流保活".into(), x: 360.0, y: 240.0,
                protocol_id: String::new(), terminal_mode: "single".into(), terminal_id: None,
                params: Default::default(), expect_protocol: None, timeout_ms: 3000, delay_ms: 0,
                next: None, on_success: None, on_failure: None, on_timeout: None,
                request_def: Some(SignalDef { header_refs: None, name: "内嵌保活".into(), fields: vec![code("信令", "35")] }),
                expect_def: None, routes: Vec::new(), ..Default::default()
            },
        ],
    };
    let steps_json = serde_json::to_string(&def.steps).unwrap();
    sqlx::query("INSERT INTO business_flows(name,system_ref,group_ref,steps_json) VALUES(?,?,?,?)")
        .bind(&def.name)
        .bind(t_sys)
        .bind(t_grp)
        .bind(&steps_json)
        .execute(&pool)
        .await
        .unwrap();
    def.id = sqlx::query_scalar::<_, i64>("SELECT MAX(id) FROM business_flows").fetch_one(&pool).await.unwrap();
    // 同步内嵌信令 → 协议库
    simulated_terminal_lib::engine::flow::sync_inline_signals(&pool, &mut def).await.unwrap();
    for pid in [&def.steps[0].protocol_id, def.steps[0].expect_protocol.as_deref().unwrap_or("")] {
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM protocols WHERE protocol_id=?")
            .bind(pid).fetch_one(&pool).await.unwrap();
        assert_eq!(n, 1, "内嵌信令 {pid} 应已自动入协议库");
    }
    // 运行流程; e2e 自任对端: 收到 s1 帧(信令31)回 信令32+结果00 → 路由命中走 s3
    sqlx::query("DELETE FROM logs").execute(&pool).await.unwrap();
    let peer = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let run3 = flow::start_flow(handle, pool.clone(), engine.clone(), def.id, Vec::new()).await.unwrap();
    // 等流程结束; 期间轮询日志, 出现 s1 出帧即回应答
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        if !engine.flow_runs.contains_key(&run3) { break; }
        if std::time::Instant::now() > deadline { fail("内嵌流程 30 秒未结束"); }
        let hit: Option<String> = sqlx::query_scalar(
            "SELECT raw_hex FROM logs WHERE direction='out' AND protocol_id=? ORDER BY id DESC LIMIT 1",
        )
        .bind(def.steps[0].protocol_id.as_str())
        .fetch_optional(&pool)
        .await
        .unwrap();
        if let Some(raw) = hit {
            if !raw.is_empty() {
                let compact: String = raw.chars().filter(|c| c.is_ascii_hexdigit()).collect();
                if compact == "3101" {
                    peer.send_to(&[0x32u8, 0x00], "127.0.0.1:9151").await.unwrap();
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(60)).await;
    }
    let logs = recent_logs(&pool).await;
    let sent = |pid: &str| logs.iter().any(|(_, d, p, _)| d == "out" && p == pid);
    let inline_req = def.steps[0].protocol_id.clone();
    assert!(sent(&inline_req), "内嵌注册信令未发送");
    let inline_ack = def.steps[0].expect_protocol.clone().unwrap();
    assert!(logs.iter().any(|(_, d, p, _)| d == "in" && *p == inline_ack), "内嵌应答信令未命中");
    assert!(sent(&def.steps[2].protocol_id), "内容分流(注册结果=00)应命中并走向指定步骤(保活)");
    assert!(!sent(&def.steps[1].protocol_id), "分流命中时不应走默认成功边(注销)");
    println!("[E2E] 内嵌信令+内容分流验证通过 ✅ (字段定义→自动入协议库→发帧→应答命中→按接收值命中分流)");
    println!(
        "[E2E] 全链路验证通过 ✅ PD/COMM 既有流程 + 内嵌信令定义与内容分流"
    );
}
