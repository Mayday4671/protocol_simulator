use musp_studio::{
    application::SEED_XML,
    persistence::{Database, PacketInsert},
    protocol::compile_xml,
};
use serde_json::{Value, json};

fn packet_tree() -> Value {
    let schema = compile_xml(SEED_XML).unwrap();
    let bytes=schema.encode(0x32,&json!({"srcAddr":1,"dstAddr":59,"transactionId":1,"sessionId":1,"resultCode":0,"serverTimeSeconds":0})).unwrap();
    serde_json::to_value(schema.decode(&bytes).unwrap()).unwrap()
}

#[test]
fn compressed_packet_storage_preserves_query_export_and_legacy_text_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("musp.db");
    let db = Database::open(&path).unwrap();
    let project = db.create_project("日志存储").unwrap();
    let tree = packet_tree();
    db.insert_packet(&PacketInsert {
        project_id: &project.id,
        timestamp: 1,
        direction: "RX",
        endpoint_id: "receiver",
        remote: "127.0.0.1:1234",
        protocol_id: "musp-v08",
        msg_code: Some(0x32),
        length: 30,
        hex: "AA 55",
        status: "已解析",
        decoded: &tree,
    })
    .unwrap();
    let raw = rusqlite::Connection::open(&path).unwrap();
    let (storage, stored_bytes): (String, usize) = raw
        .query_row(
            "SELECT typeof(decoded_json),length(decoded_json) FROM packets",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        storage, "blob",
        "repeated field-tree keys must use compressed storage"
    );
    assert!(
        stored_bytes < tree.to_string().len() / 3,
        "real decoded packet tree was not meaningfully compressed"
    );
    assert_eq!(
        db.packets(&project.id, 0, 10, "").unwrap()["items"][0]["decoded"],
        tree
    );
    let mut exported = vec![];
    assert_eq!(
        db.export_packets(&project.id, "json", &mut exported)
            .unwrap(),
        1
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&exported).unwrap()[0]["decoded"],
        tree
    );
    // An existing installation stores TEXT. Both types may coexist without migration.
    raw.execute("UPDATE packets SET decoded_json=?1", [tree.to_string()])
        .unwrap();
    drop(db);
    let reopened = Database::open(&path).unwrap();
    assert_eq!(
        reopened.packets(&project.id, 0, 10, "").unwrap()["items"][0]["decoded"],
        tree
    );
    let mut exported = vec![];
    reopened
        .export_packets(&project.id, "jsonl", &mut exported)
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&exported).unwrap()["decoded"],
        tree
    );
}

#[test]
fn damaged_compressed_packet_is_reported_without_returning_fabricated_decoded_data() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("musp.db");
    let db = Database::open(&path).unwrap();
    let project = db.create_project("日志校验").unwrap();
    db.insert_packet(&PacketInsert {
        project_id: &project.id,
        timestamp: 1,
        direction: "RX",
        endpoint_id: "receiver",
        remote: "127.0.0.1:1234",
        protocol_id: "musp-v08",
        msg_code: Some(0x32),
        length: 30,
        hex: "AA 55",
        status: "已解析",
        decoded: &packet_tree(),
    })
    .unwrap();
    let raw = rusqlite::Connection::open(&path).unwrap();
    raw.execute(
        "UPDATE packets SET decoded_json=?1",
        [b"MSPZ1invalid-zlib".as_slice()],
    )
    .unwrap();
    assert!(db.packets(&project.id, 0, 10, "").is_err());
    assert!(
        db.export_packets(&project.id, "json", &mut Vec::new())
            .is_err()
    );
}
