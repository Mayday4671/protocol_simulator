use musp_studio::{application::Studio, domain::DeviceState, runtime::FlowStatus};
use serde_json::json;
use std::time::Duration;

async fn registration(auth_before_wait: bool) {
    let directory = tempfile::tempdir().unwrap();
    let (app, rx) = Studio::open(directory.path()).unwrap();
    app.start_receiver(rx);
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
    let device=app.request(json!({"action":"device_upsert","name":"认证注册测试","systemId":system,"address":"00 00 3B"})).await.unwrap();
    let endpoint = app
        .request(json!({"action":"endpoint_start","bind":"127.0.0.1:0","protocolId":protocol}))
        .await
        .unwrap();
    let controller = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let mut flow=app.request(json!({"action":"template_install","id":"register-authenticated","protocolId":protocol})).await.unwrap();
    if auth_before_wait {
        flow["nodes"]
            .as_array_mut()
            .unwrap()
            .push(json!({"id":"delay-listener","type":"DELAY","data":{"durationMs":250}}));
        for edge in flow["edges"].as_array_mut().unwrap() {
            if edge["source"] == "save-session" {
                edge["target"] = json!("delay-listener");
            }
        }
        flow["edges"]
            .as_array_mut()
            .unwrap()
            .push(json!({"source":"delay-listener","target":"wait-auth"}));
        app.request(json!({"action":"flow_save","flow":flow}))
            .await
            .unwrap();
    }
    app.request(json!({"action":"flow_publish","flowId":flow["id"]}))
        .await
        .unwrap();
    let run=app.request(json!({"action":"flow_run","flowId":flow["id"],"endpointId":endpoint["id"],"remote":controller.local_addr().unwrap().to_string(),"variables":{"systemId":system,"deviceId":device["id"]}})).await.unwrap();
    let schema = app.schema(&protocol).unwrap();
    let mut buffer = [0u8; 2048];
    let (len, remote) =
        tokio::time::timeout(Duration::from_secs(2), controller.recv_from(&mut buffer))
            .await
            .unwrap()
            .unwrap();
    let register = schema.decode(&buffer[..len]).unwrap();
    assert_eq!(register.message_code, 1);
    let license=schema.encode(2,&json!({"srcAddr":1,"dstAddr":59,"sessionId":0x1234,"transactionId":register.values["header"]["transactionId"],"flags":2,"body":{"resultCode":0,"validSeconds":60,"authMode":1}})).unwrap();
    controller.send_to(&license, remote).await.unwrap();
    if !auth_before_wait {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let auth=schema.encode(0x11,&json!({"srcAddr":1,"dstAddr":59,"sessionId":0x1234,"transactionId":700,"body":{"tokenValidSeconds":60,"tokenLength":4,"token":"CA FE BA BE"}})).unwrap();
    controller.send_to(&auth, remote).await.unwrap();
    let (len, _) = tokio::time::timeout(Duration::from_secs(3), controller.recv_from(&mut buffer))
        .await
        .unwrap()
        .unwrap();
    let ack = schema.decode(&buffer[..len]).unwrap();
    assert_eq!(
        ack.message_code, 0x12,
        "AUTH_ACK must be sent before authenticated confirmation"
    );
    assert_eq!(ack.values["header"]["transactionId"], 700);
    assert_eq!(ack.values["body"]["resultCode"], 0);
    let (len, _) = tokio::time::timeout(Duration::from_secs(3), controller.recv_from(&mut buffer))
        .await
        .unwrap()
        .unwrap();
    let confirm = schema.decode(&buffer[..len]).unwrap();
    assert_eq!(confirm.message_code, 3);
    assert_eq!(confirm.values["header"]["sessionId"], 0x1234);
    assert_ne!(
        confirm.values["header"]["transactionId"],
        register.values["header"]["transactionId"]
    );
    assert_eq!(confirm.values["header"]["flags"], 1);
    assert!(
        confirm
            .secret_spans()
            .iter()
            .any(|(start, len)| buffer.get(*start..start + len)
                == Some(&[0xCA, 0xFE, 0xBA, 0xBE][..]))
    );
    let complete=schema.encode(4,&json!({"srcAddr":1,"dstAddr":59,"sessionId":0x1234,"transactionId":confirm.values["header"]["transactionId"],"flags":2,"body":{"resultCode":0,"acceptedCapabilities":3}})).unwrap();
    controller.send_to(&complete, remote).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let status =
                app.execution.lock().unwrap().engine.runs[run["runId"].as_str().unwrap()].status;
            assert_ne!(status, FlowStatus::Failed);
            if status == FlowStatus::Completed
                && app.domain.lock().unwrap().devices[device["id"].as_str().unwrap()].state
                    == DeviceState::Online
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let exported = app
        .request(json!({"action":"snapshot"}))
        .await
        .unwrap()
        .to_string();
    assert!(!exported.contains("CA FE BA BE") && !exported.contains("CAFEBABE"));
    app.shutdown.cancel();
    app.udp.stop_all().await;
}

#[tokio::test]
async fn authenticated_registration_waits_for_issue_acks_then_sends_token_extension() {
    registration(false).await;
}

#[tokio::test]
async fn authentication_arriving_before_wait_node_is_not_lost() {
    registration(true).await;
}
