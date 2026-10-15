use musp_studio::application::{Studio, parse_hex};
use serde_json::{Value, json};

#[tokio::test]
async fn real_udp_decodes_persists_and_masks_secret_tokens() {
    let dir = tempfile::tempdir().unwrap();
    let (app, rx) = Studio::open(dir.path()).unwrap();
    app.start_receiver(rx);
    let snapshot = app.request(json!({"action":"snapshot"})).await.unwrap();
    let protocol = snapshot["protocols"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["published"] == true)
        .unwrap()["id"]
        .as_str()
        .unwrap();
    let a=app.request(json!({"action":"endpoint_start","name":"A","bind":"127.0.0.1:0","protocolId":protocol})).await.unwrap();
    let b=app.request(json!({"action":"endpoint_start","name":"B","bind":"127.0.0.1:0","protocolId":protocol})).await.unwrap();
    let vectors: Value = serde_json::from_str(include_str!(
        "../../../docs/06_示例与测试数据/golden-vectors.json"
    ))
    .unwrap();
    let raw = vectors["request"]["hex"].as_str().unwrap();
    assert_eq!(parse_hex(raw).unwrap().len(), 99);
    app.request(json!({"action":"udp_send","endpointId":a["id"],"remote":b["bind"],"hex":raw}))
        .await
        .unwrap();
    let logs = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let logs = app.request(json!({"action":"packet_query"})).await.unwrap();
            if logs["total"].as_u64().unwrap() >= 2 {
                break logs;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_eq!(logs["items"][0]["status"], "已解析");
    assert!(!logs.to_string().contains("DE AD BE EF"));
    assert!(logs.to_string().contains("** ** ** **"));
    assert_eq!(logs["items"][0]["msgCode"], 0x21);
    app.udp.stop_all().await;
}

#[test]
fn seed_all_documented_messages_compile() {
    let schema = musp_studio::protocol::compile_xml(musp_studio::application::SEED_XML).unwrap();
    assert_eq!(schema.messages.len(), 34);
}
