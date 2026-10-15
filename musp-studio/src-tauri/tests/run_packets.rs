use musp_studio::{application::Studio, persistence::Database};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio::net::UdpSocket;

const SIMPLE_XML: &str = r#"<protocol id="lab-two-byte" version="1"><messages>
<message id="REQUEST" code="0x21"><header><field key="code" bytes="1" binding="message.code"/></header><body><field key="value" bytes="1"/></body></message>
<message id="RESPONSE" code="0x22"><header><field key="code" bytes="1" binding="message.code"/></header><body><field key="value" bytes="1"/></body></message>
<message id="EVENT" code="0x73"><header><field key="code" bytes="1" binding="message.code"/></header><body><field key="value" bytes="1"/></body></message>
</messages></protocol>"#;

async fn start(
    receiver: bool,
) -> (
    tempfile::TempDir,
    Arc<Studio>,
    String,
    Value,
    tokio::sync::mpsc::Receiver<musp_studio::network::Datagram>,
) {
    start_xml(receiver, SIMPLE_XML).await
}

async fn start_xml(
    receiver: bool,
    xml: &str,
) -> (
    tempfile::TempDir,
    Arc<Studio>,
    String,
    Value,
    tokio::sync::mpsc::Receiver<musp_studio::network::Datagram>,
) {
    let dir = tempfile::tempdir().unwrap();
    let (app, rx) = Studio::open(dir.path()).unwrap();
    let protocol = app
        .request(json!({"action":"protocol_publish","name":"实验协议","xml":xml}))
        .await
        .unwrap();
    let endpoint = app.request(json!({"action":"endpoint_start","name":"实验端点","bind":"127.0.0.1:0","protocolId":protocol["id"]})).await.unwrap();
    // Return the receiver for the project-switch test; normal cases install it below.
    let rx = if receiver {
        let (_, unused) = tokio::sync::mpsc::channel(1);
        app.start_receiver(rx);
        unused
    } else {
        rx
    };
    (
        dir,
        app,
        protocol["id"].as_str().unwrap().into(),
        endpoint,
        rx,
    )
}

async fn publish(app: &Arc<Studio>, protocol: &str, id: &str, kind: &str, data: Value) {
    let mut edges = vec![json!({"source":"start","target":"wire"})];
    if kind == "SEND" {
        edges.push(json!({"source":"wire","target":"end"}));
    } else {
        edges.extend([
            json!({"source":"wire","target":"end","label":"matched"}),
            json!({"source":"wire","target":"error","label":"timeout"}),
        ]);
    }
    let flow = json!({"id":id,"nameZh":id,"version":"1","protocol":{"id":protocol},"nodes":[{"id":"start","type":"START"},{"id":"wire","type":kind,"data":data},{"id":"end","type":"END"},{"id":"error","type":"ERROR"}],"edges":edges});
    app.request(json!({"action":"flow_save","flow":flow}))
        .await
        .unwrap();
    app.request(json!({"action":"flow_publish","flowId":id}))
        .await
        .unwrap();
}

async fn run(
    app: &Arc<Studio>,
    endpoint: &Value,
    remote: &UdpSocket,
    id: &str,
    variables: Value,
) -> String {
    let value=app.request(json!({"action":"flow_run","flowId":id,"endpointId":endpoint["id"],"remote":remote.local_addr().unwrap().to_string(),"variables":variables})).await.unwrap();
    value["runId"].as_str().unwrap().into()
}

async fn receive(socket: &UdpSocket) -> Vec<u8> {
    let mut bytes = [0; 64];
    let (size, _) = tokio::time::timeout(Duration::from_secs(2), socket.recv_from(&mut bytes))
        .await
        .expect("flow did not send the actual datagram")
        .unwrap();
    bytes[..size].to_vec()
}

async fn logs(app: &Arc<Studio>, count: u64) -> Value {
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let value = app.request(json!({"action":"packet_query"})).await.unwrap();
            if value["total"].as_u64().unwrap() >= count {
                break value;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("actual UDP packets were not persisted")
}

#[tokio::test]
async fn generic_concurrent_send_wait_logs_use_exact_pending_and_send_context() {
    let (_dir, app, protocol, endpoint, _) = start(true).await;
    let peer = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let other = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    publish(&app,&protocol,"exchange","SEND_AND_WAIT",json!({"code":33,"values":{"body":{"value":"${tag}"}},"expectedCodes":[34],"match":{"business":{"value":"${tag}"}},"timeoutMs":3000})).await;
    let first = run(&app, &endpoint, &peer, "exchange", json!({"tag":11})).await;
    let second = run(&app, &endpoint, &peer, "exchange", json!({"tag":22})).await;
    assert_eq!(receive(&peer).await, [0x21, 11]);
    assert_eq!(receive(&peer).await, [0x21, 22]);
    let target = endpoint["bind"].as_str().unwrap();
    peer.send_to(&[0x73, 9], target).await.unwrap(); // Same socket, unrelated code.
    other.send_to(&[0x22, 11], target).await.unwrap(); // Correct body, wrong peer.
    peer.send_to(&[0x22, 22], target).await.unwrap(); // Complete in reverse order.
    peer.send_to(&[0x22, 11], target).await.unwrap();
    peer.send_to(&[0x22, 11], target).await.unwrap(); // Duplicate must not become another RX match.
    peer.send_to(&[0x22], target).await.unwrap(); // Truncated body remains visible at the endpoint.
    let all = logs(&app, 8).await;
    for (id, tag) in [(&first, "0B"), (&second, "16")] {
        let own = app
            .request(json!({"action":"packet_query","runId":id}))
            .await
            .unwrap();
        assert_eq!(
            own["total"], 2,
            "run query included unrelated or concurrent packets: {own}"
        );
        let mut directions = vec![];
        for row in own["items"].as_array().unwrap() {
            assert_eq!(row["runId"], *id);
            assert_eq!(row["nodeId"], "wire");
            assert!(row["hex"].as_str().unwrap().ends_with(tag));
            directions.push(row["direction"].as_str().unwrap());
        }
        directions.sort();
        assert_eq!(directions, ["RX", "TX"]);
    }
    assert_eq!(
        all["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|p| p["runId"].is_null())
            .count(),
        4
    );
    let endpoint_packets = app
        .request(json!({"action":"packet_query","endpointId":endpoint["id"]}))
        .await
        .unwrap();
    assert_eq!(endpoint_packets["total"], 8);
    assert!(
        endpoint_packets["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["status"] == "解析失败" && p["runId"].is_null())
    );
    assert_eq!(
        app.request(
            json!({"action":"packet_query","runId":first,"endpointId":"not-this-endpoint"})
        )
        .await
        .unwrap()["total"],
        0
    );
    assert_eq!(
        app.request(json!({"action":"packet_query","endpointId":"not-this-endpoint"}))
            .await
            .unwrap()["total"],
        0
    );
    assert!(
        app.domain.lock().unwrap().devices.is_empty(),
        "generic 0x21/0x22 must not create MUSP devices"
    );
    {
        let execution = app.execution.lock().unwrap();
        assert_eq!(
            format!("{:?}", execution.engine.runs[&first].status),
            "Completed"
        );
        assert_eq!(
            format!("{:?}", execution.engine.runs[&second].status),
            "Completed"
        );
    }
    app.udp.stop_all().await;
}

#[tokio::test]
async fn generic_passive_wait_associates_rx_without_a_sent_context() {
    let (_dir, app, protocol, endpoint, _) = start(true).await;
    let peer = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    publish(
        &app,
        &protocol,
        "listen",
        "WAIT_PACKET",
        json!({"expectedCodes":[115],"match":{"business":{"value":9}},"timeoutMs":3000}),
    )
    .await;
    let id = run(&app, &endpoint, &peer, "listen", json!({})).await;
    assert!(app.execution.lock().unwrap().sent.is_empty());
    peer.send_to(&[0x73, 8], endpoint["bind"].as_str().unwrap())
        .await
        .unwrap();
    peer.send_to(&[0x73, 9], endpoint["bind"].as_str().unwrap())
        .await
        .unwrap();
    logs(&app, 2).await;
    let own = app
        .request(json!({"action":"packet_query","runId":id}))
        .await
        .unwrap();
    assert_eq!(
        own["total"], 1,
        "passive RX must be tied to the matched wait: {own}"
    );
    assert_eq!(own["items"][0]["direction"], "RX");
    assert_eq!(own["items"][0]["hex"], "73 09");
    assert_eq!(own["items"][0]["runId"], id);
    assert_eq!(own["items"][0]["nodeId"], "wire");
    assert_eq!(
        own["items"][0]["decoded"]["routing"]["match"]["type"],
        "MATCHED"
    );
    assert_eq!(
        format!(
            "{:?}",
            app.execution.lock().unwrap().engine.runs[&id].status
        ),
        "Completed"
    );
    assert!(app.execution.lock().unwrap().errors.is_empty());
    app.udp.stop_all().await;
}

#[tokio::test]
async fn queued_send_only_context_stays_in_original_project_and_survives_reopen() {
    let (dir, app, protocol, endpoint, mut rx) = start(false).await;
    let original = app.project();
    let peer = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    publish(
        &app,
        &protocol,
        "send-only",
        "SEND",
        json!({"code":33,"values":{"body":{"value":7}}}),
    )
    .await;
    let id = run(&app, &endpoint, &peer, "send-only", json!({})).await;
    assert_eq!(receive(&peer).await, [0x21, 7]);
    app.udp.stop_all().await;
    app.request(json!({"action":"project_create","name":"另一个项目"}))
        .await
        .unwrap();
    app.record_packet(rx.recv().await.unwrap()).unwrap();
    assert_eq!(
        app.request(json!({"action":"packet_query","runId":id}))
            .await
            .unwrap()["total"],
        0
    );
    assert!(
        app.request(json!({"action":"packet_query","runId":id,"expectedProjectId":original}))
            .await
            .is_err()
    );
    app.request(json!({"action":"project_open","id":original}))
        .await
        .unwrap();
    let own = app
        .request(json!({"action":"packet_query","runId":id}))
        .await
        .unwrap();
    assert_eq!(own["total"], 1);
    assert_eq!(own["items"][0]["runId"], id);
    assert_eq!(own["items"][0]["nodeId"], "wire");
    drop(app);
    let (reopened, _) = Studio::open(dir.path()).unwrap();
    let own = reopened
        .request(json!({"action":"packet_query","runId":id}))
        .await
        .unwrap();
    assert_eq!(own["total"], 1);
    assert_eq!(own["items"][0]["nodeId"], "wire");
}

#[tokio::test]
async fn old_packet_schema_migrates_with_unknown_run_context() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db/musp.db");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let old = rusqlite::Connection::open(&path).unwrap();
    old.execute_batch(include_str!("../migrations/0001_init.sql"))
        .unwrap();
    old.execute("INSERT INTO projects VALUES('old','旧项目',1)", [])
        .unwrap();
    old.execute("INSERT INTO packets(project_id,event_time,direction,endpoint_id,remote,protocol_id,msg_code,byte_length,hex,status,decoded_json) VALUES('old',1,'RX','endpoint','127.0.0.1:1','old-protocol',33,2,'21 01','已解析','{}')",[]).unwrap();
    drop(old);
    let (app, _) = Studio::open(dir.path()).unwrap();
    let all = app.request(json!({"action":"packet_query"})).await.unwrap();
    assert_eq!(all["total"], 1);
    assert!(all["items"][0].get("runId").is_some_and(Value::is_null));
    assert!(all["items"][0].get("nodeId").is_some_and(Value::is_null));
    assert_eq!(
        app.request(json!({"action":"packet_query","runId":"unrelated"}))
            .await
            .unwrap()["total"],
        0
    );
    drop(app);
    assert_eq!(
        Database::open(&path)
            .unwrap()
            .packets("old", 0, 10, "")
            .unwrap()["total"],
        1
    );
}

#[tokio::test]
async fn musp_virtual_peer_is_rejected_for_generic_protocol() {
    let (_dir, app, protocol, _, _) = start(true).await;
    let result=app.request(json!({"action":"endpoint_start","name":"错误对端","bind":"127.0.0.1:0","protocolId":protocol,"virtualPeer":true})).await;
    assert!(
        result.is_err(),
        "MUSP peer must not pretend to implement a different protocol"
    );
    assert_eq!(app.udp.list().await.len(), 1);
    app.udp.stop_all().await;
}

#[tokio::test]
async fn known_musp_send_still_requires_explicit_resolved_addresses() {
    let dir = tempfile::tempdir().unwrap();
    let (app, rx) = Studio::open(dir.path()).unwrap();
    app.start_receiver(rx);
    let protocol = app
        .db
        .protocols(&app.project())
        .unwrap()
        .into_iter()
        .find(|p| p.published)
        .unwrap()
        .id;
    let endpoint = app
        .request(json!({"action":"endpoint_start","bind":"127.0.0.1:0","protocolId":protocol}))
        .await
        .unwrap();
    let peer = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    publish(
        &app,
        &protocol,
        "missing-address",
        "SEND",
        json!({"code":49,"values":{"body":{"uptimeSeconds":0,"statusFlags":1}}}),
    )
    .await;
    let id = run(&app, &endpoint, &peer, "missing-address", json!({})).await;
    assert_eq!(
        app.udp.metrics().tx,
        0,
        "missing MUSP addresses must not silently become zero"
    );
    assert_eq!(
        format!(
            "{:?}",
            app.execution.lock().unwrap().engine.runs[&id].status
        ),
        "Failed"
    );
    app.udp.stop_all().await;
}

#[tokio::test]
async fn explicit_generic_session_matchers_are_not_replaced_by_sent_session() {
    let (_dir, app, _, initial, _) = start(true).await;
    app.udp.stop(initial["id"].as_str().unwrap()).await.unwrap();
    let xml = SIMPLE_XML.replace(
        "</header>",
        "<field key=\"sessionId\" bytes=\"2\"/></header>",
    );
    let protocol = app
        .request(json!({"action":"protocol_publish","xml":xml}))
        .await
        .unwrap();
    let protocol = protocol["id"].as_str().unwrap();
    let endpoint = app
        .request(json!({"action":"endpoint_start","bind":"127.0.0.1:0","protocolId":protocol}))
        .await
        .unwrap();
    let peer = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    for (index, matcher) in [json!({"type":"ANY"}), json!({"type":"EXACT","value":99})]
        .into_iter()
        .enumerate()
    {
        let tag = index as u8 + 1;
        let flow = format!("explicit-session-{index}");
        publish(&app,protocol,&flow,"SEND_AND_WAIT",json!({"code":33,"expectedCodes":[34],"values":{"sessionId":7,"body":{"value":tag}},"match":{"sessionMatcher":matcher,"business":{"value":tag}}})).await;
        let id = run(&app, &endpoint, &peer, &flow, json!({})).await;
        assert_eq!(receive(&peer).await, [0x21, 0, 7, tag]);
        peer.send_to(&[0x22, 0, 99, tag], endpoint["bind"].as_str().unwrap())
            .await
            .unwrap();
        logs(&app, (index as u64 + 1) * 2).await;
        let own = app
            .request(json!({"action":"packet_query","runId":id}))
            .await
            .unwrap();
        assert_eq!(
            own["total"], 2,
            "explicit matcher must retain the matched RX"
        );
        assert_eq!(
            format!(
                "{:?}",
                app.execution.lock().unwrap().engine.runs[&id].status
            ),
            "Completed"
        );
    }
    app.udp.stop_all().await;
}

const BODY_TRANSACTION_XML: &str = r#"<protocol id="lab-body-transaction" version="1"><messages>
<message id="DATA" code="0x49"><header><field key="code" bytes="1" binding="message.code"/></header><body><field key="transactionId" bytes="2"/></body></message>
</messages></protocol>"#;

#[tokio::test]
async fn missing_body_transaction_is_not_synthesized_or_sent() {
    let (_dir, app, protocol, endpoint, _) = start_xml(true, BODY_TRANSACTION_XML).await;
    let peer = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    publish(
        &app,
        &protocol,
        "missing-body-transaction",
        "SEND",
        json!({"code":73,"values":{}}),
    )
    .await;
    let id = run(
        &app,
        &endpoint,
        &peer,
        "missing-body-transaction",
        json!({}),
    )
    .await;
    assert_eq!(
        app.udp.metrics().tx,
        0,
        "runtime must not invent mandatory body business data"
    );
    assert_eq!(
        format!(
            "{:?}",
            app.execution.lock().unwrap().engine.runs[&id].status
        ),
        "Failed"
    );
    let mut buffer = [0; 16];
    assert!(
        tokio::time::timeout(Duration::from_millis(100), peer.recv_from(&mut buffer))
            .await
            .is_err()
    );
    app.udp.stop_all().await;
}

#[tokio::test]
async fn explicit_root_and_body_transactions_remain_real_send_values() {
    let (_dir, app, protocol, endpoint, _) = start_xml(true, BODY_TRANSACTION_XML).await;
    let peer = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    for (index, values) in [
        json!({"transactionId":9}),
        json!({"body":{"transactionId":9}}),
    ]
    .into_iter()
    .enumerate()
    {
        let flow = format!("explicit-body-transaction-{index}");
        publish(
            &app,
            &protocol,
            &flow,
            "SEND",
            json!({"code":73,"values":values}),
        )
        .await;
        let id = run(&app, &endpoint, &peer, &flow, json!({})).await;
        assert_eq!(receive(&peer).await, [0x49, 0, 9]);
        assert_eq!(
            format!(
                "{:?}",
                app.execution.lock().unwrap().engine.runs[&id].status
            ),
            "Completed"
        );
    }
    app.udp.stop_all().await;
}

#[tokio::test]
async fn bound_header_transaction_does_not_supply_missing_body_business_data() {
    let xml = BODY_TRANSACTION_XML.replace(
        "</header>",
        "<field key=\"transactionId\" bytes=\"2\" binding=\"flow.txn\"/></header>",
    );
    let (_dir, app, protocol, endpoint, _) = start_xml(true, &xml).await;
    let peer = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    publish(
        &app,
        &protocol,
        "header-binding-only",
        "SEND",
        json!({"code":73,"values":{"flow":{"txn":23}}}),
    )
    .await;
    let id = run(&app, &endpoint, &peer, "header-binding-only", json!({})).await;
    assert_eq!(
        app.udp.metrics().tx,
        0,
        "an explicit header binding must not cause automatic body data"
    );
    assert_eq!(
        format!(
            "{:?}",
            app.execution.lock().unwrap().engine.runs[&id].status
        ),
        "Failed"
    );
    publish(
        &app,
        &protocol,
        "header-and-body",
        "SEND",
        json!({"code":73,"values":{"flow":{"txn":23},"body":{"transactionId":9}}}),
    )
    .await;
    let id = run(&app, &endpoint, &peer, "header-and-body", json!({})).await;
    assert_eq!(receive(&peer).await, [0x49, 0, 23, 0, 9]);
    assert_eq!(
        format!(
            "{:?}",
            app.execution.lock().unwrap().engine.runs[&id].status
        ),
        "Completed"
    );
    app.udp.stop_all().await;
}

#[tokio::test]
async fn explicit_wait_transaction_filter_is_not_erased_by_empty_values() {
    let xml = SIMPLE_XML.replace(
        "</header>",
        "<field key=\"transactionId\" bytes=\"2\"/></header>",
    );
    let (_dir, app, protocol, endpoint, _) = start_xml(true, &xml).await;
    let peer = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    publish(
        &app,
        &protocol,
        "transaction-listener",
        "WAIT_PACKET",
        json!({"expectedCodes":[115],"match":{"transactionId":99},"timeoutMs":3000}),
    )
    .await;
    let id = run(&app, &endpoint, &peer, "transaction-listener", json!({})).await;
    peer.send_to(&[0x73, 0, 98, 8], endpoint["bind"].as_str().unwrap())
        .await
        .unwrap();
    logs(&app, 1).await;
    assert_eq!(
        app.request(json!({"action":"packet_query","runId":id}))
            .await
            .unwrap()["total"],
        0,
        "wrong transaction must not satisfy a configured passive wait"
    );
    peer.send_to(&[0x73, 0, 99, 9], endpoint["bind"].as_str().unwrap())
        .await
        .unwrap();
    logs(&app, 2).await;
    let own = app
        .request(json!({"action":"packet_query","runId":id}))
        .await
        .unwrap();
    assert_eq!(own["total"], 1);
    assert_eq!(own["items"][0]["hex"], "73 00 63 09");
    assert_eq!(
        format!(
            "{:?}",
            app.execution.lock().unwrap().engine.runs[&id].status
        ),
        "Completed"
    );
    app.udp.stop_all().await;
}

#[tokio::test]
async fn explicit_send_transaction_filter_is_not_replaced_by_sent_transaction() {
    let xml = SIMPLE_XML.replace(
        "</header>",
        "<field key=\"transactionId\" bytes=\"2\"/></header>",
    );
    let (_dir, app, protocol, endpoint, _) = start_xml(true, &xml).await;
    let peer = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    publish(&app, &protocol, "transaction-exchange", "SEND_AND_WAIT", json!({"code":33,"values":{"transactionId":7,"body":{"value":1}},"expectedCodes":[34],"match":{"transactionId":99},"timeoutMs":3000})).await;
    let id = run(&app, &endpoint, &peer, "transaction-exchange", json!({})).await;
    assert_eq!(receive(&peer).await, [0x21, 0, 7, 1]);
    peer.send_to(&[0x22, 0, 7, 1], endpoint["bind"].as_str().unwrap())
        .await
        .unwrap();
    logs(&app, 2).await;
    assert_eq!(
        app.request(json!({"action":"packet_query","runId":id}))
            .await
            .unwrap()["total"],
        1,
        "explicit response transaction must override inferred sent transaction"
    );
    peer.send_to(&[0x22, 0, 99, 1], endpoint["bind"].as_str().unwrap())
        .await
        .unwrap();
    logs(&app, 3).await;
    let own = app
        .request(json!({"action":"packet_query","runId":id}))
        .await
        .unwrap();
    assert_eq!(own["total"], 2);
    assert!(
        own["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["hex"] == "22 00 63 01")
    );
    assert_eq!(
        format!(
            "{:?}",
            app.execution.lock().unwrap().engine.runs[&id].status
        ),
        "Completed"
    );
    app.udp.stop_all().await;
}

#[tokio::test]
async fn automatic_header_fragment_transaction_still_correlates_real_responses() {
    let header = r#"<header><field key="code" bytes="1" binding="message.code"/></header>"#;
    let xml = SIMPLE_XML.replace("<messages>", r#"<fragments><fragment id="wire"><field key="code" bytes="1" binding="message.code"/><field key="transactionId" bytes="2"/></fragment></fragments><messages>"#).replace(header, r#"<header ref="wire"/>"#);
    let (_dir, app, protocol, endpoint, _) = start_xml(true, &xml).await;
    let peer = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    publish(
        &app,
        &protocol,
        "automatic-header",
        "SEND_AND_WAIT",
        json!({"code":33,"values":{"body":{"value":1}},"expectedCodes":[34]}),
    )
    .await;
    let id = run(&app, &endpoint, &peer, "automatic-header", json!({})).await;
    assert_eq!(receive(&peer).await, [0x21, 0, 1, 1]);
    peer.send_to(&[0x22, 0, 2, 1], endpoint["bind"].as_str().unwrap())
        .await
        .unwrap();
    logs(&app, 2).await;
    assert_eq!(
        app.request(json!({"action":"packet_query","runId":id}))
            .await
            .unwrap()["total"],
        1
    );
    peer.send_to(&[0x22, 0, 1, 1], endpoint["bind"].as_str().unwrap())
        .await
        .unwrap();
    logs(&app, 3).await;
    assert_eq!(
        app.request(json!({"action":"packet_query","runId":id}))
            .await
            .unwrap()["total"],
        2
    );
    assert_eq!(
        format!(
            "{:?}",
            app.execution.lock().unwrap().engine.runs[&id].status
        ),
        "Completed"
    );
    app.udp.stop_all().await;
}

#[tokio::test]
async fn hexadecimal_transaction_values_reject_wrong_responses_and_allow_distinct_waits() {
    let xml = SIMPLE_XML.replace(
        "</header>",
        "<field key=\"transactionId\" bytes=\"2\"/></header>",
    );
    let (_dir, app, protocol, endpoint, _) = start_xml(true, &xml).await;
    let peer = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let mut ids = Vec::new();
    for (index, values) in [
        json!({"transactionId":"00 07","body":{"value":1}}),
        json!({"header":{"transactionId":"00 08"},"body":{"value":1}}),
    ]
    .into_iter()
    .enumerate()
    {
        let flow = format!("hex-transaction-{index}");
        publish(
            &app,
            &protocol,
            &flow,
            "SEND_AND_WAIT",
            json!({"code":33,"expectedCodes":[34],"values":values,"timeoutMs":3000}),
        )
        .await;
        ids.push(run(&app, &endpoint, &peer, &flow, json!({})).await);
        assert_eq!(receive(&peer).await, [0x21, 0, 7 + index as u8, 1]);
        if index == 0 {
            peer.send_to(&[0x22, 0, 99, 1], endpoint["bind"].as_str().unwrap())
                .await
                .unwrap();
            logs(&app, 2).await;
            assert_eq!(
                app.request(json!({"action":"packet_query","runId":ids[0]}))
                    .await
                    .unwrap()["total"],
                1,
                "HEX transaction 7 must reject response transaction 99"
            );
        }
    }
    peer.send_to(&[0x22, 0, 99, 1], endpoint["bind"].as_str().unwrap())
        .await
        .unwrap();
    logs(&app, 4).await;
    for id in &ids {
        let own = app
            .request(json!({"action":"packet_query","runId":id}))
            .await
            .unwrap();
        assert_eq!(
            own["total"], 1,
            "HEX transaction must not become a wildcard: {own}"
        );
        assert_eq!(
            format!("{:?}", app.execution.lock().unwrap().engine.runs[id].status),
            "Waiting"
        );
    }
    for transaction in [8, 7] {
        peer.send_to(
            &[0x22, 0, transaction, 1],
            endpoint["bind"].as_str().unwrap(),
        )
        .await
        .unwrap();
    }
    logs(&app, 6).await;
    for id in ids {
        assert_eq!(
            app.request(json!({"action":"packet_query","runId":id}))
                .await
                .unwrap()["total"],
            2
        );
        assert_eq!(
            format!(
                "{:?}",
                app.execution.lock().unwrap().engine.runs[&id].status
            ),
            "Completed"
        );
    }
    app.udp.stop_all().await;
}

#[tokio::test]
async fn explicit_no_transaction_matching_accepts_an_unrelated_wire_transaction() {
    let xml = SIMPLE_XML.replace(
        "</header>",
        "<field key=\"transactionId\" bytes=\"2\"/></header>",
    );
    let (_dir, app, protocol, endpoint, _) = start_xml(true, &xml).await;
    let peer = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    publish(&app,&protocol,"no-transaction","SEND_AND_WAIT",json!({"code":33,"expectedCodes":[34],"noTransactionId":true,"values":{"transactionId":"00 07","body":{"value":1}},"timeoutMs":3000})).await;
    let id = run(&app, &endpoint, &peer, "no-transaction", json!({})).await;
    assert_eq!(receive(&peer).await, [0x21, 0, 7, 1]);
    peer.send_to(&[0x22, 0, 99, 1], endpoint["bind"].as_str().unwrap())
        .await
        .unwrap();
    logs(&app, 2).await;
    assert_eq!(
        format!(
            "{:?}",
            app.execution.lock().unwrap().engine.runs[&id].status
        ),
        "Completed"
    );
    app.udp.stop_all().await;
}

#[tokio::test]
async fn generic_opcode_one_passive_wait_does_not_require_a_musp_assigned_session() {
    let xml = SIMPLE_XML.replace("code=\"0x73\"", "code=\"0x01\"");
    let (_dir, app, protocol, endpoint, _) = start_xml(true, &xml).await;
    let peer = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    publish(
        &app,
        &protocol,
        "generic-one",
        "WAIT_PACKET",
        json!({"code":1,"expectedCodes":[1],"timeoutMs":3000}),
    )
    .await;
    let id = run(&app, &endpoint, &peer, "generic-one", json!({})).await;
    peer.send_to(&[1, 9], endpoint["bind"].as_str().unwrap())
        .await
        .unwrap();
    logs(&app, 1).await;
    assert_eq!(
        app.request(json!({"action":"packet_query","runId":id}))
            .await
            .unwrap()["total"],
        1
    );
    assert_eq!(
        format!(
            "{:?}",
            app.execution.lock().unwrap().engine.runs[&id].status
        ),
        "Completed"
    );
    app.udp.stop_all().await;
}

#[tokio::test]
async fn nested_header_transaction_is_taken_from_actual_decoded_field() {
    let xml = SIMPLE_XML.replace(
        "</header>",
        "<struct key=\"routing\"><field key=\"transactionId\" bytes=\"2\"/></struct></header>",
    );
    let (_dir, app, protocol, endpoint, _) = start_xml(true, &xml).await;
    let peer = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    publish(&app,&protocol,"nested-transaction","SEND_AND_WAIT",json!({"code":33,"expectedCodes":[34],"values":{"header":{"routing":{"transactionId":7}},"body":{"value":1}},"timeoutMs":3000})).await;
    let id = run(&app, &endpoint, &peer, "nested-transaction", json!({})).await;
    assert_eq!(receive(&peer).await, [0x21, 0, 7, 1]);
    peer.send_to(&[0x22, 0, 99, 1], endpoint["bind"].as_str().unwrap())
        .await
        .unwrap();
    logs(&app, 2).await;
    assert_eq!(
        app.request(json!({"action":"packet_query","runId":id}))
            .await
            .unwrap()["total"],
        1,
        "nested header cannot discard transaction correlation"
    );
    peer.send_to(&[0x22, 0, 7, 1], endpoint["bind"].as_str().unwrap())
        .await
        .unwrap();
    logs(&app, 3).await;
    assert_eq!(
        app.request(json!({"action":"packet_query","runId":id}))
            .await
            .unwrap()["total"],
        2
    );
    assert_eq!(
        format!(
            "{:?}",
            app.execution.lock().unwrap().engine.runs[&id].status
        ),
        "Completed"
    );
    app.udp.stop_all().await;
}
