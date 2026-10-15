use musp_studio::persistence::Database;
use serde_json::json;

#[test]
fn migration_versions_and_immutable_protocol_snapshot() {
    let dir = tempfile::tempdir().unwrap();
    let db = Database::open(&dir.path().join("musp.db")).unwrap();
    let p = db.create_project("集成测试").unwrap();
    let protocol = db
        .save_protocol(&p.id, None, "测试", "<draft />", false)
        .unwrap();
    let published = db
        .save_protocol(&p.id, Some(&protocol.id), "测试", "<published />", true)
        .unwrap();
    db.save_protocol(&p.id, Some(&protocol.id), "测试", "<edited />", false)
        .unwrap();
    assert_eq!(db.protocol_source(&published.id).unwrap(), "<published />");
    db.save_entity(&p.id, "devices", "d1", &json!({"state":"00"}))
        .unwrap();
    assert_eq!(db.entities(&p.id, "devices").unwrap().len(), 1);
    drop(db);
    let reopened = Database::open(&dir.path().join("musp.db")).unwrap();
    assert_eq!(reopened.projects().unwrap().len(), 1);
}
