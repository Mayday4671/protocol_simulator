use musp_studio::{application::Studio, domain::*, network::now_ms, runtime::FlowStatus};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};

struct Fixture {
    _directory: tempfile::TempDir,
    app: Arc<Studio>,
    protocol: String,
    system: String,
    local: Value,
    peer: Value,
    devices: Vec<String>,
}
impl Fixture {
    async fn new(delay: u64) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let (app, receiver) = Studio::open(directory.path()).unwrap();
        app.start_receiver(receiver);
        app.start_scheduler();
        let snapshot = app.request(json!({"action":"snapshot"})).await.unwrap();
        let protocol = snapshot["protocols"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["published"] == true)
            .unwrap()["id"]
            .as_str()
            .unwrap()
            .to_owned();
        let system = snapshot["systems"][0]["id"].as_str().unwrap().to_owned();
        let local = app
            .request(json!({"action":"endpoint_start","protocolId":protocol,"bind":"127.0.0.1:0"}))
            .await
            .unwrap();
        let peer = app.request(json!({"action":"endpoint_start","protocolId":protocol,"bind":"127.0.0.1:0","virtualPeer":true,"fault":{"delayMs":delay}})).await.unwrap();
        let scope = format!(
            "{}|{}|{}",
            local["id"].as_str().unwrap(),
            protocol,
            peer["bind"].as_str().unwrap()
        );
        let devices = {
            let mut domain = app.domain.lock().unwrap();
            domain.add_pool("pool", "test", 50000, "kbps").unwrap();
            (59..=61)
                .map(|address| {
                    let id = domain
                        .create_device(&system, "participant", "MUSP", vec![0, 0, address], "TEST")
                        .unwrap();
                    let session = domain
                        .permit_session(&id, &scope, 1, now_ms() + 120000, false)
                        .unwrap();
                    domain.complete_registration(&session, 0, now_ms()).unwrap();
                    id
                })
                .collect()
        };
        Self {
            _directory: directory,
            app,
            protocol,
            system,
            local,
            peer,
            devices,
        }
    }
    async fn send(&self, code: u8, source: u32, session: u16, txn: u32, body: Value) {
        let bytes=self.app.schema(&self.protocol).unwrap().encode(code,&json!({"srcAddr":source,"dstAddr":1,"sessionId":session,"transactionId":txn,"body":body})).unwrap();
        self.app
            .udp
            .send(
                self.peer["id"].as_str().unwrap(),
                self.local["bind"].as_str().unwrap(),
                &bytes,
            )
            .await
            .unwrap();
    }
    fn packets(&self, code: u8) -> Vec<Value> {
        let schema = self.app.schema(&self.protocol).unwrap();
        self.app
            .db
            .packets(&self.app.project(), 0, 500, "")
            .unwrap()["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| {
                p["direction"] == "TX"
                    && p["endpointId"] == self.local["id"]
                    && p["msgCode"] == code
            })
            .map(|p| {
                schema
                    .decode(
                        &musp_studio::application::parse_hex(p["hex"].as_str().unwrap()).unwrap(),
                    )
                    .unwrap()
                    .values
            })
            .collect()
    }
    async fn reply(&self, code: u8, txn: u32) -> Value {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if let Some(packet) = self
                    .packets(code)
                    .into_iter()
                    .find(|v| v["header"]["transactionId"] == txn)
                {
                    break packet;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("expected actual UDP response")
    }
    async fn launch(&self, template: &str, call: u16, original_txn: u32) -> String {
        let flow = self
            .app
            .request(json!({"action":"template_install","id":template,"protocolId":self.protocol}))
            .await
            .unwrap();
        self.app
            .request(json!({"action":"flow_publish","flowId":flow["id"]}))
            .await
            .unwrap();
        let result=self.app.request(json!({"action":"flow_run","flowId":flow["id"],"endpointId":self.local["id"],"remote":self.peer["bind"],"variables":{"systemId":self.system,"deviceAId":self.devices[0],"deviceBId":self.devices[1],"poolId":"pool","sessionA":1,"sessionB":1,"wireCallId":call}})).await.unwrap();
        let code = if template == "video-control" {
            0x60
        } else {
            0x65
        };
        self.send(code,59,1,original_txn,json!({"calleeAddr":60,"mediaType":2,"serviceType":1,"requestTxKbps":2048,"requestRxKbps":512,"codecType":1,"codecFlags":3,"priority":1})).await;
        result["runId"].as_str().unwrap().into()
    }
    async fn active(&self, wire: u16) -> String {
        tokio::time::timeout(Duration::from_secs(8), async {
            loop {
                if let Some(id) = {
                    let domain = self.app.domain.lock().unwrap();
                    domain
                        .calls
                        .values()
                        .find(|c| c.wire_call_id == wire && c.state == CallState::Active)
                        .map(|c| c.id.clone())
                } {
                    break id;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap()
    }
    async fn ended(&self, id: &str) {
        tokio::time::timeout(Duration::from_secs(6), async {
            loop {
                if self.app.domain.lock().unwrap().calls[id].state == CallState::Ended {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
    }
    async fn stop(&self) {
        self.app.shutdown.cancel();
        self.app.udp.stop_all().await;
    }
}

#[tokio::test]
async fn either_participant_can_end_one_call_and_duplicates_do_not_repeat_release() {
    let f = Fixture::new(0).await;
    f.launch("video-control", 101, 201).await;
    let first = f.active(101).await;
    f.launch("video-control", 102, 202).await;
    let second = f.active(102).await;
    f.launch("voice-control", 103, 203).await;
    let voice = f.active(103).await;
    // A registered non-participant cannot terminate this call.
    f.send(0x73, 61, 1, 301, json!({"callId":101,"reasonCode":3}))
        .await;
    assert_ne!(f.reply(0x74, 301).await["body"]["resultCode"], 0);
    assert_eq!(
        f.app.domain.lock().unwrap().calls[&first].state,
        CallState::Active
    );
    f.send(0x73, 59, 1, 302, json!({"callId":101,"reasonCode":3}))
        .await;
    assert_eq!(f.reply(0x74, 302).await["body"]["resultCode"], 0);
    f.ended(&first).await;
    f.send(0x73, 60, 1, 303, json!({"callId":102,"reasonCode":0}))
        .await;
    assert_eq!(f.reply(0x74, 303).await["body"]["resultCode"], 0);
    f.ended(&second).await;
    let released = f.packets(0x76).len();
    assert_eq!(released, 8);
    f.send(0x73, 59, 1, 304, json!({"callId":101,"reasonCode":3}))
        .await;
    f.reply(0x74, 304).await;
    assert_eq!(f.packets(0x76).len(), released);
    assert_eq!(
        f.app.domain.lock().unwrap().calls[&voice].state,
        CallState::Active
    );
    assert_eq!(
        f.app.domain.lock().unwrap().devices[&f.devices[0]].state,
        DeviceState::Communicating
    );
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if f.packets(0x7A)
                .iter()
                .filter(|p| p["body"]["resultCode"] == 0 && p["body"]["unknownCount"] == 0)
                .count()
                >= 4
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    f.stop().await;
}

#[tokio::test]
async fn cancel_original_request_stops_paging_and_late_reply_cannot_create_call() {
    let f = Fixture::new(500).await;
    let run = f.launch("video-control", 201, 401).await;
    tokio::time::timeout(Duration::from_secs(2), async {
        while f.packets(0x62).is_empty() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    f.send(
        0x7B,
        59,
        1,
        401,
        json!({"originalTransactionId":401,"cancelReason":0}),
    )
    .await;
    f.send(
        0x7B,
        59,
        1,
        405,
        json!({"originalTransactionId":401,"cancelReason":255}),
    )
    .await;
    f.send(
        0x7B,
        59,
        1,
        402,
        json!({"originalTransactionId":401,"cancelReason":0}),
    )
    .await;
    assert_eq!(f.reply(0x7C, 402).await["body"]["resultCode"], 0);
    assert!(
        !f.packets(0x7C)
            .iter()
            .any(|p| matches!(p["header"]["transactionId"].as_u64(), Some(401 | 405))),
        "invalid cancellation must not fabricate an acceptance result"
    );
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(
        f.app.execution.lock().unwrap().engine.runs[&run].status,
        FlowStatus::Cancelled
    );
    assert!(f.app.domain.lock().unwrap().calls.is_empty());
    assert!(
        f.packets(0x61).is_empty(),
        "late paging response must not issue a permit"
    );
    f.send(
        0x7B,
        59,
        1,
        403,
        json!({"originalTransactionId":401,"cancelReason":0}),
    )
    .await;
    assert_eq!(f.reply(0x7C, 403).await["body"]["resultCode"], 0);
    f.send(
        0x7B,
        59,
        1,
        404,
        json!({"originalTransactionId":99999,"cancelReason":0}),
    )
    .await;
    assert_eq!(f.reply(0x7C, 404).await["body"]["resultCode"], 1);
    f.stop().await;
}

#[tokio::test]
async fn end_request_requires_bound_peer_endpoint_session_and_controller_address() {
    let f = Fixture::new(50).await;
    f.launch("video-control", 301, 501).await;
    let call = f.active(301).await;
    let schema = f.app.schema(&f.protocol).unwrap();
    // A real packet from the bound peer with a stale session cannot be accepted.
    f.send(0x73, 59, 2, 502, json!({"callId":301,"reasonCode":0}))
        .await;
    // The same address and session from another UDP source is not this device.
    let stranger = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let release = schema.encode(0x73, &json!({"srcAddr":59,"dstAddr":1,"sessionId":1,"transactionId":503,"body":{"callId":301,"reasonCode":0}})).unwrap();
    stranger
        .send_to(&release, f.local["bind"].as_str().unwrap())
        .await
        .unwrap();
    // A valid registered sender cannot target a different controller address.
    let wrong_controller = schema.encode(0x73, &json!({"srcAddr":59,"dstAddr":2,"sessionId":1,"transactionId":504,"body":{"callId":301,"reasonCode":0}})).unwrap();
    f.app
        .udp
        .send(
            f.peer["id"].as_str().unwrap(),
            f.local["bind"].as_str().unwrap(),
            &wrong_controller,
        )
        .await
        .unwrap();
    assert_eq!(f.reply(0x74, 504).await["body"]["resultCode"], 1);
    // The bound remote is also insufficient when the local endpoint differs.
    let other = f
        .app
        .request(json!({"action":"endpoint_start","protocolId":f.protocol,"bind":"127.0.0.1:0"}))
        .await
        .unwrap();
    f.app
        .udp
        .send(
            f.peer["id"].as_str().unwrap(),
            other["bind"].as_str().unwrap(),
            &release,
        )
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        f.app.domain.lock().unwrap().calls[&call].state,
        CallState::Active
    );
    assert!(f.packets(0x76).is_empty());
    assert!(
        !f.packets(0x74)
            .iter()
            .any(|p| matches!(p["header"]["transactionId"].as_u64(), Some(502 | 503)))
    );
    // Once established, cancellation must explicitly direct the caller to 0x73.
    f.send(
        0x7B,
        59,
        1,
        505,
        json!({"originalTransactionId":501,"cancelReason":0}),
    )
    .await;
    assert_eq!(f.reply(0x7C, 505).await["body"]["resultCode"], 2);
    assert_eq!(
        f.app.domain.lock().unwrap().calls[&call].state,
        CallState::Active
    );
    // Concurrent A/B end requests each receive an ACK, with exactly one cleanup.
    tokio::join!(
        f.send(0x73, 59, 1, 506, json!({"callId":301,"reasonCode":0})),
        f.send(0x73, 60, 1, 507, json!({"callId":301,"reasonCode":0}))
    );
    let a = f.reply(0x74, 506).await["body"]["resultCode"]
        .as_u64()
        .unwrap();
    let b = f.reply(0x74, 507).await["body"]["resultCode"]
        .as_u64()
        .unwrap();
    assert!(
        (a == 0 && b == 3) || (a == 3 && b == 0),
        "one accepted, one already ending: {a}, {b}"
    );
    f.ended(&call).await;
    assert_eq!(f.packets(0x76).len(), 4);
    assert_eq!(f.packets(0x78).len(), 2);
    f.stop().await;
}

#[tokio::test]
async fn cancelling_allocated_call_releases_its_resource_and_ignores_late_configuration_ack() {
    let f = Fixture::new(500).await;
    let run = f.launch("video-control", 401, 601).await;
    tokio::time::timeout(Duration::from_secs(3), async {
        while f.packets(0x70).is_empty() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let call = {
        let domain = f.app.domain.lock().unwrap();
        let call = domain
            .calls
            .values()
            .find(|c| c.wire_call_id == 401)
            .unwrap();
        assert_ne!(call.state, CallState::Active);
        assert_eq!(
            domain
                .resources
                .values()
                .filter(|r| r.key.call_id == call.id)
                .count(),
            1
        );
        call.id.clone()
    };
    f.send(
        0x7B,
        59,
        1,
        602,
        json!({"originalTransactionId":601,"cancelReason":0}),
    )
    .await;
    assert_eq!(f.reply(0x7C, 602).await["body"]["resultCode"], 0);
    f.ended(&call).await;
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(
        f.app.execution.lock().unwrap().engine.runs[&run].status,
        FlowStatus::Cancelled
    );
    {
        let domain = f.app.domain.lock().unwrap();
        assert_eq!(domain.calls[&call].state, CallState::Ended);
        let resources: Vec<_> = domain
            .resources
            .values()
            .filter(|r| r.key.call_id == call)
            .collect();
        assert_eq!(
            resources.len(),
            1,
            "late ACK must not reserve the next resource"
        );
        assert_eq!(resources[0].state, ResourceState::Released);
        assert!(domain.calls[&call].active_devices.is_empty());
    }
    assert_eq!(f.packets(0x76).len(), 1);
    assert!(f.packets(0x71).is_empty());
    assert!(f.packets(0x75).is_empty());
    f.stop().await;
}

#[tokio::test]
async fn reusing_wire_session_number_does_not_authorize_the_previous_call_epoch() {
    let f = Fixture::new(0).await;
    f.launch("voice-control", 501, 701).await;
    let call = f.active(501).await;
    {
        let mut domain = f.app.domain.lock().unwrap();
        let scope = format!(
            "{}|{}|{}",
            f.local["id"].as_str().unwrap(),
            f.protocol,
            f.peer["bind"].as_str().unwrap()
        );
        let fresh = domain
            .permit_session(&f.devices[0], &scope, 1, now_ms() + 120000, false)
            .unwrap();
        domain.complete_registration(&fresh, 0, now_ms()).unwrap();
    }
    f.send(0x73, 59, 1, 702, json!({"callId":501,"reasonCode":0}))
        .await;
    assert_eq!(f.reply(0x74, 702).await["body"]["resultCode"], 1);
    f.send(
        0x7B,
        59,
        1,
        703,
        json!({"originalTransactionId":701,"cancelReason":0}),
    )
    .await;
    assert_eq!(f.reply(0x7C, 703).await["body"]["resultCode"], 1);
    assert_eq!(
        f.app.domain.lock().unwrap().calls[&call].state,
        CallState::Active
    );
    assert!(f.packets(0x76).is_empty());
    f.stop().await;
}
