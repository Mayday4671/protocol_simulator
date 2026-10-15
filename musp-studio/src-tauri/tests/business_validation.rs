use musp_studio::{business, domain::DomainStore};
use serde_json::json;

#[test]
fn resource_identifiers_cannot_silently_wrap_to_another_wire_resource() {
    let mut store = DomainStore::default();
    for (field, value) in [("resourceId", 65536), ("version", 65536)] {
        let mut request = json!({"operation":"reserve_resource","callId":"call","deviceId":"device","resourceId":1,"direction":"RX","version":1,"poolId":"pool","requestedBw":10,"assignedBw":10});
        request[field] = json!(value);
        let error = business::update_domain(&mut store, "project", "run", &request).unwrap_err();
        assert!(error.contains("2字节"), "{field}: {error}");
    }
}

#[tokio::test]
async fn saving_a_draft_never_silently_runs_an_older_published_flow() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = musp_studio::application::Studio::open(dir.path()).unwrap();
    let protocol = app
        .db
        .protocols(&app.project())
        .unwrap()
        .into_iter()
        .find(|p| p.published)
        .unwrap()
        .id;
    let mut flow = json!({"id":"publication-test","nameZh":"发布保护","protocol":{"id":protocol},"nodes":[{"id":"start","type":"START"},{"id":"end","type":"END"}],"edges":[{"source":"start","target":"end"}]});
    app.request(json!({"action":"flow_save","flow":flow}))
        .await
        .unwrap();
    app.request(json!({"action":"flow_publish","flowId":"publication-test"}))
        .await
        .unwrap();
    app.request(json!({"action":"flow_run","flowId":"publication-test"}))
        .await
        .unwrap();
    flow["nameZh"] = json!("未发布变更");
    app.request(json!({"action":"flow_save","flow":flow}))
        .await
        .unwrap();
    let error = app
        .request(json!({"action":"flow_run","flowId":"publication-test"}))
        .await
        .unwrap_err();
    assert!(error.contains("未发布草稿"));
}
