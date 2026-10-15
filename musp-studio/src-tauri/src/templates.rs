//! Offline, editable MUSP flow templates. Installation returns a draft with a
//! new identity; the application persists it using its normal project service.
use crate::runtime::FlowDefinition;
use serde_json::{Value, json};

const TEMPLATES: [(&str, &str); 7] = [
    (
        "config-devices-5",
        include_str!("../../workflows/config-devices-5.json"),
    ),
    (
        "config-parameters-5",
        include_str!("../../workflows/config-parameters-5.json"),
    ),
    (
        "register-no-auth",
        include_str!("../../workflows/register-no-auth.json"),
    ),
    (
        "register-authenticated",
        include_str!("../../workflows/register-authenticated.json"),
    ),
    ("heartbeat", include_str!("../../workflows/heartbeat.json")),
    (
        "video-control",
        include_str!("../../workflows/video-control.json"),
    ),
    (
        "voice-control",
        include_str!("../../workflows/voice-control.json"),
    ),
];

pub fn template_list() -> Vec<Value> {
    TEMPLATES.iter().map(|(id, source)| {
        let value: Value = serde_json::from_str(source).expect("embedded template JSON must be tested");
        json!({"id":id,"nameZh":value["nameZh"],"description":value["description"],"kind":"WORKFLOW",
            "protocol":"MUSP v0.8","inputs":value["inputs"],"defaultVariables":value["defaultVariables"],
            "nodeCount":value["nodes"].as_array().map(Vec::len).unwrap_or(0)})
    }).collect()
}

pub fn template_install(template_id: &str, protocol_version_id: &str) -> Result<Value, String> {
    if protocol_version_id.trim().is_empty() {
        return Err("请指定已发布协议版本".into());
    }
    let (_, source) = TEMPLATES
        .iter()
        .find(|(id, _)| *id == template_id)
        .ok_or("模板不存在")?;
    let mut flow: Value = serde_json::from_str(source).map_err(|e| e.to_string())?;
    flow["id"] = json!(uuid::Uuid::new_v4().to_string());
    flow["templateId"] = json!(template_id);
    flow["protocol"]["id"] = json!(protocol_version_id);
    flow["published"] = json!(false);
    let definition: FlowDefinition =
        serde_json::from_value(flow.clone()).map_err(|e| e.to_string())?;
    definition.validate()?;
    Ok(flow)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_templates_install_as_independent_valid_drafts() {
        assert_eq!(template_list().len(), 7);
        for (id, _) in TEMPLATES {
            let first = template_install(id, "locked-protocol-version").unwrap();
            let second = template_install(id, "locked-protocol-version").unwrap();
            assert_ne!(first["id"], second["id"]);
            assert_eq!(first["protocol"]["id"], "locked-protocol-version");
            assert_eq!(first["published"], false);
        }
    }
    #[test]
    fn call_templates_have_real_waits_and_each_resource_direction() {
        for (id, count) in [("video-control", 4), ("voice-control", 2)] {
            let flow = template_install(id, "v1").unwrap();
            let nodes = flow["nodes"].as_array().unwrap();
            assert!(nodes.iter().any(|n| n["type"] == "WAIT_PACKET"));
            assert_eq!(
                nodes
                    .iter()
                    .filter(|n| n["data"]["code"] == 0x70 || n["data"]["code"] == 0x71)
                    .count(),
                count
            );
            assert!(
                !nodes.iter().any(
                    |n| n["data"]["state"] == "ONLINE" || n["data"]["state"] == "COMMUNICATING"
                )
            );
        }
    }

    #[tokio::test]
    async fn installed_call_templates_execute_over_real_udp_and_apply_six_resources() {
        use crate::{
            application::Studio,
            domain::{CallState, DeviceState, ResourceState},
            network::now_ms,
        };
        use std::time::Duration;
        let directory = tempfile::tempdir().unwrap();
        let (app, receiver) = Studio::open(directory.path()).unwrap();
        app.start_receiver(receiver);
        app.start_scheduler();
        let snapshot = app.request(json!({"action":"snapshot"})).await.unwrap();
        let protocol = snapshot["protocols"][0]["id"].as_str().unwrap().to_owned();
        let system = snapshot["systems"][0]["id"].as_str().unwrap().to_owned();
        let local = app
            .request(json!({"action":"endpoint_start","bind":"127.0.0.1:0","protocolId":protocol}))
            .await
            .unwrap();
        let peer=app.request(json!({"action":"endpoint_start","bind":"127.0.0.1:0","protocolId":protocol,"virtualPeer":true})).await.unwrap();
        let (device_a, device_b) = {
            let mut domain = app.domain.lock().unwrap();
            domain
                .add_pool("template-pool", "测试资源池", 20_000, "kbps")
                .unwrap();
            let mut devices = vec![];
            for address in [59, 60] {
                let device = domain
                    .create_device(&system, "测试参与者", "MUSP", vec![0, 0, address], "TEST")
                    .unwrap();
                let session = domain
                    .permit_session(&device, "template-peer", 1, now_ms() + 120_000, false)
                    .unwrap();
                domain.complete_registration(&session, 0, now_ms()).unwrap();
                devices.push(device);
            }
            (devices[0].clone(), devices[1].clone())
        };
        for (template, request_code) in [("video-control", 0x60u8), ("voice-control", 0x65u8)] {
            let flow = template_install(template, &protocol).unwrap();
            app.request(json!({"action":"flow_save","flow":flow}))
                .await
                .unwrap();
            app.request(json!({"action":"flow_publish","flowId":flow["id"]}))
                .await
                .unwrap();
            let mut variables = flow["defaultVariables"].clone();
            variables["systemId"] = json!(system);
            variables["deviceAId"] = json!(device_a);
            variables["deviceBId"] = json!(device_b);
            variables["poolId"] = json!("template-pool");
            let run=app.request(json!({"action":"flow_run","flowId":flow["id"],"endpointId":local["id"],"remote":peer["bind"],"variables":variables})).await.unwrap();
            let run_id = run["runId"].as_str().unwrap().to_owned();
            let body = if request_code == 0x60 {
                json!({"calleeAddr":60,"mediaType":2,"requestTxKbps":2048,"requestRxKbps":512,"codecFlags":3,"priority":1})
            } else {
                json!({"serviceType":1,"requestTxKbps":64,"requestRxKbps":64,"codecType":1,"priority":1})
            };
            let bytes=app.schema(&protocol).unwrap().encode(request_code,&json!({"srcAddr":59,"dstAddr":1,"sessionId":1,"transactionId":900+u32::from(request_code),"body":body})).unwrap();
            app.udp
                .send(
                    peer["id"].as_str().unwrap(),
                    local["bind"].as_str().unwrap(),
                    &bytes,
                )
                .await
                .unwrap();
            tokio::time::timeout(Duration::from_secs(8), async {
                loop {
                    {
                        let execution = app.execution.lock().unwrap();
                        let current = &execution.engine.runs[&run_id];
                        match current.status {
                            crate::runtime::FlowStatus::Completed => break,
                            crate::runtime::FlowStatus::Failed => {
                                panic!("{template}: {:?}", current.failure)
                            }
                            _ => {}
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
        }
        {
            let domain = app.domain.lock().unwrap();
            assert_eq!(domain.calls.len(), 2);
            assert!(domain.calls.values().all(|c| c.state == CallState::Active));
            assert_eq!(domain.resources.len(), 6);
            assert!(
                domain
                    .resources
                    .values()
                    .all(|r| r.state == ResourceState::Active)
            );
            assert_eq!(domain.devices[&device_a].state, DeviceState::Communicating);
            assert_eq!(domain.devices[&device_a].active_calls.len(), 2);
            assert_eq!(domain.pools["template-pool"].available, 20_000 - 5120 - 128);
        }
        app.shutdown.cancel();
        app.udp.stop_all().await;
    }
}
