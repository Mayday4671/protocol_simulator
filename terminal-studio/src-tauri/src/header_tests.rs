use crate::db;
use serde_json::json;
use simterm_core::models::{GroupHeadersPayload, HeaderDefinition, ProtocolPayload};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

async fn database() -> SqlitePool {
    let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    sqlx::query("INSERT INTO protocol_systems(id,name) VALUES(1,'S'),(2,'Other')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO protocol_groups(id,system_ref,name) VALUES(1,1,'G'),(2,2,'Other')").execute(&pool).await.unwrap();
    pool
}

fn header(id: &str, field: &str, value: &str) -> HeaderDefinition {
    serde_json::from_value(json!({"id":id,"name":format!("头{id}"),"fields":[{"name":field,"len":1,"enc":"hex","value":value}]})).unwrap()
}

fn protocol(id: &str, refs: Option<Vec<&str>>) -> ProtocolPayload {
    serde_json::from_value(json!({"protocol_id":id,"name":id,"format":"hex","msg_type":"Request","def_mode":"fields","system_ref":1,"group_ref":1,
        "header_refs":refs,"fields":[{"name":"body","len":1,"enc":"hex","value":"CC"}]})).unwrap()
}

#[tokio::test]
async fn header_migration_preserves_legacy_columns_and_null_selection() {
    let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
    sqlx::raw_sql("CREATE TABLE protocol_groups(id INTEGER,header_enabled INTEGER,header_fields_json TEXT); CREATE TABLE protocols(protocol_id TEXT,fields_json TEXT); INSERT INTO protocol_groups VALUES(7,1,'[{\"name\":\"H\",\"len\":1}]'); INSERT INTO protocols VALUES('old','[]');").execute(&pool).await.unwrap();
    sqlx::raw_sql(include_str!("../migrations/018_header_definitions.sql")).execute(&pool).await.unwrap();
    let row: (i64, String, Option<String>) = sqlx::query_as("SELECT header_enabled,header_fields_json,header_profiles_json FROM protocol_groups WHERE id=7").fetch_one(&pool).await.unwrap();
    assert_eq!(row, (1,"[{\"name\":\"H\",\"len\":1}]".into(),None));
    let refs: Option<String> = sqlx::query_scalar("SELECT header_refs_json FROM protocols WHERE protocol_id='old'").fetch_one(&pool).await.unwrap();
    assert!(refs.is_none());
}

#[tokio::test]
async fn header_database_preserves_order_none_and_scope() {
    let pool = database().await;
    let headers = vec![header("A","Type","AA"),header("B","Type","BB")];
    db::update_group_headers(&pool,1,&GroupHeadersPayload{headers}).await.unwrap();
    for (id, refs, expected) in [
        ("a",Some(vec!["A"]),vec!["A::Type"]), ("b",Some(vec!["B"]),vec!["B::Type"]),
        ("ab",Some(vec!["A","B"]),vec!["A::Type","B::Type"]), ("ba",Some(vec!["B","A"]),vec!["B::Type","A::Type"]),
        ("none",Some(vec![]),vec![]), ("legacy",None,vec![])] {
        let payload = protocol(id,refs);
        let saved = db::upsert_protocol(&pool,&payload).await.unwrap();
        assert_eq!(saved.header_refs().unwrap(),payload.header_refs);
        let fields = db::protocol_headers(&pool,&saved).await.unwrap();
        assert_eq!(fields.iter().filter_map(|f|f.header_key.as_deref()).collect::<Vec<_>>(),expected);
    }
    for refs in [vec!["missing"], vec!["A","A"]] {
        assert!(db::upsert_protocol(&pool,&protocol("invalid",Some(refs))).await.is_err());
    }
    let mut foreign = protocol("foreign",Some(vec!["A"]));
    foreign.system_ref=Some(2);foreign.group_ref=Some(2);
    assert!(db::upsert_protocol(&pool,&foreign).await.unwrap_err().to_string().contains('A'));
    foreign.system_ref=Some(1);
    assert!(db::upsert_protocol(&pool,&foreign).await.unwrap_err().to_string().contains("系统"));
}

#[tokio::test]
async fn header_edits_preserve_existing_references_and_reject_ambiguity() {
    let pool = database().await;
    let a = header("A","Type","AA");
    let b = header("B","Other","BB");
    db::update_group_headers(&pool,1,&GroupHeadersPayload{headers:vec![a.clone(),b.clone()]}).await.unwrap();
    let mut payload=protocol("uses A and B",Some(vec!["A","B"]));
    payload.fields=serde_json::from_value(json!([{"name":"override","len":1,"enc":"hex","hdr_ref":"Type","value":"01"}])).unwrap();
    db::upsert_protocol(&pool,&payload).await.unwrap();
    for changed in [vec![b.clone()], vec![header("A","Renamed","AA"),b.clone()],vec![a.clone(),header("B","Type","BB")]] {
        let error=db::update_group_headers(&pool,1,&GroupHeadersPayload{headers:changed}).await.unwrap_err().to_string();
        assert!(error.contains("uses A and B"),"{error}");
        assert_eq!(simterm_core::headers::group_definitions(&db::get_group(&pool,1).await.unwrap()).unwrap(),vec![a.clone(),b.clone()]);
    }
}

#[tokio::test]
async fn inherited_legacy_header_cannot_be_deleted_until_protocol_opts_out() {
    let pool=database().await;
    sqlx::query("UPDATE protocol_groups SET header_enabled=1,header_fields_json=? WHERE id=1").bind(serde_json::to_string(&header("legacy-default","Type","AA").fields).unwrap()).execute(&pool).await.unwrap();
    let mut payload=protocol("old inherited",None);
    let saved=db::upsert_protocol(&pool,&payload).await.unwrap();
    assert_eq!(db::protocol_headers(&pool,&saved).await.unwrap()[0].value,"AA");
    let error=db::update_group_headers(&pool,1,&GroupHeadersPayload{headers:vec![]}).await.unwrap_err().to_string();
    assert!(error.contains("old inherited"),"{error}");
    payload.header_refs=Some(vec![]);
    db::upsert_protocol(&pool,&payload).await.unwrap();
    let group=db::update_group_headers(&pool,1,&GroupHeadersPayload{headers:vec![]}).await.unwrap();
    assert_eq!(group.header_profiles_json.as_deref(),Some("[]"));
    assert!(!group.header_enabled);
}

#[cfg(feature="e2e")]
#[tokio::test]
async fn header_combinations_real_udp_and_header_only_type_code_reply() {
    use crate::{engine::udp,EngineState};
    use std::{collections::HashMap,sync::Arc,time::Duration};
    let pool=database().await;
    let peer=tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let reservation=std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let port=reservation.local_addr().unwrap().port();drop(reservation);
    sqlx::query("INSERT INTO nodes(name,role,local_ip,local_port,remote_ip,remote_port,group_bind_json) VALUES('Headers','simulator','127.0.0.1',?,'127.0.0.1',?,'[1]')").bind(port as i64).bind(peer.local_addr().unwrap().port() as i64).execute(&pool).await.unwrap();
    let node=db::get_node(&pool,1).await.unwrap();
    let a=header("A","Type","AA");let b=header("B","Type","BB");
    db::update_group_headers(&pool,1,&GroupHeadersPayload{headers:vec![a,b]}).await.unwrap();
    let app=tauri::test::mock_app();let handle=app.handle();let engine=Arc::new(EngineState::new());
    let mut buffer=[0u8;64];
    for (id,refs,expected) in [("a",vec!["A"],vec![0xAA,0xCC]),("b",vec!["B"],vec![0xBB,0xCC]),("ab",vec!["A","B"],vec![0xAA,0xBB,0xCC]),("ba",vec!["B","A"],vec![0xBB,0xAA,0xCC]),("none",vec![],vec![0xCC])] {
        let saved=db::upsert_protocol(&pool,&protocol(id,Some(refs))).await.unwrap();
        udp::send_datagram(handle,&pool,&engine,&node,&saved,&HashMap::new(),None).await.unwrap();
        let (len,_)=tokio::time::timeout(Duration::from_secs(3),peer.recv_from(&mut buffer)).await.unwrap().unwrap();
        assert_eq!(&buffer[..len],expected,"{id}");
    }
    let mut type_header=header("R","RequestType","D1");
    type_header.fields[0].role=simterm_core::models::FieldRole::TypeCode;
    db::update_group_headers(&pool,1,&GroupHeadersPayload{headers:vec![header("A","Type","AA"),header("B","Type","BB"),type_header]}).await.unwrap();
    let mut incoming=protocol("request",Some(vec!["R"]));incoming.auto_respond=Some("ba".into());
    db::upsert_protocol(&pool,&incoming).await.unwrap();
    let socket=udp::ensure_server(handle,&pool,&engine,&node).await.unwrap();
    peer.send_to(&[0xD1,0xCC],socket.local_addr().unwrap()).await.unwrap();
    let (len,_)=tokio::time::timeout(Duration::from_secs(3),peer.recv_from(&mut buffer)).await.unwrap().unwrap();
    assert_eq!(&buffer[..len],&[0xBB,0xAA,0xCC]);
    let matched: String=sqlx::query_scalar("SELECT protocol_id FROM logs WHERE direction='in' ORDER BY id DESC LIMIT 1").fetch_one(&pool).await.unwrap();
    assert_eq!(matched,"request");
    udp::stop_server(handle,&engine,node.id);
}

#[tokio::test]
async fn xml_group_import_can_atomically_remove_headers_and_opt_protocol_out() {
    let pool=database().await;
    db::update_group_headers(&pool,1,&GroupHeadersPayload{headers:vec![header("A","Type","AA")]}).await.unwrap();
    db::upsert_protocol(&pool,&protocol("p",Some(vec!["A"]))).await.unwrap();
    let state=crate::AppState{pool:pool.clone(),engine:std::sync::Arc::new(crate::EngineState::new())};
    let xml=r#"<simterm-import><system name="S"><group name="G" header-profiles="[]"><protocol id="p" name="p" header-refs=""><field name="body" len="1" value="DD"/></protocol></group></system></simterm-import>"#;
    let result=crate::commands::protocols::import_xml_text(&state,xml.into()).await.unwrap();
    assert!(result.failed.is_empty(),"{:?}",result.failed);assert_eq!(result.updated,1);
    assert_eq!(db::get_group(&pool,1).await.unwrap().header_profiles_json.as_deref(),Some("[]"));
    let p=db::get_protocol(&pool,"p").await.unwrap();
    assert_eq!(p.header_refs().unwrap(),Some(vec![]));assert_eq!(p.field_specs()[0].value,"DD");
}

#[tokio::test]
async fn xml_group_import_retains_protection_for_protocols_not_in_the_file() {
    let pool=database().await;
    let original=vec![header("A","Type","AA")];
    db::update_group_headers(&pool,1,&GroupHeadersPayload{headers:original.clone()}).await.unwrap();
    db::upsert_protocol(&pool,&protocol("p",Some(vec!["A"]))).await.unwrap();
    db::upsert_protocol(&pool,&protocol("not imported",Some(vec!["A"]))).await.unwrap();
    let state=crate::AppState{pool:pool.clone(),engine:std::sync::Arc::new(crate::EngineState::new())};
    let xml=r#"<simterm-import><system name="S"><group name="G" header-profiles="[]"><protocol id="p" header-refs=""><field name="body" len="1" value="DD"/></protocol></group></system></simterm-import>"#;
    let result=crate::commands::protocols::import_xml_text(&state,xml.into()).await.unwrap();
    assert_eq!(result.updated,0);assert!(result.failed.join(" ").contains("not imported"));
    assert_eq!(simterm_core::headers::group_definitions(&db::get_group(&pool,1).await.unwrap()).unwrap(),original);
    let p=db::get_protocol(&pool,"p").await.unwrap();
    assert_eq!(p.header_refs().unwrap(),Some(vec!["A".into()]));assert_eq!(p.field_specs()[0].value,"CC");
}

#[tokio::test]
async fn xml_group_import_updates_header_and_qualified_override_together() {
    let pool=database().await;
    db::update_group_headers(&pool,1,&GroupHeadersPayload{headers:vec![header("A","Type","AA")]}).await.unwrap();
    let mut payload=protocol("p",Some(vec!["A"]));
    payload.fields=serde_json::from_value(json!([{"name":"override","len":1,"enc":"hex","hdr_ref":"A::Type","value":"01"}])).unwrap();
    db::upsert_protocol(&pool,&payload).await.unwrap();
    let state=crate::AppState{pool:pool.clone(),engine:std::sync::Arc::new(crate::EngineState::new())};
    let xml=r#"<simterm-import><system name="S"><group name="G"><header id="A"><field name="Renamed" len="1" value="AA"/></header><protocol id="p" header-refs="A"><field name="override" len="1" header-ref="A::Renamed" value="02"/></protocol></group></system></simterm-import>"#;
    let result=crate::commands::protocols::import_xml_text(&state,xml.into()).await.unwrap();
    assert!(result.failed.is_empty(),"{:?}",result.failed);
    let fields=crate::devices::fields_for(&pool,&db::get_protocol(&pool,"p").await.unwrap()).await.unwrap();
    assert_eq!(fields[0].name,"Renamed");assert_eq!(fields[0].value,"02");
}

#[tokio::test]
async fn xml_group_import_invalid_protocol_leaves_headers_and_other_protocol_unchanged() {
    let pool=database().await;
    let original=vec![header("A","Type","AA")];
    db::update_group_headers(&pool,1,&GroupHeadersPayload{headers:original.clone()}).await.unwrap();
    db::upsert_protocol(&pool,&protocol("p",Some(vec!["A"]))).await.unwrap();
    let state=crate::AppState{pool:pool.clone(),engine:std::sync::Arc::new(crate::EngineState::new())};
    let xml=r#"<simterm-import><system name="S"><group name="G"><header id="A"><field name="Type" len="1" value="BB"/></header><protocol id="p" header-refs="A"><field name="body" len="1" value="DD"/></protocol><protocol id="invalid" header-refs="A"><field name="bad" len="1" dict="missing" value="01"/></protocol></group></system></simterm-import>"#;
    let result=crate::commands::protocols::import_xml_text(&state,xml.into()).await.unwrap();
    assert!(!result.failed.is_empty());assert_eq!(result.updated,0);assert_eq!(result.imported,0);
    assert_eq!(simterm_core::headers::group_definitions(&db::get_group(&pool,1).await.unwrap()).unwrap(),original);
    assert_eq!(db::get_protocol(&pool,"p").await.unwrap().field_specs()[0].value,"CC");
    assert!(db::get_protocol(&pool,"invalid").await.is_err());
}
