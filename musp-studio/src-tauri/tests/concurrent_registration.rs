use musp_studio::{application::Studio, domain::DeviceState, runtime::FlowStatus};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{sync::Barrier, task::JoinSet};

#[derive(Clone)]
struct Group {
    system: String,
    local_id: String,
    local_bind: String,
    controller_id: String,
    controller_bind: String,
    devices: Vec<String>,
}

async fn request(app: &Arc<Studio>, value: Value) -> Value {
    app.request(value).await.expect("application request")
}

fn unmatched_test_responses(app: &Studio, endpoint: &str) -> usize {
    // 65000 (FD E8) is reserved for the deliberately unrelated test responses.
    let packets = app.db.packets(&app.project(), 0, 500, "FD E8").unwrap();
    packets["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| {
            p["direction"] == "RX"
                && p["endpointId"] == endpoint
                && p["decoded"]["routing"]["match"]["type"] == "UNMATCHED"
        })
        .count()
}

/// Exercises the public application commands and operating-system UDP sockets.
/// Both systems deliberately reuse all 50 logical addresses and controllers
/// independently assign the same range of wire session numbers.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_hundred_devices_register_concurrently_with_two_isolated_controllers() {
    let directory = tempfile::tempdir().unwrap();
    let (app, receiver) = Studio::open(directory.path()).unwrap();
    app.start_receiver(receiver);
    app.start_scheduler();

    let result = tokio::time::timeout(Duration::from_secs(20), async {
        let snapshot = request(&app, json!({"action":"snapshot"})).await;
        let protocol = snapshot["protocols"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["published"] == true)
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let system_a = snapshot["systems"][0]["id"].as_str().unwrap().to_owned();
        let system_b = request(&app, json!({"action":"system_create","name":"独立系统 B"}))
            .await["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let mut groups = Vec::new();
        for (index, system) in [system_a, system_b].into_iter().enumerate() {
            let local = request(
                &app,
                json!({"action":"endpoint_start","name":format!("设备侧 {index}"),
                    "bind":"127.0.0.1:0","protocolId":protocol}),
            )
            .await;
            let controller = request(
                &app,
                json!({"action":"endpoint_start","name":format!("控制端 {index}"),
                    "bind":"127.0.0.1:0","protocolId":protocol,"virtualPeer":true,
                    "fault":{"delayMs":1000}}),
            )
            .await;
            let mut devices = Vec::new();
            for address in 59..109 {
                let device = request(
                    &app,
                    json!({"action":"device_upsert","name":format!("系统 {index} 设备 {address}"),
                        "systemId":system,"address":format!("00 00 {address:02X}")}),
                )
                .await;
                assert_eq!(device["status"], "OFFLINE");
                devices.push(device["id"].as_str().unwrap().to_owned());
            }
            groups.push(Group {
                system,
                local_id: local["id"].as_str().unwrap().into(),
                local_bind: local["bind"].as_str().unwrap().into(),
                controller_id: controller["id"].as_str().unwrap().into(),
                controller_bind: controller["bind"].as_str().unwrap().into(),
                devices,
            });
        }
        let template = request(
            &app,
            json!({"action":"template_install","id":"register-no-auth","protocolId":protocol}),
        )
        .await;
        let flow_id = template["id"].as_str().unwrap().to_owned();
        request(&app, json!({"action":"flow_publish","flowId":flow_id})).await;

        // With no request open, even a successful-looking permit/completion from
        // the configured controller must leave every device and session intact.
        let schema = app.schema(&protocol).unwrap();
        for code in [0x02, 0x04] {
            let bytes = schema.encode(code, &json!({
                "srcAddr":"00 00 01","dstAddr":"00 00 3B",
                "transactionId":u32::MAX - 10,"sessionId":65000,"flags":2,
                "body":{"resultCode":0,"validSeconds":3600,"authMode":0,"acceptedCapabilities":3}
            })).unwrap();
            app.udp.send(&groups[0].controller_id, &groups[0].local_bind, &bytes).await.unwrap();
        }
        while unmatched_test_responses(&app, &groups[0].local_id) != 2 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let unchanged = app.domain.lock().unwrap().clone();
        assert!(unchanged.sessions.is_empty(), "unsolicited permits must not create sessions");
        assert!(unchanged.devices.values().all(|d| d.state == DeviceState::Offline && !d.registration_valid));

        let start = Instant::now();
        let launch = Arc::new(Barrier::new(101));
        let peak_pending = Arc::new(AtomicUsize::new(0));
        let mut tasks = JoinSet::new();
        for group in &groups {
            for (offset, device_id) in group.devices.iter().enumerate() {
                let address = 59 + offset;
                let values = json!({"action":"flow_run","flowId":flow_id,
                    "endpointId":group.local_id,"remote":group.controller_bind,
                    "variables":{"systemId":group.system,"deviceAddress":format!("00 00 {address:02X}"),
                        "deviceNumericId":address}});
                let app = app.clone();
                let launch = launch.clone();
                let peak = peak_pending.clone();
                let device_id = device_id.clone();
                tasks.spawn(async move {
                    launch.wait().await;
                    let run = request(&app, values).await;
                    let open = app
                        .execution
                        .lock()
                        .unwrap()
                        .engine
                        .pending
                        .requests
                        .values()
                        .filter(|p| p.status.is_open())
                        .count();
                    peak.fetch_max(open, Ordering::Relaxed);
                    (device_id, run["runId"].as_str().unwrap().to_owned())
                });
            }
        }
        launch.wait().await;
        let mut runs = BTreeMap::new();
        while let Some(task) = tasks.join_next().await {
            let (device, run) = task.unwrap();
            runs.insert(device, run);
        }
        assert_eq!(runs.len(), 100);
        assert_eq!(peak_pending.load(Ordering::Relaxed), 100, "all 100 registrations must actually overlap");

        // During overlapping waits, send validly encoded but unrelated
        // permits/completions through real sockets. One preserves the
        // pending transaction but originates at the other controller; another
        // uses the correct remote with a transaction that was never issued.
        let waiting = {
            let execution = app.execution.lock().unwrap();
            execution
                .engine
                .pending
                .requests
                .values()
                .find(|p| p.endpoint_id == groups[0].local_id && p.status.is_open())
                .unwrap()
                .clone()
        };
        assert_eq!(waiting.request_msg_code, 1);
        for (sender, code, transaction) in [
            (&groups[1].controller_id, 0x02, waiting.transaction_id.unwrap()),
            (&groups[0].controller_id, 0x02, u32::MAX),
            (&groups[0].controller_id, 0x04, u32::MAX - 1),
        ] {
            let bytes = schema
                .encode(
                    code,
                    &json!({"srcAddr":waiting.expect_src_addr,"dstAddr":waiting.expect_dst_addr,
                        "transactionId":transaction,"sessionId":65000,"flags":2,
                        "body":{"resultCode":0,"validSeconds":3600,"authMode":0,"acceptedCapabilities":3}}),
                )
                .unwrap();
            app.udp.send(sender, &groups[0].local_bind, &bytes).await.unwrap();
        }
        while unmatched_test_responses(&app, &groups[0].local_id) != 5 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(app.domain.lock().unwrap().sessions.values().all(|s| s.wire_session_id != 65000));

        loop {
            let completed = {
            let execution = app.execution.lock().unwrap();
            for run_id in runs.values() {
                let run = &execution.engine.runs[run_id];
                assert_ne!(run.status, FlowStatus::Failed, "registration failed: {:?}", run.failure);
            }
            runs.values().all(|id| execution.engine.runs[id].status == FlowStatus::Completed)
            };
            if completed && app.domain.lock().unwrap().devices.values().all(|d| d.state == DeviceState::Online) {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        let domain = app.domain.lock().unwrap();
        assert_eq!(domain.devices.len(), 100);
        assert_eq!(domain.sessions.len(), 100, "no session may be manufactured by unrelated responses");
        assert!(domain.devices.values().all(|d| d.registration_valid && d.state == DeviceState::Online));
        let execution = app.execution.lock().unwrap();
        let mut wire_session_sets = Vec::new();
        for group in &groups {
            let expected_scope = format!("{}|{}|{}", group.local_id, protocol, group.controller_bind);
            let mut wire_sessions = BTreeSet::new();
            for (offset, device_id) in group.devices.iter().enumerate() {
                let device = &domain.devices[device_id];
                assert_eq!(device.system_id, group.system);
                assert_eq!(device.identities[0].address, vec![0, 0, (59 + offset) as u8]);
                let sessions: Vec<_> = domain.sessions.values().filter(|s| &s.device_id == device_id).collect();
                assert_eq!(sessions.len(), 1);
                let session = sessions[0];
                assert!(session.active && session.registered);
                assert_eq!(session.peer_system_id, expected_scope, "session escaped its endpoint/protocol/controller scope");
                assert_eq!(execution.engine.runs[&runs[device_id]].variables["sessionId"], session.wire_session_id);
                assert!(wire_sessions.insert(session.wire_session_id));
            }
            assert_eq!(wire_sessions, (1..=50).collect::<BTreeSet<u16>>());
            wire_session_sets.push(wire_sessions);
        }
        assert_eq!(wire_session_sets[0], wire_session_sets[1], "the test must exercise colliding wire session numbers across scopes");
        assert_eq!(execution.engine.pending.requests.values().filter(|p| p.status.is_open()).count(), 0);
        assert!(execution.errors.is_empty(), "business errors: {:?}", execution.errors);
        eprintln!("100 ONLINE; 2 controllers; peak pending={}; 5 unrelated responses ignored; registration elapsed={:?}", peak_pending.load(Ordering::Relaxed), start.elapsed());
    })
    .await;

    app.shutdown.cancel();
    app.udp.stop_all().await;
    result.expect("100 concurrent registrations must finish within 20 seconds");
}
