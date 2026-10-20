//! 设备通信协议(COMM_1.0)全链路 E2E: mock app + 真实引擎 + 本机 py 测试服务端(4001)。
//!
//! 前置: `python scripts/udcp_server_gui.py 4001 --auto` 且已导入 protocols/COMM_1.0.xml
//! 运行: `cargo run --features e2e --bin e2e_comm`
//! 覆盖(两终端共用一个通道, 仅靠「源设备编号」区分):
//!   终端A配置→捕获服务器IP → A/B各自注册→各自安全信息分桶
//!   → A主叫B → 服务端给B推来电 → B自动应答接收(携带B自己的安全信息, 服务端校验)
//!   → A收到呼叫确认(捕获呼叫结果)。
//! 数据库用真实库快照, 不污染现场数据。
#![cfg(feature = "e2e")]

use simulated_terminal_lib::engine::state::EngineState;
use simulated_terminal_lib::engine::udp;
use simulated_terminal_lib::{db, AppState};
use sqlx::sqlite::SqlitePoolOptions;
use std::collections::HashMap;
use std::sync::Arc;

type Row = (i64, String, String, Option<String>);

async fn recent_logs(pool: &sqlx::SqlitePool) -> Vec<Row> {
    sqlx::query_as::<_, Row>("SELECT id, direction, protocol_id, parsed FROM logs ORDER BY id")
        .fetch_all(pool)
        .await
        .unwrap()
}

/// 等待 base 之后出现指定方向+协议(可再按解析摘要包含内容区分)的日志;
/// 固定睡眠在本机高负载下会漏收包, 轮询才稳
async fn wait_log(pool: &sqlx::SqlitePool, base: i64, dir: &str, protocol: &str, contains: &str, what: &str) -> i64 {
    for _ in 0..240 {
        let hit = recent_logs(pool)
            .await
            .into_iter()
            .find(|(id, d, p, parsed)| {
                *id > base && d == dir && p == protocol
                    && (contains.is_empty() || parsed.as_deref().unwrap_or("").contains(contains))
            });
        if let Some((id, _, _, _)) = hit {
            return id;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    fail(&format!("等待 {what}({dir} {protocol} 含[{contains}])超时, 最近日志: {:?}",
        recent_logs(pool).await.iter().rev().take(8).rev().map(|r| format!("{}:{}:{}", r.1, r.2, r.3.as_deref().unwrap_or(""))).collect::<Vec<_>>()));
}

fn fail(msg: &str) -> ! {
    println!("[E2E-FAIL] {msg}");
    std::process::exit(1);
}

/// 服务端存活预检: 探测 4001 是否已被监听（无副作用）
async fn assert_server_up() {
    match tokio::net::UdpSocket::bind("0.0.0.0:4001").await {
        Ok(sock) => {
            drop(sock);
            fail("本机 4001 无 py 测试服务端, 先运行: python scripts/udcp_server_gui.py 4001 --auto");
        }
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            println!("[E2E] py 服务端在线(4001 已监听)")
        }
        Err(e) => fail(&format!("4001 端口探测异常: {e}")),
    }
}

#[tokio::main]
async fn main() {
    assert_server_up().await;

    // 真实库快照 → 临时库
    let appdata = std::env::var("APPDATA").unwrap();
    let src = format!("{appdata}\\com.simterm.desktop\\simterm.db");
    let dst = std::env::temp_dir().join(format!("simterm_e2e_{}.db", std::process::id()));
    let dst_str = dst.to_str().unwrap().to_string();
    let _ = std::fs::remove_file(&dst);
    {
        let opts = sqlx::sqlite::SqliteConnectOptions::new().filename(&src).read_only(true);
        let src_pool = SqlitePoolOptions::new().max_connections(1).connect_with(opts).await.unwrap();
        sqlx::query(&format!("VACUUM INTO '{}'", dst_str.replace('\'', "''")))
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
    let state = AppState {
        engine: Arc::new(EngineState::new()),
        pool: pool.clone(),
    };
    let engine = &state.engine;

    let node = db::list_nodes(&pool)
        .await
        .unwrap()
        .into_iter()
        .find(|n| n.local_port == 9001)
        .unwrap_or_else(|| fail("库中缺少本地 9001 节点"));
    let protocols = db::list_protocols(&pool).await.unwrap();
    let proto = |pid: &str| {
        protocols
            .iter()
            .find(|p| p.protocol_id == pid)
            .unwrap_or_else(|| fail(&format!("库中缺少协议 {pid}(先导入 COMM_1.0.xml)")))
            .clone()
    };
    let mut params = |fields: &[(&str, &str)]| {
        let mut m: HashMap<String, String> = HashMap::new();
        for (k, v) in fields {
            m.insert((*k).to_string(), (*v).to_string());
        }
        m
    };

    // 发送并等待链路完成, 返回本步新增日志
    let mut step_base = recent_logs(&pool).await.len() as i64;
    let send = |p_name: &str, m: HashMap<String, String>| {
        let handle = handle.clone();
        let pool = pool.clone();
        let engine = state.engine.clone();
        let node = node.clone();
        let pid = proto_id_of(p_name);
        async move {
            udp::send_datagram(&handle, &pool, &engine, &node, &proto(&pid), &m, None)
                .await
                .unwrap();
        }
    };
    // 上面闭包借用 proto 不便, 直接用辅助函数式展开
    macro_rules! send {
        ($pid:expr, $m:expr) => {{
            let p = proto($pid);
            // 与控制台命令同口径: 每次发送前重取节点(拿到最新捕获桶/会话)
            let node = db::get_node(&pool, node.id).await.unwrap();
            udp::send_datagram(&handle, &pool, engine, &node, &p, &$m, None)
                .await
                .unwrap();
        }};
    }
    let _ = &mut step_base;
    let _ = &mut params;
    let _ = send;

    // ---- ⓪ 界面保存同路径自检: 全部协议空手打开-保存是否会被拦 ----
    let errs = simulated_terminal_lib::e2e_validate_all_protocols(&pool).await;
    if errs.is_empty() {
        println!("[E2E] 保存自检: 全部协议打开-保存无阻碍 ✓");
    } else {
        for e in &errs {
            println!("[E2E][保存自检失败] {e}");
        }
        fail("存在会被保存校验拦截的协议(见上)");
    }

    // ---- ① 终端A: 配置请求 → 配置数据(捕获服务器IP) → 自动配置应答 ----
    let step_base = recent_logs(&pool).await.last().map(|r| r.0).unwrap_or(0);
    send!("dev_cfg_req", params(&[("源设备编号", "000001")]));
    wait_log(&pool, step_base, "in", "dev_cfg_data", "", "配置数据").await;
    wait_log(&pool, step_base, "out", "dev_cfg_ack", "", "配置应答").await;
    let logs = recent_logs(&pool).await;
    let seq: Vec<String> = logs.iter().map(|r| format!("{}:{}", r.1, r.2)).collect();
    assert_tail(
        &seq,
        &["out:dev_cfg_req", "in:dev_cfg_data", "out:dev_cfg_ack"],
        "配置链",
    );

    // ---- ② 双终端注册: A、B 各自拿到安全信息(按源设备编号分桶) ----
    let reg_base = recent_logs(&pool).await.last().map(|r| r.0).unwrap_or(0);
    send!("dev_reg", params(&[("源设备编号", "000001")]));
    wait_log(&pool, reg_base, "in", "dev_reg_resp", "00 00 01", "终端A注册响应").await;
    send!("dev_reg", params(&[("源设备编号", "000002")]));
    wait_log(&pool, reg_base, "in", "dev_reg_resp", "00 00 02", "终端B注册响应").await;
    let logs = recent_logs(&pool).await;
    let reg_count = logs
        .iter()
        .filter(|(id, d, p, _)| *id > reg_base && d == "in" && p == "dev_reg_resp")
        .count();
    if reg_count != 2 {
        fail(&format!("两次注册应有两条注册响应命中, 实际 {reg_count}"));
    }
    // 注册响应命中即为各终端启动独立定时心跳(多终端单通道各自保活)
    let triggered: Vec<String> = logs
        .iter()
        .filter(|(id, d, p, parsed)| {
            *id > reg_base && d == "in" && p == "dev_reg_resp"
                && parsed.as_deref().unwrap_or("").contains("已启动定时发送「心跳」")
        })
        .map(|r| r.3.clone().unwrap_or_default())
        .collect();
    if triggered.len() != 2
        || !triggered.iter().any(|t| t.contains("目标设备编号=00 00 01"))
        || !triggered.iter().any(|t| t.contains("目标设备编号=00 00 02"))
    {
        fail(&format!(
            "两次注册应为 A/B 各启动一条定时心跳, 实际 {} 条: {:?}",
            triggered.len(),
            triggered
        ));
    }
    // 同一终端重复注册不重复启动
    let re_reg_base = recent_logs(&pool).await.last().map(|r| r.0).unwrap_or(0);
    send!("dev_reg", params(&[("源设备编号", "000001")]));
    wait_log(&pool, re_reg_base, "in", "dev_reg_resp", "00 00 01", "终端A重复注册响应").await;
    let dup_started = recent_logs(&pool)
        .await
        .iter()
        .filter(|(id, d, p, parsed)| {
            *id > re_reg_base
                && d == "in"
                && p == "dev_reg_resp"
                && parsed.as_deref().unwrap_or("").contains("已启动定时发送「心跳」")
        })
        .count();
    if dup_started != 0 {
        fail(&format!("同一终端重复注册不应重复启动定时心跳, 实际新增 {dup_started} 条"));
    }

    // ---- ③ A主叫B → 来电(推给B) → B自动应答(带B的安全信息, 服务端校验) → A收确认 ----
    let call_base = recent_logs(&pool).await.last().map(|r| r.0).unwrap_or(0);
    send!("dev_call", params(&[("源设备编号", "000001"), ("被叫设备", "000002")]));
    wait_log(&pool, call_base, "in", "dev_incoming", "", "来电").await;
    wait_log(&pool, call_base, "out", "dev_call_ack", "", "呼叫应答").await;
    wait_log(&pool, call_base, "in", "dev_call_cfm", "", "呼叫确认").await;
    let logs: Vec<Row> = recent_logs(&pool)
        .await
        .into_iter()
        .filter(|r| r.0 > call_base)
        .collect();
    let seq: Vec<String> = logs.iter().map(|r| format!("{}:{}", r.1, r.2)).collect();
    assert_tail(
        &seq,
        &["out:dev_call", "in:dev_incoming", "out:dev_call_ack", "in:dev_call_cfm"],
        "呼叫链",
    );

    // 呼叫应答必须携带 B 的安全信息(服务端校验通过才会回确认, 这里再显式验帧)
    let ack = logs
        .iter()
        .rev()
        .find(|(_, d, p, _)| d == "out" && p == "dev_call_ack")
        .unwrap_or_else(|| fail("缺少呼叫应答"));
    let ack_hex: String = sqlx::query_scalar::<_, String>(
        "SELECT replace(raw_hex, ' ', '') FROM logs WHERE id=?",
    )
    .bind(ack.0)
    .fetch_one(&pool)
    .await
    .unwrap();
    let _ = &ack_hex;

    // ---- ④ 捕获桶: A=服务器IP+会话+呼叫结果; B=会话 ----
    let node_row = db::get_node(&pool, node.id).await.unwrap();
    let caps = node_row.capture_buckets();
    // 捕获桶按 s{系统}:终端 归档(多系统同终端不串桶)
    let sys: i64 = sqlx::query_scalar("SELECT system_ref FROM protocols WHERE protocol_id='dev_reg_resp'")
        .fetch_one(&pool).await.unwrap();
    let a = caps.get(&format!("s{sys}:000001")).unwrap_or_else(|| fail("终端A(000001)无捕获桶"));
    if a.get("服务器IP").map(|v| v.as_str()) != Some("C0A80001") {
        fail(&format!("A 桶服务器IP 应为 C0A80001, 实际 {:?}", a.get("服务器IP")));
    }
    let a_sec = a.get("__session").cloned().unwrap_or_default();
    if a_sec.len() != 8 || a_sec.chars().all(|c| c == '0') {
        fail(&format!("A 桶安全信息应为服务端分配的非零值, 实际 {a_sec:?}"));
    }
    if a.get("呼叫结果").map(|v| v.as_str()) != Some("01") {
        fail(&format!("A 桶呼叫结果应为 01(接收), 实际 {:?}", a.get("呼叫结果")));
    }
    let b = caps.get(&format!("s{sys}:000002")).unwrap_or_else(|| fail("终端B(000002)无捕获桶"));
    let b_sec = b.get("__session").cloned().unwrap_or_default();
    if b_sec.len() != 8 || b_sec.chars().all(|c| c == '0') {
        fail(&format!("B 桶安全信息应为服务端分配的非零值, 实际 {b_sec:?}"));
    }
    if a_sec == b_sec {
        fail(&format!("A/B 安全信息必须不同(分桶隔离), 实际均为 {a_sec}"));
    }
    // 呼叫应答帧携带的正是 B 的安全信息(服务端校验通过才回了确认)
    if !ack_hex.contains(&format!("08{b_sec}01")) {
        fail(&format!("呼叫应答应带 B 的安全信息 {b_sec} + 接收(01), 帧: {ack_hex}"));
    }

    // ---- ⑤ 复杂结构真实引擎验证: 分组公共头条件分支 + 条目数列表 + 收包分支解析 ----
    // 测试数据(只进临时库): 新分组, 公共头=判别码1B(信令类型), 两条协议
    let sys_id: i64 = sqlx::query_scalar("SELECT id FROM protocol_systems LIMIT 1")
        .fetch_one(&pool).await.unwrap();
    let header_json = r#"[{"name":"判别码","len":1,"enc":"hex","value":"01"}]"#;
    let gid: i64 = sqlx::query_scalar(
        "INSERT INTO protocol_groups (system_ref, name, header_enabled, header_fields_json) VALUES (?1, '分支条目测试', 1, ?2) RETURNING id",
    )
    .bind(sys_id).bind(header_json)
    .fetch_one(&pool).await.unwrap();
    let branch_fields = r#"[
        {"name":"判别码","len":1,"enc":"hex","value":"11","role":"type_code","hdr_ref":"判别码"},
        {"name":"分支A","len":2,"enc":"hex","value":"AA BB","when":{"field":"判别码","value":"11"}},
        {"name":"分支B","len":4,"enc":"hex","value":"11 22 33 44","when":{"field":"判别码","value":"12"}},
        {"name":"尾部","len":1,"enc":"hex","value":"FF"}
    ]"#;
    let list_fields = r#"[
        {"name":"判别码","len":1,"enc":"hex","value":"13","role":"type_code","hdr_ref":"判别码"},
        {"name":"条目数","len":1,"enc":"hex","value":"02","role":"device_count"},
        {"name":"设备列表","len":2,"enc":"hex","value":"{vlen}","role":"device_list","unit_fields":[
            {"name":"编号","len":1,"enc":"hex","value":""},{"name":"状态","len":1,"enc":"hex","value":"01"}]},
        {"name":"校验","len":2,"enc":"hex","value":""}
    ]"#;
    for (pid, name, fields) in [
        ("t_branch", "分支上报", branch_fields),
        ("t_list", "条目上报", list_fields),
    ] {
        sqlx::query(
            "INSERT INTO protocols (protocol_id, name, format, msg_type, def_mode, fields_json, payload_template, system_ref, group_ref)
             VALUES (?1, ?2, 'hex', 'Request', 'fields', ?3, '', ?4, ?5)",
        )
        .bind(pid).bind(name).bind(fields).bind(sys_id).bind(gid)
        .execute(&pool).await.unwrap();
    }
    let t_branch = db::get_protocol(&pool, "t_branch").await.unwrap();
    let t_list = db::get_protocol(&pool, "t_list").await.unwrap();

    // 分支: 条件字段在公共头(判别码), 分支字段在消息体 —— 跨头/体条件
    let base5 = recent_logs(&pool).await.last().map(|r| r.0).unwrap_or(0);
    udp::send_datagram(&handle, &pool, engine, &node, &t_branch,
        &params(&[("判别码", "12")]), None).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    let hx: String = sqlx::query_scalar("SELECT replace(raw_hex,' ','') FROM logs WHERE id>? AND direction='out' AND protocol_id='t_branch' ORDER BY id LIMIT 1")
        .bind(base5).fetch_one(&pool).await.unwrap();
    if hx != "021122334" .to_string().replace('1', "1") + "" { /* 占位防误 */ }
    let expect_b = "12" .to_string() + "11223344" + "FF";
    if hx != expect_b {
        fail(&format!("分支02帧应为 {expect_b}(判别码+分支B+尾部), 实际 {hx}"));
    }
    udp::send_datagram(&handle, &pool, engine, &node, &t_branch,
        &params(&[("判别码", "11")]), None).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    let hx2: String = sqlx::query_scalar("SELECT replace(raw_hex,' ','') FROM logs WHERE direction='out' AND protocol_id='t_branch' ORDER BY id DESC LIMIT 1")
        .fetch_one(&pool).await.unwrap();
    let expect_a = "11" .to_string() + "AABB" + "FF";
    if hx2 != expect_a {
        fail(&format!("分支01帧应为 {expect_a}(判别码+分支A+尾部), 实际 {hx2}"));
    }

    // 条目数: 头(判别码03 覆盖) + 条目数02 + 列表2台×2B + 校验补零
    let mut lp: HashMap<String, String> = params(&[("判别码", "13"), ("设备列表", "AA 01 BB 02")]);
    lp.insert("条目数".into(), "02".into());
    udp::send_datagram(&handle, &pool, engine, &node, &t_list, &lp, None).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    let hx3: String = sqlx::query_scalar("SELECT replace(raw_hex,' ','') FROM logs WHERE direction='out' AND protocol_id='t_list' ORDER BY id DESC LIMIT 1")
        .fetch_one(&pool).await.unwrap();
    let expect_l = "13" .to_string() + "02" + "AA01BB02" + "0000";
    if hx3 != expect_l {
        fail(&format!("条目数帧应为 {expect_l}(判别码+02+两台×2B+校验0000), 实际 {hx3}"));
    }

    // 收包分支解析: 探测 socket 模拟服务端下发 判别码01+分支A+尾部 → 命中"分支上报"且只解析分支A
    let probe = tokio::net::UdpSocket::bind("0.0.0.0:0").await.unwrap();
    let base6 = recent_logs(&pool).await.last().map(|r| r.0).unwrap_or(0);
    probe.send_to(&[0x11, 0xAA, 0xBB, 0xFF], ("127.0.0.1", 9001)).await.unwrap();
    // 真库协议多, 未命中包要逐协议匹配, 固定睡眠不够 — 轮询等待命中日志
    let hit_id = wait_log(&pool, base6, "in", "t_branch", "", "分支上报命中").await;
    let parsed: String = sqlx::query_scalar("SELECT parsed FROM logs WHERE id=?").bind(hit_id).fetch_one(&pool).await.unwrap();
    if !(parsed.contains("命中 分支上报") && parsed.contains("分支A") && !parsed.contains("分支B")) {
        fail(&format!("收包分支解析异常: {parsed}"));
    }


    udp::stop_server(&handle, engine, node.id);
    pool.close().await;
    let _ = std::fs::remove_file(&dst);
    println!("[E2E] 全链路验证通过 ✅ (配置→双注册分桶安全信息→A主叫→B应答接收→确认)");
}

fn proto_id_of(name: &str) -> String {
    name.to_string()
}

/// 断言日志序列以 expected 结尾
fn assert_tail(seq: &[String], expected: &[&str], what: &str) {
    let tail: Vec<&str> = seq
        .iter()
        .rev()
        .take(expected.len())
        .rev()
        .map(|s| s.as_str())
        .collect();
    if tail != expected {
        fail(&format!(
            "{what}序列不符:\n  实际(末{}) {:?}\n  预期 {:?}",
            expected.len(),
            tail,
            expected
        ));
    }
}
