use musp_studio::application::Studio;
use serde_json::{Value, json};
use std::sync::Arc;
async fn setup() -> (
    tempfile::TempDir,
    Arc<Studio>,
    String,
    String,
    String,
    String,
) {
    let dir = tempfile::tempdir().unwrap();
    let (app, rx) = Studio::open(dir.path()).unwrap();
    app.start_receiver(rx);
    app.start_scheduler();
    let snap = app.request(json!({"action":"snapshot"})).await.unwrap();
    let protocol = snap["protocols"][0]["id"].as_str().unwrap().to_string();
    let system = snap["systems"][0]["id"].as_str().unwrap().to_string();
    let a=app.request(json!({"action":"endpoint_start","name":"A","bind":"127.0.0.1:0","protocolId":protocol})).await.unwrap();
    let b=app.request(json!({"action":"endpoint_start","name":"真实UDP虚拟对端","bind":"127.0.0.1:0","protocolId":protocol,"virtualPeer":true})).await.unwrap();
    (
        dir,
        app,
        protocol,
        system,
        a["id"].as_str().unwrap().into(),
        b["bind"].as_str().unwrap().into(),
    )
}
async fn run(
    app: &Arc<Studio>,
    flow: Value,
    endpoint: &str,
    remote: &str,
    variables: Value,
) -> Value {
    app.request(json!({"action":"flow_save","flow":flow}))
        .await
        .unwrap();
    app.request(json!({"action":"flow_publish","flowId":flow["id"]}))
        .await
        .unwrap();
    let r=app.request(json!({"action":"flow_run","flowId":flow["id"],"endpointId":endpoint,"remote":remote,"variables":variables})).await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let snap = app.request(json!({"action":"snapshot"})).await.unwrap();
            let run = snap["runs"]
                .as_array()
                .unwrap()
                .iter()
                .find(|x| x["id"] == r["runId"])
                .unwrap();
            if !["RUNNING", "WAITING"].contains(&run["status"].as_str().unwrap()) {
                assert_eq!(run["status"], "COMPLETED", "{run}");
                break snap;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}
#[tokio::test]
async fn five_devices_only_appear_after_real_config_ack_then_register_online() {
    let (_dir, app, protocol, system, endpoint, remote) = setup().await;
    let items: Vec<_> = (59..64)
        .map(|a| json!({"deviceAddr":a,"deviceType":1,"deviceNumericId":a,"initialParameter":1}))
        .collect();
    let flow = json!({"id":"config-five","nameZh":"配置五台设备","version":"1","protocol":{"id":protocol},"nodes":[{"id":"start","type":"START"},{"id":"send","type":"SEND_AND_WAIT","data":{"code":33,"expectedCodes":[34],"timeoutMs":1000,"values":{"srcAddr":"00 00 01","dstAddr":"00 00 3B","sessionId":0,"body":{"configMode":1,"targetDeviceAddr":0,"items":items}}}},{"id":"end","type":"END"},{"id":"error","type":"ERROR"}],"edges":[{"source":"start","target":"send"},{"source":"send","target":"end","label":"matched"},{"source":"send","target":"error","label":"timeout"}]});
    let snap = run(&app, flow, &endpoint, &remote, json!({"systemId":system})).await;
    assert_eq!(snap["devices"].as_array().unwrap().len(), 5);
    assert!(
        snap["devices"]
            .as_array()
            .unwrap()
            .iter()
            .all(|d| d["status"] == "OFFLINE")
    );
    let registration = json!({"id":"registration","nameZh":"注册","version":"1","protocol":{"id":protocol},"nodes":[{"id":"start","type":"START"},{"id":"req","type":"SEND_AND_WAIT","data":{"code":1,"expectedCodes":[2],"timeoutMs":1000,"values":{"body":{"deviceType":1,"deviceNumericId":59,"bootReason":1,"capabilityFlags":3,"configVersion":1}}}},{"id":"sid","type":"ASSIGN","data":{"set":[{"path":"sessionId","from":"response.header.sessionId"}]}},{"id":"confirm","type":"SEND_AND_WAIT","data":{"code":3,"expectedCodes":[4],"timeoutMs":1000,"values":{"body":{"deviceNumericId":59,"confirmResult":0}}}},{"id":"end","type":"END"},{"id":"error","type":"ERROR"}],"edges":[{"source":"start","target":"req"},{"source":"req","target":"sid","label":"matched"},{"source":"req","target":"error","label":"timeout"},{"source":"sid","target":"confirm"},{"source":"confirm","target":"end","label":"matched"},{"source":"confirm","target":"error","label":"timeout"}]});
    let snap = run(
        &app,
        registration,
        &endpoint,
        &remote,
        json!({"systemId":system,"srcAddr":"00 00 3B","dstAddr":"00 00 01","sessionId":0}),
    )
    .await;
    assert_eq!(
        snap["devices"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|d| d["status"] == "ONLINE")
            .count(),
        1
    );
    assert!(snap["stats"]["rx"].as_u64().unwrap() >= 6);
    app.udp.stop_all().await;
}

#[test]
fn graph_branches_parallel_join_and_subflow_execute() {
    use musp_studio::runtime::*;
    let mut e = FlowEngine::default();
    let sub:FlowDefinition=serde_json::from_value(json!({"id":"sub","nodes":[{"id":"s","type":"START"},{"id":"assign","type":"ASSIGN","data":{"path":"subRan","value":true}},{"id":"e","type":"END"}],"edges":[{"source":"s","target":"assign"},{"source":"assign","target":"e"}]})).unwrap();
    e.publish(sub).unwrap();
    let main:FlowDefinition=serde_json::from_value(json!({"id":"main","nodes":[{"id":"s","type":"START"},{"id":"p","type":"PARALLEL","data":{"joinId":"j"}},{"id":"a","type":"ASSIGN","data":{"path":"left","value":true}},{"id":"b","type":"INVOKE_SUBFLOW","data":{"flowId":"sub"}},{"id":"j","type":"JOIN"},{"id":"check","type":"CONDITION","data":{"expression":"left == true && subRan == true"}},{"id":"end","type":"END"},{"id":"error","type":"ERROR"}],"edges":[{"source":"s","target":"p"},{"source":"p","target":"a"},{"source":"p","target":"b"},{"source":"a","target":"j"},{"source":"b","target":"j"},{"source":"j","target":"check"},{"source":"check","target":"end","label":"true"},{"source":"check","target":"error","label":"false"}]})).unwrap();
    e.publish(main).unwrap();
    let r = e.start("main", json!({}), 0).unwrap();
    e.poll(1);
    assert_eq!(e.runs[&r].status, FlowStatus::Completed);
}
