//! Manual real-UDP acceptance load; intentionally excluded from ordinary tests.
//! cargo run --release --example udp_load -- [--quick]
use musp_studio::{application::Studio, network::now_ms};
use serde_json::{Value, json};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::net::UdpSocket;

#[cfg(windows)]
fn memory_bytes() -> u64 {
    #[repr(C)]
    struct Counters {
        cb: u32,
        faults: u32,
        peak: usize,
        working: usize,
        quota_peak: usize,
        quota: usize,
        pool_peak: usize,
        pool: usize,
        pagefile: usize,
        pagefile_peak: usize,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
    }
    #[link(name = "psapi")]
    unsafe extern "system" {
        fn GetProcessMemoryInfo(
            handle: *mut std::ffi::c_void,
            counters: *mut Counters,
            size: u32,
        ) -> i32;
    }
    unsafe {
        let mut c: Counters = std::mem::zeroed();
        c.cb = std::mem::size_of::<Counters>() as u32;
        if GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut c,
            std::mem::size_of::<Counters>() as u32,
        ) != 0
        {
            c.working as u64
        } else {
            0
        }
    }
}
#[cfg(not(windows))]
fn memory_bytes() -> u64 {
    0
}

fn database_counts(path: &Path) -> anyhow::Result<(u64, u64)> {
    let db = rusqlite::Connection::open(path.join("db/musp.db"))?;
    db.busy_timeout(Duration::from_secs(2))?;
    Ok(db.query_row(
        "SELECT COALESCE(MAX(id),0),COUNT(*) FROM packets",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?)
}

fn database_size(path: &Path) -> Value {
    json!({"mainBytes":std::fs::metadata(path.join("db/musp.db")).map(|m|m.len()).unwrap_or(0),"walBytes":std::fs::metadata(path.join("db/musp.db-wal")).map(|m|m.len()).unwrap_or(0)})
}

async fn stage(
    app: &Arc<Studio>,
    sender: &UdpSocket,
    remote: &str,
    bytes: &[u8],
    rate: u64,
    seconds: u64,
    dir: &Path,
) -> anyhow::Result<Value> {
    let before = app.udp.metrics();
    let (before_processed, _) = database_counts(dir)?;
    let before_errors = app
        .processing_errors
        .load(std::sync::atomic::Ordering::Relaxed);
    let started = Instant::now();
    let mut sent = 0u64;
    let mut send_errors = 0u64;
    let mut tick = tokio::time::interval(Duration::from_millis(10));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut next_progress = 30u64;
    let mut memory = vec![json!({"elapsedSeconds":0,"workingSetBytes":memory_bytes()})];
    while started.elapsed() < Duration::from_secs(seconds) {
        tick.tick().await;
        let target =
            ((started.elapsed().as_secs_f64() * rate as f64).floor() as u64).min(rate * seconds);
        while sent + send_errors < target {
            match sender.send_to(bytes, remote).await {
                Ok(n) if n == bytes.len() => sent += 1,
                _ => send_errors += 1,
            }
        }
        let elapsed = started.elapsed().as_secs();
        if elapsed >= next_progress {
            let m = app.udp.metrics();
            let sample = json!({"elapsedSeconds":elapsed,"workingSetBytes":memory_bytes()});
            memory.push(sample.clone());
            println!(
                "{}",
                json!({"progress":true,"targetRate":rate,"sent":sent,"kernelRx":m.rx-before.rx,"queueDropped":m.queue_dropped-before.queue_dropped,"memory":sample})
            );
            next_progress += 30;
        }
    }
    let send_elapsed = started.elapsed().as_secs_f64();
    let drain = Instant::now();
    loop {
        let m = app.udp.metrics();
        let (processed, _) = database_counts(dir)?;
        if processed - before_processed
            >= m.rx - before.rx - (m.queue_dropped - before.queue_dropped)
            && drain.elapsed() > Duration::from_millis(200)
        {
            break;
        }
        if drain.elapsed() > Duration::from_secs(60) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let after = app.udp.metrics();
    let (processed, retained) = database_counts(dir)?;
    let snapshot = app
        .request(json!({"action":"snapshot"}))
        .await
        .map_err(anyhow::Error::msg)?;
    memory.push(
        json!({"elapsedSeconds":started.elapsed().as_secs_f64(),"workingSetBytes":memory_bytes()}),
    );
    let result = json!({"targetDatagramsPerSecond":rate,"requestedDurationSeconds":seconds,"datagramBytes":bytes.len(),"sent":sent,"sendErrors":send_errors,"sendElapsedSeconds":send_elapsed,"actualSendRate":sent as f64/send_elapsed,"kernelRx":after.rx-before.rx,"kernelNotReceived":sent.saturating_sub(after.rx-before.rx),"queueDropped":after.queue_dropped-before.queue_dropped,"socketErrors":after.socket_errors-before.socket_errors,"processedAndPersisted":processed-before_processed,"processingErrors":app.processing_errors.load(std::sync::atomic::Ordering::Relaxed)-before_errors,"retainedSqlitePackets":retained,"snapshotProcessedPackets":snapshot.get("processedPackets"),"drainElapsedSeconds":drain.elapsed().as_secs_f64(),"memory":memory,"sqliteStorage":database_size(dir)});
    println!("{}", result);
    Ok(result)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let quick = std::env::args().any(|v| v == "--quick");
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../test-results");
    std::fs::create_dir_all(&root)?;
    let dir = root.join(format!("udp-load-{}", now_ms()));
    let (app, rx) = Studio::open(&dir).map_err(anyhow::Error::msg)?;
    app.start_receiver(rx);
    app.start_scheduler();
    let protocol = app
        .db
        .protocols(&app.project())
        .map_err(anyhow::Error::msg)?
        .into_iter()
        .find(|p| p.published)
        .ok_or_else(|| anyhow::anyhow!("published schema missing"))?
        .id;
    let bytes=app.schema(&protocol).map_err(anyhow::Error::msg)?.encode(0x32,&json!({"srcAddr":1,"dstAddr":59,"sessionId":4660,"transactionId":77,"resultCode":0,"serverTimeSeconds":0}))?;
    anyhow::ensure!(bytes.len() <= 512, "load packet exceeds acceptance size");
    let endpoint=app.request(json!({"action":"endpoint_start","name":"UDP acceptance receiver","bind":"127.0.0.1:0","protocolId":protocol})).await.map_err(anyhow::Error::msg)?;
    let sender = UdpSocket::bind("127.0.0.1:0").await?;
    println!(
        "{}",
        json!({"dataDirectory":dir,"profile":if cfg!(debug_assertions){"debug"}else{"release"},"quick":quick,"receiver":endpoint["bind"],"sender":sender.local_addr()?.to_string()})
    );
    let mut results = vec![];
    for (rate, seconds) in if quick {
        [(1000, 3), (5000, 2)]
    } else {
        [(1000, 300), (5000, 60)]
    } {
        results.push(
            stage(
                &app,
                &sender,
                endpoint["bind"].as_str().unwrap(),
                &bytes,
                rate,
                seconds,
                &dir,
            )
            .await?,
        );
    }
    app.shutdown.cancel();
    app.udp.stop_all().await;
    let result = json!({"profile":if cfg!(debug_assertions){"debug"}else{"release"},"quick":quick,"dataDirectory":dir,"transport":"real UDP loopback, independent sender socket","headless":true,"stages":results});
    let output = dir.join("results.json");
    std::fs::write(&output, serde_json::to_vec_pretty(&result)?)?;
    println!("results={}", output.display());
    Ok(())
}
