use musp_studio::persistence::Database;
use serde_json::json;

#[test]
fn device_history_records_changes_but_not_heartbeats_or_secrets() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("history.db")).unwrap();
    let project = db.create_project("history").unwrap();
    let mut state = json!({"devices":{"d1":{"id":"d1","state":"OFFLINE","name":"设备","configVersion":1,"identities":[],"activeCalls":[],"registrationValid":false,"config":{"token":"secret"},"lastSeenMs":1}}});
    db.save_entity(&project.id, "domain", "current", &state)
        .unwrap();
    state["devices"]["d1"]["lastSeenMs"] = json!(2);
    db.save_entity(&project.id, "domain", "current", &state)
        .unwrap();
    state["devices"]["d1"]["state"] = json!("ONLINE");
    state["devices"]["d1"]["registrationValid"] = json!(true);
    db.save_entity(&project.id, "domain", "current", &state)
        .unwrap();
    let history = db.entities(&project.id, "device_events").unwrap();
    assert_eq!(history.len(), 2);
    assert_eq!(history[0]["before"], serde_json::Value::Null);
    assert_eq!(history[1]["after"]["state"], "ONLINE");
    assert!(!serde_json::to_string(&history).unwrap().contains("secret"));
    drop(db);
    let db = Database::open(&dir.path().join("history.db")).unwrap();
    assert_eq!(db.entities(&project.id, "device_events").unwrap().len(), 2);
}

#[tokio::test]
async fn stale_project_requests_cannot_query_a_different_project() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = musp_studio::application::Studio::open(dir.path()).unwrap();
    let original = app.project();
    app.request(json!({"action":"project_create","name":"new"}))
        .await
        .unwrap();
    for action in ["packet_query", "device_history", "device_upsert"] {
        let error = app
            .request(json!({"action":action,"expectedProjectId":original,"id":"device"}))
            .await
            .unwrap_err();
        assert!(error.contains("项目已切换"));
    }
}

#[test]
fn project_backup_preserves_settings_and_existing_device_history() {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = musp_studio::application::Studio::open(dir.path()).unwrap();
    let project = app.project();
    app.db
        .save_entity(
            &project,
            "settings",
            "current",
            &json!({"heartbeatTimeoutMs":7000}),
        )
        .unwrap();
    app.db.save_entity(&project,"device_events","event-1",&json!({"id":"event-1","deviceId":"d1","timestamp":1,"before":null,"after":{"state":"OFFLINE"}})).unwrap();
    let exported = app.export_project().unwrap();
    let imported = app
        .import_project(exported["path"].as_str().unwrap())
        .unwrap();
    let imported_id = imported["id"].as_str().unwrap();
    assert_eq!(
        app.db.entities(imported_id, "settings").unwrap()[0]["heartbeatTimeoutMs"],
        7000
    );
    assert_eq!(
        app.db.entities(imported_id, "device_events").unwrap()[0]["id"],
        "event-1"
    );
}
