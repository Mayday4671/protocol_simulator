//! 智能配电接入协议(PD_1.0)全链路 E2E: mock app + 真实引擎 + 本机 py 平台服务端(4002)。
//!
//! 前置: `python scripts/pd_server_gui.py 4002` 且已导入 protocols/PD_1.0.xml、种子通道 9101 与 3 台设备
//! 运行: `cargo run --features e2e --bin e2e_pd`
//! 覆盖(两终端共用一个通道, 仅靠「源地址」区分):
//!   保存自检 → A/B 各自开机注册 → 注册许可从头内令牌位捕获 2B 令牌(存设备档案+按终端分桶)
//!   → 心跳/参数设置/参数查询(变长列表)/状态/告警/多终端清单上报, 逐帧断言头内令牌与源地址
//!   → 伪造令牌被平台拒绝(令牌无效) → 注销。
//! 数据库用真实库快照, 不污染现场数据。
#![cfg(feature = "e2e")]

use simulated_terminal_lib::engine::state::EngineState;
use simulated_terminal_lib::engine::udp;
use simulated_terminal_lib::{db, AppState};
use sqlx::sqlite::SqlitePoolOptions;
use std::collections::HashMap;
use std::sync::Arc;

type Row = (i64, String, String, Option<String>, Option<String>);

async fn recent_logs(pool: &sqlx::SqlitePool) -> Vec<Row> {
    sqlx::query_as::<_, Row>(
        "SELECT id, direction, protocol_id, parsed, raw_hex FROM logs ORDER BY id",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn wait_log(pool: &sqlx::SqlitePool, base: i64, dir: &str, protocol: &str) -> Row {
    wait_frame(pool, base, dir, protocol, &|_| true, "").await
}

/// 按方向+协议+字节谓词等待一帧(自动心跳等同类帧会交错, 必须按源/目标地址精确匹配)
async fn wait_frame(
    pool: &sqlx::SqlitePool,
    base: i64,
    dir: &str,
    protocol: &str,
    pred: &(dyn Fn(&Row) -> bool + Send),
    what: &str,
) -> Row {
    for _ in 0..240 {
        let hit = recent_logs(pool)
            .await
            .into_iter()
            .find(|r| r.0 > base && r.1 == dir && r.2 == protocol && pred(&r));
        if let Some(row) = hit {
            return row;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    fail(&format!(
        "等待 {dir} {protocol} {what} 超时, 最近日志: {:?}",
        recent_logs(pool)
            .await
            .iter()
            .rev()
            .take(8)
            .rev()
            .map(|r| format!("{}:{}:{}", r.1, r.2, r.3.as_deref().unwrap_or("")))
            .collect::<Vec<_>>()
    ));
}

fn fail(msg: &str) -> ! {
    println!("[E2E-FAIL] {msg}");
    std::process::exit(1);
}

/// 紧凑 hex 的头内切片: [byte_lo, byte_hi) 区间 → 大写 hex(如令牌 12..14)
fn slice_hex(raw: &str, lo: usize, hi: usize) -> String {
    let compact: String = raw.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    compact
        .get(lo * 2..hi * 2)
        .unwrap_or_default()
        .to_ascii_uppercase()
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
        Err(e) => fail(&format!("4002 端口探测异常: {e}")),
    }

    let appdata = std::env::var("APPDATA").unwrap();
    let src = format!("{appdata}\\com.simterm.desktop\\simterm.db");
    let dst = std::env::temp_dir().join(format!("simterm_e2e_pd_{}.db", std::process::id()));
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
    let state = AppState {
        engine: Arc::new(EngineState::new()),
        pool: pool.clone(),
    };
    let engine = &state.engine;

    let node = db::list_nodes(&pool)
        .await
        .unwrap()
        .into_iter()
        .find(|n| n.local_port == 9101)
        .unwrap_or_else(|| fail("库中缺少本地 9101 节点(配电模拟通道)"));
    let protocols = db::list_protocols(&pool).await.unwrap();
    let proto = |pid: &str| {
        protocols
            .iter()
            .find(|p| p.protocol_id == pid)
            .unwrap_or_else(|| fail(&format!("库中缺少协议 {pid}(先导入 PD_1.0.xml)")))
            .clone()
    };

    // ---- ⓪ 界面保存同路径自检 ----
    let errs = simulated_terminal_lib::e2e_validate_all_protocols(&pool).await;
    if errs.is_empty() {
        println!("[E2E] 保存自检: 全部协议打开-保存无阻碍 ✓");
    } else {
        for e in &errs {
            println!("[E2E][保存自检失败] {e}");
        }
        fail("存在会被保存校验拦截的协议(见上)");
    }

    // 终端 id: 按设备档案标识查
    async fn terminal(pool: &sqlx::SqlitePool, node_id: i64, system_ref: i64, key: &str) -> i64 {
        sqlx::query_scalar::<_, i64>(
            "SELECT id FROM terminal_profiles WHERE node_ref=? AND system_ref=? AND device_key=?",
        )
        .bind(node_id)
        .bind(system_ref)
        .bind(key)
        .fetch_one(pool)
        .await
        .unwrap()
    }
    async fn session_of(pool: &sqlx::SqlitePool, tid: i64) -> Option<String> {
        sqlx::query_scalar::<_, Option<String>>("SELECT session_hex FROM terminal_profiles WHERE id=?")
            .bind(tid)
            .fetch_one(pool)
            .await
            .unwrap()
    }
    let term_a = terminal(&pool, node.id, proto("pd_reg").system_ref.unwrap_or(-1), "00000001").await;
    let term_b = terminal(&pool, node.id, proto("pd_reg").system_ref.unwrap_or(-1), "00000002").await;

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
    let mut m = |pairs: &[(&str, &str)]| {
        let mut map: HashMap<String, String> = HashMap::new();
        for (k, v) in pairs {
            map.insert((*k).to_string(), (*v).to_string());
        }
        map
    };

    let mut base = recent_logs(&pool).await.last().map(|r| r.0).unwrap_or(0);

    // ---- ① A/B 各自开机注册: 注册许可从头内令牌位捕获 2B 令牌 ----
    send!("pd_reg", m(&[("__terminal_id", &term_a.to_string())]));
    let grant_a = wait_frame(&pool, base, "in", "pd_reg_grant", &|r| slice_hex(r.4.as_deref().unwrap_or(""), 6, 10) == "00000001", "dst=A").await;
    base = grant_a.0;
    let token_a = session_of(&pool, term_a).await.unwrap_or_else(|| fail("A 注册后设备档案 session_hex 未写入"));
    assert_ne!(token_a, "0000", "A 的令牌不应是全 0");
    assert_eq!(
        slice_hex(grant_a.4.as_deref().unwrap_or(""), 12, 14),
        token_a,
        "A 令牌应来自头内令牌位"
    );
    send!("pd_reg", m(&[("__terminal_id", &term_b.to_string())]));
    let grant_b = wait_frame(&pool, base, "in", "pd_reg_grant", &|r| slice_hex(r.4.as_deref().unwrap_or(""), 6, 10) == "00000002", "dst=B").await;
    base = grant_b.0;
    let token_b = session_of(&pool, term_b).await.unwrap_or_else(|| fail("B 注册后设备档案 session_hex 未写入"));
    assert_ne!(token_b, token_a, "A/B 令牌必须独立分桶");
    // 注册许可触发自动定时心跳(每终端一条)
    assert!(
        engine.periodic.iter().any(|r| r.value().0.protocol_id == "pd_heartbeat"),
        "注册许可收到后应自动启动心跳定时任务"
    );
    println!("[E2E] ① A/B 注册完成: 令牌 A={token_a} B={token_b}, 自动心跳已启动 ✓");

    // ---- ② 后续所有帧头自动携带本终端令牌(逐帧断言 头@12..14 + 源地址@2..6) ----
    for (pid, resp_id, tid, token, src, body0) in [
        ("pd_heartbeat", "pd_heartbeat_resp", term_a, &token_a, "00000001", "00"),
        ("pd_param_set", "pd_param_set_resp", term_a, &token_a, "00000001", "00"),
        ("pd_param_query", "pd_param_query_resp", term_a, &token_a, "00000001", "03"),
        ("pd_status_report", "pd_report_ack", term_b, &token_b, "00000002", "00"),
        ("pd_alarm_report", "pd_alarm_ack", term_a, &token_a, "00000001", "00"),
    ] {
        let params = m(&[("__terminal_id", &tid.to_string())]);
        send!(pid, params);
        // 自动心跳与手工帧交错: 按源地址认帧
        let out = wait_frame(&pool, base, "out", pid, &|r| slice_hex(r.4.as_deref().unwrap_or(""), 2, 6) == src, &format!("src={src}")).await;
        base = out.0;
        let raw = out.4.as_deref().unwrap_or("");
        assert_eq!(slice_hex(raw, 12, 14), *token, "{pid} 头内令牌应为 {token}");
        assert_eq!(slice_hex(raw, 2, 6), src, "{pid} 源地址应为 {src}");
        assert_eq!(slice_hex(raw, 6, 10), "00000000", "{pid} 目标地址应为平台");
        let row = wait_frame(&pool, base, "in", resp_id, &|r| slice_hex(r.4.as_deref().unwrap_or(""), 6, 10) == src && slice_hex(r.4.as_deref().unwrap_or(""), 16, 17) == body0, &format!("dst={src} 体首字节={body0}")).await;
        base = row.0;
    }
    println!("[E2E] ② 心跳/参数设置/参数查询/状态(B)/告警 逐帧头内令牌断言 + 平台认证应答 ✓");

    // ---- ③ 多终端清单上报: 条目数 × (终端地址+运行状态) + 尾随校验和 ----
    send!("pd_dev_list_report", m(&[
        ("__terminal_id", &term_a.to_string()),
        ("终端个数", "02"),
        ("终端列表", "000000010000000201"),
    ]));
    let out = wait_frame(&pool, base, "out", "pd_dev_list_report", &|r| slice_hex(r.4.as_deref().unwrap_or(""), 2, 6) == "00000001", "src=A").await;
    base = out.0;
    let raw = out.4.as_deref().unwrap_or("");
    assert_eq!(slice_hex(raw, 12, 14), token_a);
    assert_eq!(slice_hex(raw, 16, 17), "02", "清单条目数应为 2");
    let ack = wait_frame(&pool, base, "in", "pd_list_ack", &|r| slice_hex(r.4.as_deref().unwrap_or(""), 6, 10) == "00000001" && slice_hex(r.4.as_deref().unwrap_or(""), 16, 17) == "00", "dst=A 成功").await;
    base = ack.0;
    println!("[E2E] ③ 设备清单上报(条目数联动变长列表) 平台确认 ✓");

    // ---- ④ 注销 A(令牌核销) ----
    send!("pd_logout", m(&[("__terminal_id", &term_a.to_string())]));
    let out = wait_frame(&pool, base, "out", "pd_logout", &|r| slice_hex(r.4.as_deref().unwrap_or(""), 2, 6) == "00000001", "src=A").await;
    base = out.0;
    assert_eq!(slice_hex(out.4.as_deref().unwrap_or(""), 12, 14), token_a, "注销帧仍携带当前令牌");
    let lr = wait_frame(&pool, base, "in", "pd_logout_resp", &|r| slice_hex(r.4.as_deref().unwrap_or(""), 6, 10) == "00000001" && slice_hex(r.4.as_deref().unwrap_or(""), 16, 17) == "00", "dst=A 成功").await;
    base = lr.0;
    println!("[E2E] ④ 注销 A 完成(帧仍携带当前令牌) ✓");

    // ---- ⑤ 负例: 令牌已吊销后心跳 → 平台拒绝(未注册 03) ----
    send!("pd_heartbeat", m(&[("__terminal_id", &term_a.to_string())]));
    let out = wait_frame(&pool, base, "out", "pd_heartbeat", &|r| slice_hex(r.4.as_deref().unwrap_or(""), 2, 6) == "00000001" && slice_hex(r.4.as_deref().unwrap_or(""), 12, 14) == token_a, "src=A 带已吊销令牌").await;
    base = out.0;
    let rej = wait_frame(&pool, base, "in", "pd_heartbeat_resp", &|r| slice_hex(r.4.as_deref().unwrap_or(""), 6, 10) == "00000001" && slice_hex(r.4.as_deref().unwrap_or(""), 16, 17) == "03", "dst=A 未注册").await;
    base = rej.0;
    println!("[E2E] ⑤ 吊销令牌后心跳被平台拒绝(未注册) ✓");

    println!("[E2E] 全链路验证通过 ✅ (注册令牌捕获→档案存储→全帧头携带→平台认证→清单/查询变长→令牌拒绝→注销)");
}
