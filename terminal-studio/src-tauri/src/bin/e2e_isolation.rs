//! Self-contained localhost UDP regression. No production database or external peer required.
use simulated_terminal_lib::{db, EngineState};
use simulated_terminal_lib::engine::{udp,heartbeat};
use std::{collections::HashMap,sync::Arc,time::Duration};
use sqlx::sqlite::SqlitePoolOptions;

#[tokio::main]
async fn main() {
    let pool=SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let peer=tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let reservation=std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let local_port=reservation.local_addr().unwrap().port();drop(reservation);
    sqlx::query("INSERT INTO protocol_systems(id,name) VALUES(1,'A'),(2,'B')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO protocol_groups(id,system_ref,name) VALUES(1,1,'A'),(2,2,'B')").execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO nodes(name,role,local_ip,local_port,remote_ip,remote_port,node_id_hex,group_bind_json) VALUES('test','simulator','127.0.0.1',?,'127.0.0.1',?,'01','[1,2]')")
        .bind(local_port as i64).bind(peer.local_addr().unwrap().port() as i64).execute(&pool).await.unwrap();
    for (pid,system,code,role,kind,value) in [("reg_a",1,"81","dst_id","Response","0000"),("reg_b",2,"82","dst_id","Response","0000"),("push_b",1,"83","dst_id","Request","0000"),("biz_a",1,"11","src_id","Request","{session}"),("biz_b",2,"12","src_id","Request","{session}")] {
        let fields=serde_json::json!([
            {"name":"type","len":1,"enc":"hex","value":code,"role":"type_code"},
            {"name":"identity","len":1,"enc":"hex","value":"{node_id}","role":role},
            {"name":"session","len":2,"enc":"hex","value":value,"role":"session"}
        ]).to_string();
        sqlx::query("INSERT INTO protocols(protocol_id,name,format,msg_type,payload_template,def_mode,fields_json,system_ref,group_ref) VALUES(?,?,'hex',?,'','fields',?,?,?)")
            .bind(pid).bind(pid).bind(kind).bind(fields).bind(system).bind(system).execute(&pool).await.unwrap();
    }
    let app=tauri::test::mock_app(); let handle=app.handle().clone();let engine=Arc::new(EngineState::new());
    sqlx::query("UPDATE protocols SET auto_respond='biz_a' WHERE protocol_id='push_b'").execute(&pool).await.unwrap();
    let stale=db::get_node(&pool,1).await.unwrap();
    let socket=udp::ensure_server(&handle,&pool,&engine,&stale).await.unwrap();
    let address=socket.local_addr().unwrap();
    for packet in [[0x81,1,0xA1,0xA2],[0x81,2,0xB1,0xB2],[0x82,1,0xC1,0xC2]] {peer.send_to(&packet,address).await.unwrap();}
    for _ in 0..100 {
        let n:i64=sqlx::query_scalar("SELECT COUNT(*) FROM terminal_profiles").fetch_one(&pool).await.unwrap();
        if n==3 {break;} tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let n:i64=sqlx::query_scalar("SELECT COUNT(*) FROM terminal_profiles").fetch_one(&pool).await.unwrap();assert_eq!(n,3,"three independent terminal/system sessions persisted");
    let mut buffer=[0u8;32];
    for (pid,id,expected) in [("biz_a","01",[0x11,1,0xA1,0xA2]),("biz_a","02",[0x11,2,0xB1,0xB2]),("biz_a","03",[0x11,3,0,0]),("biz_b","01",[0x12,1,0xC1,0xC2]),("biz_a","01",[0x11,1,0xA1,0xA2])] {
        let protocol=db::get_protocol(&pool,pid).await.unwrap();
        let params=HashMap::from([("identity".to_string(),id.to_string())]);
        udp::send_datagram(&handle,&pool,&engine,&stale,&protocol,&params,None).await.unwrap();
        let (n,_)=tokio::time::timeout(Duration::from_secs(2),peer.recv_from(&mut buffer)).await.unwrap().unwrap();
        assert_eq!(&buffer[..n],&expected,"{pid} terminal {id}");
    }
    // An unsolicited incoming request addressed to B must reply using B's session, not the peer's or A's.
    peer.send_to(&[0x83,2,0,0],address).await.unwrap();
    let (n,_)=tokio::time::timeout(Duration::from_secs(2),peer.recv_from(&mut buffer)).await.unwrap().unwrap();
    assert_eq!(&buffer[..n],&[0x11,2,0xB1,0xB2]);
    // Re-registration must be visible to a task holding the old node snapshot.
    peer.send_to(&[0x81,1,0xD1,0xD2],address).await.unwrap();
    for _ in 0..100 {if db::get_node(&pool,1).await.unwrap().capture_buckets().get("s1:01").and_then(|b|b.get("__session")).map(String::as_str)==Some("D1D2"){break;}tokio::time::sleep(Duration::from_millis(10)).await;}
    heartbeat::start_periodic_send_with_params(handle.clone(),pool.clone(),engine.clone(),1,"biz_a",100,HashMap::from([("__device_key".into(),"01".into())])).await.unwrap();
    let (n,_)=tokio::time::timeout(Duration::from_secs(2),peer.recv_from(&mut buffer)).await.unwrap().unwrap();
    assert_eq!(&buffer[..n],&[0x11,1,0xD1,0xD2]);
    udp::stop_server(&handle,&engine,1).await;
    tokio::time::sleep(Duration::from_millis(150)).await;
    assert!(engine.periodic.is_empty() && engine.sockets.is_empty());
    println!("PASS: real UDP A/B registration, unregistered terminal, cross-system isolation, switch back, fresh periodic session, stop cleanup");
}
