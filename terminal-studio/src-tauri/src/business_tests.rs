use crate::{business, db, devices, engine::state::EngineState};
use serde_json::json;
use simterm_core::{
    business::SaveRule,
    models::{Node, Protocol, ProtocolPayload},
    render::DictMap,
};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

async fn fixture() -> (SqlitePool, EngineState, Node, Protocol, Protocol, SaveRule) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    sqlx::query("INSERT INTO protocol_systems(id,name) VALUES(1,'Test')")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO nodes(id,name,local_ip,local_port,remote_ip,remote_port,node_id_hex) VALUES(1,'Test','127.0.0.1',39989,'127.0.0.1',39990,'01')").execute(&pool).await.unwrap();
    for (id, kind, code, unit) in [
        (
            "request",
            "Request",
            "21",
            json!([{"name":"id","len":1,"enc":"hex"},{"name":"device","len":1,"enc":"hex"}]),
        ),
        (
            "response",
            "Response",
            "22",
            json!([{"name":"id","len":1,"enc":"hex"},{"name":"result","len":1,"enc":"hex"}]),
        ),
    ] {
        let payload:ProtocolPayload=serde_json::from_value(json!({"protocol_id":id,"name":id,"format":"hex","msg_type":kind,"def_mode":"fields","system_ref":1,
            "match_fields":[{"field":"code","value":code}],"fields":[{"name":"code","len":1,"enc":"hex","value":code},{"name":"seq","len":1,"enc":"hex","value":"01"},{"name":"count","len":1,"enc":"hex","value":"02","role":"device_count"},{"name":"rows","len":2,"enc":"hex","role":"device_list","unit_fields":unit,"value":if id=="request"{"01010201"}else{"01AA0200"}}]})).unwrap();
        db::upsert_protocol(&pool, &payload).await.unwrap();
    }
    let r:SaveRule=serde_json::from_value(json!({"request":"request","response":"response","kind":"device","request_array":"rows","response_array":"rows","pairing":"key","request_match":"item.id","response_match":"item.id","request_sequence":"frame.seq","response_sequence":"frame.seq","serial":false,"timeout_ms":5000,"success_field":"item.result","success_hex":"AA","identity":"item.id","name_field":"","device_field":"","mappings":[{"property":"地址","source":"request","field":"item.id"},{"property":"结果","source":"response","field":"item.result"}]})).unwrap();
    let node = db::get_node(&pool, 1).await.unwrap();
    let req = db::get_protocol(&pool, "request").await.unwrap();
    let resp = db::get_protocol(&pool, "response").await.unwrap();
    (pool, EngineState::new(), node, req, resp, r)
}
async fn count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM business_records")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn dynamic_types_device_scope_delete_and_definition_guards() {
    use crate::record_types;
    use simterm_core::business::RecordType;
    let (pool, engine, node, req, resp, mut r) = fixture().await;
    let q = devices::fields_for(&pool, &req).await.unwrap();
    let a = devices::fields_for(&pool, &resp).await.unwrap();
    let peer = "127.0.0.1:39990".parse().unwrap();
    let dict = DictMap::new();
    for key in ["0A", "0B"] {
        sqlx::query("INSERT INTO terminal_profiles(name,node_ref,system_ref,protocol_ref,device_key,fields_json) VALUES(?,1,1,'request',?,'{}')").bind(key).bind(key).execute(&pool).await.unwrap();
    }
    let mut definition = RecordType { kind: "port".into(), name: "端口资料".into(), device_required: true, key_scope: "device".into(), properties: vec!["编号".into()] };
    record_types::save(&pool, 1, &definition).await.unwrap();
    r.kind = "port".into(); r.key_scope = "device".into(); r.device_field = "item.device".into(); r.pairing = "index".into();
    r.mappings = vec![simterm_core::business::Mapping { property: "编号".into(), source: "request".into(), field: "item.id".into() }];
    business::save(&pool, &r).await.unwrap();
    business::track(&pool, &engine, &node, &req, &q, &[0x21,1,2,1,10,1,11], &dict, peer).await.unwrap();
    business::accept(&pool, &engine, &node, &resp, &a, &[0x22,1,2,1,0xAA,1,0xAA], &dict, peer).await.unwrap();
    assert_eq!(count(&pool).await, 2);
    let scopes: Vec<String> = sqlx::query_scalar("SELECT scope_key FROM business_records ORDER BY scope_key").fetch_all(&pool).await.unwrap();
    assert_eq!(scopes, vec!["0A", "0B"]);
    let mut changed = definition.clone(); changed.device_required = false; changed.key_scope = "channel".into();
    assert!(record_types::save(&pool, 1, &changed).await.unwrap_err().to_string().contains("引用"));
    assert!(record_types::delete(&pool, 1, "port").await.unwrap_err().to_string().contains("规则"));
    definition.properties.clear();
    assert!(record_types::save(&pool, 1, &definition).await.is_err());
    definition.properties.push("编号".into());
    r.operation = "delete".into(); r.mappings.clear();
    business::save(&pool, &r).await.unwrap();
    // Failed response cannot delete either device's same-numbered port.
    business::track(&pool, &engine, &node, &req, &q, &[0x21,2,1,1,10], &dict, peer).await.unwrap();
    business::accept(&pool, &engine, &node, &resp, &a, &[0x22,2,1,1,0], &dict, peer).await.unwrap();
    assert_eq!(count(&pool).await, 2);
    business::track(&pool, &engine, &node, &req, &q, &[0x21,3,1,1,10], &dict, peer).await.unwrap();
    let note = business::accept(&pool, &engine, &node, &resp, &a, &[0x22,3,1,1,0xAA], &dict, peer).await.unwrap();
    assert!(note.contains("删除处理完成 1/1"));
    assert_eq!(count(&pool).await, 1);
    let scope: String = sqlx::query_scalar("SELECT scope_key FROM business_records").fetch_one(&pool).await.unwrap();
    assert_eq!(scope, "0B");
    // Definition edits while an ACK is pending invalidate the pending write.
    business::track(&pool, &engine, &node, &req, &q, &[0x21,4,1,1,11], &dict, peer).await.unwrap();
    definition.name = "端口".into();
    record_types::save(&pool, 1, &definition).await.unwrap();
    let note = business::accept(&pool, &engine, &node, &resp, &a, &[0x22,4,1,1,0xAA], &dict, peer).await.unwrap();
    assert!(note.contains("已变更"));
    assert_eq!(count(&pool).await, 1);
    sqlx::query("DELETE FROM business_save_rules").execute(&pool).await.unwrap();
    assert!(record_types::delete(&pool, 1, "port").await.unwrap_err().to_string().contains("1 条资料"));
    sqlx::query("DELETE FROM business_records").execute(&pool).await.unwrap();
    record_types::delete(&pool, 1, "port").await.unwrap();
    assert!(!record_types::list(&pool, 1).await.unwrap().iter().any(|t| t.kind == "port"));
}

#[tokio::test]
async fn type_xml_roundtrip_and_system_isolation() {
    use crate::record_types;
    use simterm_core::business::RecordType;
    let (pool, _, _, req, resp, r) = fixture().await;
    sqlx::query("INSERT INTO protocol_systems(id,name) VALUES(2,'Other')").execute(&pool).await.unwrap();
    let t = RecordType { kind: "sensor".into(), name: "温度<&传感器".into(), device_required: true, key_scope: "device".into(), properties: vec!["温度".into()] };
    record_types::save(&pool, 1, &t).await.unwrap();
    let mut other = t.clone(); other.properties = vec!["电压".into()];
    record_types::save(&pool, 2, &other).await.unwrap();
    assert_eq!(record_types::explicit(&pool, 1, "sensor").await.unwrap().unwrap(), t);
    let types = record_types::export(&pool, &[1], &[]).await.unwrap();
    let xml = simterm_core::xml::append_save_rules(simterm_core::xml::export_protocols_xml(&[req,resp], &[], &[], &[]), &[r]).unwrap();
    let xml = simterm_core::xml::append_record_types(xml, &types);
    let parsed = simterm_core::xml::parse_protocols_xml(&xml).unwrap();
    assert_eq!(parsed.record_types.len(), 1);
    assert_eq!(parsed.record_types[0].definition, t);
    assert_eq!(parsed.record_types[0].system, "Test");
    assert_eq!(parsed.save_rules[0].operation, "upsert");
}

#[tokio::test]
async fn legacy_type_inference_preserves_optional_links_and_long_properties() {
    use crate::record_types;
    let (pool, _, _, req, _, mut r) = fixture().await;
    r.kind = "legacy".into(); r.device_field = "item.device".into();
    r.mappings[0].property = "温".repeat(22);
    business::save(&pool, &r).await.unwrap();
    // Two existing protocols share a legacy kind; only one requires a device mapping.
    let payload: ProtocolPayload = serde_json::from_value(json!({"protocol_id":"zz_request","name":"second","format":"hex","msg_type":"Request","def_mode":"fields","system_ref":1,"fields":serde_json::from_str::<serde_json::Value>(&req.fields_json.unwrap()).unwrap()})).unwrap();
    db::upsert_protocol(&pool, &payload).await.unwrap();
    r.request = "zz_request".into(); r.device_field.clear();
    business::save(&pool, &r).await.unwrap();
    let t = record_types::list(&pool, 1).await.unwrap().into_iter().find(|t| t.kind == "legacy").unwrap();
    assert!(!t.device_required);
    record_types::save(&pool, 1, &t).await.unwrap();
    business::validate(&pool, &r).await.unwrap();
    let mut changed = t.clone(); changed.device_required = true;
    assert!(record_types::save(&pool, 1, &changed).await.unwrap_err().to_string().contains("引用"));
}

#[tokio::test]
async fn explicit_rule_partial_success_phone_link_and_replay() {
    let (pool, engine, node, req, resp, mut r) = fixture().await;
    let q = devices::fields_for(&pool, &req).await.unwrap();
    let a = devices::fields_for(&pool, &resp).await.unwrap();
    let peer = "127.0.0.1:39990".parse().unwrap();
    let dict = DictMap::new();
    let request = [0x21, 1, 2, 1, 1, 2, 1];
    let response = [0x22, 1, 2, 2, 0, 1, 0xAA];
    assert!(
        business::track(&pool, &engine, &node, &req, &q, &request, &dict, peer)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        business::accept(&pool, &engine, &node, &resp, &a, &response, &dict, peer)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(count(&pool).await, 0);
    business::save(&pool, &r).await.unwrap();
    business::track(&pool, &engine, &node, &req, &q, &request, &dict, peer)
        .await
        .unwrap();
    assert!(business::accept(
        &pool,
        &engine,
        &node,
        &resp,
        &a,
        &response,
        &dict,
        "127.0.0.1:39991".parse().unwrap()
    )
    .await
    .unwrap()
    .is_empty());
    let mut wrong = response;
    wrong[1] = 9;
    assert!(
        business::accept(&pool, &engine, &node, &resp, &a, &wrong, &dict, peer)
            .await
            .unwrap()
            .contains("流水号不匹配")
    );
    assert_eq!(count(&pool).await, 0);
    assert!(
        business::accept(&pool, &engine, &node, &resp, &a, &response, &dict, peer)
            .await
            .unwrap()
            .contains("1/2")
    );
    assert_eq!(count(&pool).await, 1);
    let device: i64 = sqlx::query_scalar("SELECT id FROM terminal_profiles WHERE device_key='01'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(
        business::accept(&pool, &engine, &node, &resp, &a, &response, &dict, peer)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        business::track(&pool, &engine, &node, &req, &q, &request, &dict, peer)
            .await
            .unwrap_err()
            .to_string()
            .contains("流水号")
    );
    // A later response updates only its explicitly mapped properties.
    sqlx::query("UPDATE terminal_profiles SET fields_json=? WHERE id=?")
        .bind(r#"{"地址":"FF","结果":"AA"}"#)
        .bind(device)
        .execute(&pool)
        .await
        .unwrap();
    r.mappings.retain(|m| m.property != "地址");
    business::save(&pool, &r).await.unwrap();
    business::track(
        &pool,
        &engine,
        &node,
        &req,
        &q,
        &[0x21, 5, 1, 1, 1],
        &dict,
        peer,
    )
    .await
    .unwrap();
    business::accept(
        &pool,
        &engine,
        &node,
        &resp,
        &a,
        &[0x22, 5, 1, 1, 0xAA],
        &dict,
        peer,
    )
    .await
    .unwrap();
    let values: String = sqlx::query_scalar("SELECT fields_json FROM terminal_profiles WHERE id=?")
        .bind(device)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&values).unwrap()["地址"],
        "FF"
    );
    r.kind = "phone".into();
    r.device_field = "item.device".into();
    business::save(&pool, &r).await.unwrap();
    let request = [0x21, 2, 1, 0x80, 1];
    let response = [0x22, 2, 1, 0x80, 0xAA];
    business::track(&pool, &engine, &node, &req, &q, &request, &dict, peer)
        .await
        .unwrap();
    assert!(
        business::accept(&pool, &engine, &node, &resp, &a, &response, &dict, peer)
            .await
            .unwrap()
            .contains("1/1")
    );
    let linked: i64 =
        sqlx::query_scalar("SELECT device_ref FROM business_records WHERE kind='phone'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(linked, device);
    assert_eq!(count(&pool).await, 2);
    // Missing parent device rolls back the whole transaction, including an earlier valid row.
    let request = [0x21, 3, 2, 0x81, 1, 0x82, 0xFF];
    let response = [0x22, 3, 2, 0x81, 0xAA, 0x82, 0xAA];
    business::track(&pool, &engine, &node, &req, &q, &request, &dict, peer)
        .await
        .unwrap();
    assert!(
        business::accept(&pool, &engine, &node, &resp, &a, &response, &dict, peer)
            .await
            .unwrap_err()
            .to_string()
            .contains("不存在")
    );
    assert_eq!(count(&pool).await, 2);
    // Channel deletion cascades both device and related phone without FK deadlock.
    sqlx::query("DELETE FROM nodes WHERE id=1")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(count(&pool).await, 0);
}

#[tokio::test]
async fn mismatch_timeout_and_xml_roundtrip() {
    let (pool, engine, node, req, resp, mut r) = fixture().await;
    let q = devices::fields_for(&pool, &req).await.unwrap();
    let a = devices::fields_for(&pool, &resp).await.unwrap();
    let dict = DictMap::new();
    let peer = "127.0.0.1:39990".parse().unwrap();
    r.serial = true;
    r.timeout_ms = 100;
    business::save(&pool, &r).await.unwrap();
    business::track(
        &pool,
        &engine,
        &node,
        &req,
        &q,
        &[0x21, 1, 2, 1, 1, 2, 1],
        &dict,
        peer,
    )
    .await
    .unwrap();
    assert!(business::track(
        &pool,
        &engine,
        &node,
        &req,
        &q,
        &[0x21, 2, 1, 1, 1],
        &dict,
        peer
    )
    .await
    .is_err());
    assert!(business::accept(
        &pool,
        &engine,
        &node,
        &resp,
        &a,
        &[0x22, 1, 1, 1, 0xAA],
        &dict,
        peer
    )
    .await
    .unwrap_err()
    .to_string()
    .contains("数量不一致"));
    tokio::time::sleep(std::time::Duration::from_millis(110)).await;
    assert!(business::accept(
        &pool,
        &engine,
        &node,
        &resp,
        &a,
        &[0x22, 1, 2, 1, 0xAA, 2, 0xAA],
        &dict,
        peer
    )
    .await
    .unwrap()
    .contains("超时"));
    assert_eq!(count(&pool).await, 0);
    r.mappings[0].property = "地址<&\"".into();
    let xml = simterm_core::xml::export_protocols_xml(&[req, resp], &[], &[], &[]);
    let xml = simterm_core::xml::append_save_rules(xml, &[r.clone()]).unwrap();
    let parsed = simterm_core::xml::parse_protocols_xml(&xml).unwrap();
    assert!(parsed.failed.is_empty(), "{:?}", parsed.failed);
    assert_eq!(parsed.save_rules, vec![r]);
}

#[cfg(feature = "e2e")]
#[tokio::test]
async fn real_udp_success_is_saved_only_with_rule() {
    use crate::engine::udp;
    use std::{collections::HashMap, sync::Arc, time::Duration};
    let (pool, engine, mut node, req, _resp, r) = fixture().await;
    let peer = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    node.remote_port = peer.local_addr().unwrap().port() as _;
    let reservation = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    node.local_port = reservation.local_addr().unwrap().port() as _;
    drop(reservation);
    sqlx::query("UPDATE nodes SET local_port=?,remote_port=? WHERE id=1")
        .bind(node.local_port)
        .bind(node.remote_port)
        .execute(&pool)
        .await
        .unwrap();
    let engine = Arc::new(engine);
    let app = tauri::test::mock_app();
    let handle = app.handle();
    business::save(&pool, &r).await.unwrap();
    udp::send_datagram(handle, &pool, &engine, &node, &req, &HashMap::new(), None)
        .await
        .unwrap();
    let mut buf = [0u8; 64];
    let (n, src) = tokio::time::timeout(Duration::from_secs(2), peer.recv_from(&mut buf))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(&buf[..n], &[0x21, 1, 2, 1, 1, 2, 1]);
    peer.send_to(&[0x22, 1, 2, 1, 0xAA, 2, 0], src)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while count(&pool).await != 1 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap();
    udp::stop_server(handle, &engine, node.id).await;
    assert!(engine.pending_saves.lock().await.is_empty());
}
