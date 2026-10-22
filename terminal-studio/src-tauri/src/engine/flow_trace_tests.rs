use super::{flow_trace::{self,FlowTrace}, state::EngineState, udp};
use crate::db;
use serde_json::{json,Value};
use sqlx::{SqlitePool,sqlite::SqlitePoolOptions};
use std::{sync::Arc,collections::HashMap,time::Duration};
use tokio::net::UdpSocket;

async fn fixture() -> (SqlitePool,Arc<EngineState>,UdpSocket,i64) {
    let pool=SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let peer=UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let local=UdpSocket::bind("127.0.0.1:0").await.unwrap();let port=local.local_addr().unwrap().port();drop(local);
    sqlx::query("INSERT INTO protocol_systems(id,name) VALUES(1,'trace test')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO protocol_groups(id,name,system_ref) VALUES(1,'test',1)").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO nodes(id,name,local_ip,local_port,remote_ip,remote_port,node_id_hex,group_bind_json) VALUES(1,'trace','127.0.0.1',?,'127.0.0.1',?,'01','[1]')").bind(port as i64).bind(peer.local_addr().unwrap().port() as i64).execute(&pool).await.unwrap();
    for (id,code,kind) in [("q","21","Request"),("a","22","Response"),("notice","41","Event")] {
        let mut fields=vec![json!({"name":"code","len":1,"enc":"hex","value":code,"role":"type_code"}),json!({"name":"seq","len":1,"enc":"hex","value":"01","role":"correlation"})];
        if id=="a" { fields.push(json!({"name":"result","len":1,"enc":"hex","value":"AA"})); }
        let p=serde_json::from_value(json!({"protocol_id":id,"name":id,"format":"hex","def_mode":"fields","msg_type":kind,"system_ref":1,"group_ref":1,"fields":fields})).unwrap();
        db::upsert_protocol(&pool,&p).await.unwrap();
    }
    let step=json!({"id":"s1","name":"请求与应答","protocol_id":"q","expect_protocol":"a","timeout_ms":1500,"next":null,"result_field":"result","success_hex":"AA"});
    set_steps(&pool,json!([step])).await;
    (pool,Arc::new(EngineState::new()),peer,port as i64)
}
async fn set_steps(pool:&SqlitePool, steps:Value){
    sqlx::query("INSERT INTO business_flows(id,name,system_ref,group_ref,steps_json) VALUES(1,'Trace',1,1,?) ON CONFLICT(id) DO UPDATE SET steps_json=excluded.steps_json").bind(steps.to_string()).execute(pool).await.unwrap();
}
async fn ended(engine:&EngineState,run:i64)->FlowTrace {
    tokio::time::timeout(Duration::from_secs(5),async {loop{let t=flow_trace::traces(engine,1).into_iter().find(|t|t.run_id==run).unwrap();if t.ended_at.is_some(){return t;}tokio::time::sleep(Duration::from_millis(20)).await;}}).await.unwrap()
}
async fn state(engine:&EngineState,run:i64,step:&str,status:&str){
    tokio::time::timeout(Duration::from_secs(3),async {loop{if flow_trace::traces(engine,1).iter().find(|t|t.run_id==run).unwrap().events.iter().any(|e|e.step_id==step&&e.status==status){break;}tokio::time::sleep(Duration::from_millis(10)).await;}}).await.unwrap();
}

#[tokio::test]
async fn auto_real_udp_ignores_foreign_peer_and_sequence_then_matches_fast_reply(){
    let (pool,engine,peer,_)=fixture().await;let app=tauri::test::mock_app();
    let responder=tokio::spawn(async move{let mut b=[0;64];let (n,src)=peer.recv_from(&mut b).await.unwrap();assert_eq!(&b[..n],&[0x21,1]);
        let foreign=UdpSocket::bind("127.0.0.1:0").await.unwrap();foreign.send_to(&[0x22,1,0xAA],src).await.unwrap();
        peer.send_to(&[0x22,2,0xAA],src).await.unwrap();
        peer.send_to(&[0x22,1,0xAA],src).await.unwrap();peer
    });
    let run=flow_trace::start(app.handle().clone(),pool.clone(),engine.clone(),1,vec![],Some(1),false).await.unwrap();
    let trace=ended(&engine,run).await;assert_eq!(trace.status,"done");
    assert!(trace.events.iter().any(|e|e.status=="unmatched"&&e.note=="其他对端"));
    assert!(trace.events.iter().any(|e|e.status=="unmatched"&&e.note=="关联字段不符"));
    assert_eq!(trace.events.iter().filter(|e|e.status=="success").count(),1);
    assert!(trace.events.iter().any(|e|e.status=="sent"&&e.packet.is_some()));
    let _peer=responder.await.unwrap();udp::stop_server(app.handle(),&engine,1).await;
}

#[tokio::test]
async fn manual_watch_sends_nothing_and_tracks_send_receive_and_passive_step(){
    let (pool,engine,peer,port)=fixture().await;let app=tauri::test::mock_app();
    set_steps(&pool,json!([
        {"id":"s1","name":"请求","protocol_id":"q","expect_protocol":"a","timeout_ms":2000,"on_success":"s2","next":null,"result_field":"result","success_hex":"AA"},
        {"id":"s2","name":"等通知","action":"receive","protocol_id":"notice","timeout_ms":2000,"next":null}
    ])).await;
    let run=flow_trace::start(app.handle().clone(),pool.clone(),engine.clone(),1,vec![],Some(1),true).await.unwrap();
    state(&engine,run,"s1","waiting_send").await;
    let mut b=[0;64];assert!(tokio::time::timeout(Duration::from_millis(80),peer.recv_from(&mut b)).await.is_err());
    assert!(flow_trace::start(app.handle().clone(),pool.clone(),engine.clone(),1,vec![],Some(1),true).await.is_err());
    let node=db::get_node(&pool,1).await.unwrap();let req=db::get_protocol(&pool,"q").await.unwrap();
    udp::send_datagram(app.handle(),&pool,&engine,&node,&req,&HashMap::new(),None).await.unwrap();
    peer.recv_from(&mut b).await.unwrap();peer.send_to(&[0x22,1,0xAA],format!("127.0.0.1:{port}")).await.unwrap();
    state(&engine,run,"s2","waiting").await;
    peer.send_to(&[0x41,1],format!("127.0.0.1:{port}")).await.unwrap();
    let trace=ended(&engine,run).await;assert_eq!(trace.status,"done");assert!(trace.manual);
    assert!(trace.events.iter().any(|e|e.next_step.as_deref()==Some("s2")));
    assert_eq!(trace.events.iter().filter(|e|e.status=="success").count(),2);
    udp::stop_server(app.handle(),&engine,1).await;
}

#[tokio::test]
async fn timeout_branch_is_recorded_and_run_is_not_success(){
    let (pool,engine,_peer,_)=fixture().await;let app=tauri::test::mock_app();
    set_steps(&pool,json!([
        {"id":"s1","name":"等应答","protocol_id":"q","expect_protocol":"a","timeout_ms":200,"on_timeout":"s2","next":null},
        {"id":"s2","name":"补发","protocol_id":"q","timeout_ms":200,"next":null}
    ])).await;
    let run=flow_trace::start(app.handle().clone(),pool.clone(),engine.clone(),1,vec![],Some(1),false).await.unwrap();
    let trace=ended(&engine,run).await;assert_eq!(trace.status,"issues");
    assert!(trace.events.iter().any(|e|e.branch.as_deref()==Some("on_timeout")&&e.next_step.as_deref()==Some("s2")));
    assert!(trace.events.iter().any(|e|e.step_id=="s2"&&e.status=="success"));
    udp::stop_server(app.handle(),&engine,1).await;
}

#[tokio::test]
async fn stop_preserves_trace_and_does_not_advance_after_late_reply(){
    let (pool,engine,peer,port)=fixture().await;let app=tauri::test::mock_app();
    let run=flow_trace::start(app.handle().clone(),pool.clone(),engine.clone(),1,vec![],Some(1),true).await.unwrap();
    state(&engine,run,"s1","waiting_send").await;assert!(flow_trace::stop(&engine,run));
    peer.send_to(&[0x22,1,0xAA],format!("127.0.0.1:{port}")).await.unwrap();
    tokio::time::sleep(Duration::from_millis(80)).await;
    let trace=ended(&engine,run).await;assert_eq!(trace.status,"aborted");assert!(!trace.events.iter().any(|e|e.status=="success"));assert!(!engine.flow_runs.contains_key(&run));
    udp::stop_server(app.handle(),&engine,1).await;
}

#[tokio::test]
async fn manual_does_not_reuse_ack_received_before_actual_send(){
    let (pool,engine,peer,port)=fixture().await;let app=tauri::test::mock_app();
    let run=flow_trace::start(app.handle().clone(),pool.clone(),engine.clone(),1,vec![],Some(1),true).await.unwrap();
    state(&engine,run,"s1","waiting_send").await;
    peer.send_to(&[0x22,1,0xAA],format!("127.0.0.1:{port}")).await.unwrap();
    state(&engine,run,"s1","unmatched").await;
    let node=db::get_node(&pool,1).await.unwrap();let req=db::get_protocol(&pool,"q").await.unwrap();
    udp::send_datagram(app.handle(),&pool,&engine,&node,&req,&HashMap::new(),None).await.unwrap();
    let trace=ended(&engine,run).await;assert_eq!(trace.status,"issues");
    assert!(!trace.events.iter().any(|e|e.status=="success"));
    udp::stop_server(app.handle(),&engine,1).await;
}

#[tokio::test]
async fn automatic_passive_step_keeps_immediately_following_notice(){
    let (pool,engine,peer,_)=fixture().await;let app=tauri::test::mock_app();
    set_steps(&pool,json!([
        {"id":"s1","name":"请求","protocol_id":"q","expect_protocol":"a","timeout_ms":2000,"on_success":"s2","next":null},
        {"id":"s2","name":"紧接通知","action":"receive","protocol_id":"notice","timeout_ms":400,"next":null}
    ])).await;
    let peer_task=tokio::spawn(async move{let mut b=[0;64];let (_,src)=peer.recv_from(&mut b).await.unwrap();peer.send_to(&[0x22,1,0xAA],src).await.unwrap();peer.send_to(&[0x41,1],src).await.unwrap();peer});
    let run=flow_trace::start(app.handle().clone(),pool.clone(),engine.clone(),1,vec![],Some(1),false).await.unwrap();
    let trace=ended(&engine,run).await;assert_eq!(trace.status,"done");assert!(trace.events.iter().any(|e|e.step_id=="s2"&&e.status=="success"));
    let _peer=peer_task.await.unwrap();udp::stop_server(app.handle(),&engine,1).await;
}

#[tokio::test]
async fn template_protocols_support_explicit_serial_tracking(){
    let (pool,engine,peer,_)=fixture().await;let app=tauri::test::mock_app();
    for (id,code,kind) in [("q","21","Request"),("a","22","Response")] {
        let p=serde_json::from_value(json!({"protocol_id":id,"name":id,"format":"hex","def_mode":"template","msg_type":kind,"system_ref":1,"group_ref":1,"fields":[],"payload_template":format!("{code} AA"),"match_rule":format!("^{code}")})).unwrap();db::upsert_protocol(&pool,&p).await.unwrap();
    }
    set_steps(&pool,json!([{ "id":"s1","name":"模板","protocol_id":"q","expect_protocol":"a","timeout_ms":1000,"next":null,"allow_unkeyed_reply":true }])).await;
    let peer_task=tokio::spawn(async move{let mut b=[0;64];let (_,src)=peer.recv_from(&mut b).await.unwrap();peer.send_to(&[0x22,0xAA],src).await.unwrap();peer});
    let run=flow_trace::start(app.handle().clone(),pool.clone(),engine.clone(),1,vec![],Some(1),false).await.unwrap();
    let trace=ended(&engine,run).await;assert_eq!(trace.status,"done");
    let _peer=peer_task.await.unwrap();udp::stop_server(app.handle(),&engine,1).await;
}

#[tokio::test]
async fn late_out_log_commit_does_not_hide_notice_between_ack_and_out(){
    use simterm_core::models::Direction;
    let (pool,engine,peer,_)=fixture().await;let app=tauri::test::mock_app();
    set_steps(&pool,json!([
        {"id":"s1","name":"请求","protocol_id":"q","expect_protocol":"a","timeout_ms":1000,"on_success":"s2","next":null},
        {"id":"s2","name":"通知","action":"receive","protocol_id":"notice","timeout_ms":400,"next":null}
    ])).await;
    let run=flow_trace::start(app.handle().clone(),pool.clone(),engine.clone(),1,vec![],Some(1),true).await.unwrap();
    state(&engine,run,"s1","waiting_send").await;
    let remote=peer.local_addr().unwrap().to_string();
    // Deterministic commit order for the race: UDP send began at watermark 0,
    // ACK and notice committed before the sender managed to commit its OUT log.
    for (direction,pid,hex,boundary) in [(Direction::In,"a","22 01 AA",None),(Direction::In,"notice","41 01",None),(Direction::Out,"q","21 01",Some(0))] {
        db::insert_log(&pool,direction,Some(1),"127.0.0.1:1",&remote,Some(pid),Some(hex),None,None,if direction==Direction::Out{"sent"}else{"ok"},boundary,"2026-10-22 10:00:00.000").await.unwrap();
    }
    let trace=ended(&engine,run).await;assert_eq!(trace.status,"done");
    assert!(trace.events.iter().any(|e|e.step_id=="s2"&&e.status=="success"));
    udp::stop_server(app.handle(),&engine,1).await;
}

#[tokio::test]
async fn each_device_partial_failure_stays_failed_on_graph(){
    let (pool,engine,peer,_)=fixture().await;let app=tauri::test::mock_app();
    let p=serde_json::from_value(json!({"protocol_id":"q","name":"q","format":"hex","def_mode":"fields","msg_type":"Request","system_ref":1,"group_ref":1,"fields":[{"name":"code","len":1,"enc":"hex","value":"21","role":"type_code"},{"name":"seq","len":1,"enc":"hex","value":"01","role":"correlation"},{"name":"device","len":1,"enc":"hex","value":"00","role":"src_id"}]})).unwrap();
    db::upsert_protocol(&pool,&p).await.unwrap();
    for (id,key) in [(1,"0A"),(2,"0B")] {sqlx::query("INSERT INTO terminal_profiles(id,name,node_ref,system_ref,protocol_ref,device_key,fields_json) VALUES(?, ?, 1,1,'q',?,'{}')").bind(id).bind(key).bind(key).execute(&pool).await.unwrap();}
    set_steps(&pool,json!([{ "id":"s1","name":"逐台","protocol_id":"q","expect_protocol":"a","timeout_ms":1000,"next":null,"terminal_mode":"each","result_field":"result","success_hex":"AA" }])).await;
    let peer_task=tokio::spawn(async move{let mut b=[0;64];for _ in 0..2{let (n,src)=peer.recv_from(&mut b).await.unwrap();assert_eq!(n,3);peer.send_to(&[0x22,b[1],if b[2]==0x0A{0}else{0xAA}],src).await.unwrap();}peer});
    let run=flow_trace::start(app.handle().clone(),pool.clone(),engine.clone(),1,vec![1,2],Some(1),false).await.unwrap();
    let trace=ended(&engine,run).await;assert_eq!(trace.status,"issues");assert!(trace.events.iter().any(|e|e.status=="success"));assert!(trace.events.iter().any(|e|e.status=="failure"));assert_eq!(trace.step_states["s1"].status,"failure");
    let _peer=peer_task.await.unwrap();udp::stop_server(app.handle(),&engine,1).await;
}

#[tokio::test]
async fn stopping_channel_also_stops_observer(){
    let (pool,engine,_peer,_)=fixture().await;let app=tauri::test::mock_app();
    let run=flow_trace::start(app.handle().clone(),pool.clone(),engine.clone(),1,vec![],Some(1),true).await.unwrap();
    state(&engine,run,"s1","waiting_send").await;
    udp::stop_server(app.handle(),&engine,1).await;
    assert_eq!(ended(&engine,run).await.status,"aborted");
    assert!(!engine.flow_runs.contains_key(&run));assert!(!engine.sockets.contains_key(&1));
}
