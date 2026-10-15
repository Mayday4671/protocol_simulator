use musp_studio::{application::Studio, domain::*, network::now_ms};
use serde_json::{Value, json};
use std::time::Duration;

#[tokio::test]
async fn real_resource_acks_end_one_call_without_stopping_another_and_timeout_keeps_capacity() {
    let dir = tempfile::tempdir().unwrap();
    let (app, rx) = Studio::open(dir.path()).unwrap();
    app.start_receiver(rx);
    app.start_scheduler();
    let snap = app.request(json!({"action":"snapshot"})).await.unwrap();
    let protocol = snap["protocols"][0]["id"].as_str().unwrap();
    let system = snap["systems"][0]["id"].as_str().unwrap();
    let local = app
        .request(json!({"action":"endpoint_start","bind":"127.0.0.1:0","protocolId":protocol}))
        .await
        .unwrap();
    let peer=app.request(json!({"action":"endpoint_start","bind":"127.0.0.1:0","protocolId":protocol,"virtualPeer":true})).await.unwrap();
    let (a, b, video, voice, resources) = {
        let mut d = app.domain.lock().unwrap();
        d.add_pool("pool", "test", 1000, "kbps").unwrap();
        let mut devices = vec![];
        for address in [59, 60] {
            let id = d
                .create_device(system, "test", "MUSP", vec![0, 0, address], "TEST")
                .unwrap();
            let session = d
                .permit_session(&id, "test", 1, now_ms() + 60000, false)
                .unwrap();
            d.complete_registration(&session, 0, now_ms()).unwrap();
            devices.push(id);
        }
        let video = d
            .create_call(
                &app.project(),
                "scenario",
                1,
                "VIDEO",
                devices.clone(),
                StartMode::LocalReady,
                now_ms(),
            )
            .unwrap();
        let voice = d
            .create_call(
                &app.project(),
                "scenario",
                2,
                "VOICE",
                vec![devices[0].clone()],
                StartMode::LocalReady,
                now_ms(),
            )
            .unwrap();
        let mut resources = vec![];
        for (call, parts) in [
            (&video, devices.clone()),
            (&voice, vec![devices[0].clone()]),
        ] {
            d.permit_call(call).unwrap();
            for device in parts {
                for direction in [Direction::Rx, Direction::Tx] {
                    let r = d
                        .reserve_resource(call, &device, 10, direction, 1, "pool", 100, 60)
                        .unwrap();
                    d.mark_configuring(&r).unwrap();
                    resources.push(d.resources[&r].clone());
                }
            }
        }
        (
            devices[0].clone(),
            devices[1].clone(),
            video,
            voice,
            resources,
        )
    };
    let mut nodes = vec![
        json!({"id":"start","type":"START"}),
        json!({"id":"end","type":"END"}),
        json!({"id":"error","type":"ERROR"}),
    ];
    let mut edges = vec![];
    let mut previous = "start".to_string();
    for (i, resource) in resources.iter().enumerate() {
        let id = format!("r{i}");
        let wire = if resource.key.call_id == video { 1 } else { 2 };
        let address = if resource.key.device_id == a {
            "00 00 3B"
        } else {
            "00 00 3C"
        };
        let rx = resource.key.direction == Direction::Rx;
        nodes.push(json!({"id":id,"type":"SEND_AND_WAIT","data":{"code":if rx{0x70}else{0x71},"expectedCodes":[0x72],"timeoutMs":1000,"values":{"srcAddr":"00 00 01","dstAddr":address,"sessionId":1,"body":{"callId":wire,"resourceId":10,"assignedRxKbps":60,"assignedTxKbps":60,"rxFreqKHz":145000,"txFreqKHz":145000,"slotIndex":1,"resourceVersion":1,"applyFlag":1}}}}));
        edges.push(json!({"source":previous,"target":id,"label":if i==0{Value::Null}else{json!("matched")}}));
        edges.push(json!({"source":id,"target":"error","label":"timeout"}));
        previous = id;
    }
    edges.push(json!({"source":previous,"target":"end","label":"matched"}));
    app.request(json!({"action":"flow_save","flow":{"id":"resources","protocol":{"id":protocol},"nodes":nodes,"edges":edges}})).await.unwrap();
    app.request(json!({"action":"flow_publish","flowId":"resources"}))
        .await
        .unwrap();
    app.request(json!({"action":"flow_run","flowId":"resources","endpointId":local["id"],"remote":peer["bind"],"variables":{"scenarioRunId":"scenario","systemId":system}})).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if app.domain.lock().unwrap().calls[&voice].state == CallState::Active {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(app.domain.lock().unwrap().pools["pool"].available, 640);
    app.request(json!({"action":"call_end","callId":video}))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if app.domain.lock().unwrap().calls[&video].state == CallState::Ended {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    {
        let d = app.domain.lock().unwrap();
        assert_eq!(d.pools["pool"].available, 880);
        assert_eq!(d.devices[&a].state, DeviceState::Communicating);
        assert_eq!(d.devices[&b].state, DeviceState::Online);
        assert_eq!(d.calls[&voice].state, CallState::Active);
    }
    app.request(json!({"action":"endpoint_stop","id":peer["id"]}))
        .await
        .unwrap();
    app.request(json!({"action":"call_end","callId":voice}))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(6), async {
        loop {
            if app.domain.lock().unwrap().calls[&voice].state == CallState::CleanupPending {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    {
        let d = app.domain.lock().unwrap();
        assert_eq!(d.pools["pool"].available, 880);
        assert!(
            d.resources
                .values()
                .filter(|r| r.key.call_id == voice)
                .any(|r| r.state == ResourceState::Unknown)
        );
    }
    assert!(app.request(json!({"action":"resource_reconcile","resourceId":resources.last().unwrap().id,"confirmedReleased":true})).await.is_err());
    app.shutdown.cancel();
    app.udp.stop_all().await;
}
