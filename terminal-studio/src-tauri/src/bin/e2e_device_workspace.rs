use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};
use simterm_core::{fields::{extract_field_values, render_fields, req_ref_name}, models::{Node, Protocol}, render::{compact_hex, RenderCtx}};
use simulated_terminal_lib::{db, devices::{self, TerminalProfile}, engine::{state::EngineState,udp}};
use sqlx::{SqlitePool, sqlite::{SqlitePoolOptions, SqliteConnectOptions}};
use tokio::net::UdpSocket;
type Result<T> = std::result::Result<T,Box<dyn std::error::Error+Send+Sync>>;
type Values = HashMap<String,String>;

async fn profile(pool:&SqlitePool,id:i64)->Result<TerminalProfile> {
    Ok(sqlx::query_as("SELECT * FROM terminal_profiles WHERE id=?").bind(id).fetch_one(pool).await?)
}
async fn properties(pool:&SqlitePool,id:i64)->Result<Values> { Ok(serde_json::from_str(&profile(pool,id).await?.fields_json)?) }

async fn sent(app:&tauri::AppHandle<tauri::test::MockRuntime>,pool:&SqlitePool,engine:&Arc<EngineState>,node:&Node,p:&Protocol,peer:&UdpSocket,id:i64,extra:Values)->Result<(Vec<u8>,std::net::SocketAddr,Values)> {
    let mut params=extra;params.insert("__terminal_id".into(),id.to_string());
    udp::send_datagram(app,pool,engine,node,p,&params,None).await?;
    let mut buf=vec![0u8;65535];
    let (n,from)=tokio::time::timeout(Duration::from_secs(3),peer.recv_from(&mut buf)).await??;
    let fields=devices::fields_for(pool,p).await?;
    let dicts=db::dict_map_for_group(pool,p.group_ref).await?;
    Ok((buf[..n].to_vec(),from,extract_field_values(&fields,&buf[..n],&dicts)))
}

async fn ack(pool:&SqlitePool,node:&Node,peer:&UdpSocket,p:&Protocol,to:std::net::SocketAddr,request:&Values,extra:Values)->Result<Vec<u8>> {
    let response=db::get_protocol(pool,p.auto_respond.as_ref().unwrap()).await?;
    let fields=devices::fields_for(pool,&response).await?;
    let dicts=db::dict_map_for_group(pool,response.group_ref).await?;
    let mut params=HashMap::new();
    for f in &fields { if let Some(name)=req_ref_name(&f.value) { params.insert(f.name.clone(),request[name].clone()); } }
    params.extend(extra);
    let first=render_fields(&fields,&RenderCtx::new(node,0,&params,&dicts))?;
    let bytes=render_fields(&fields,&RenderCtx::new(node,0,&params,&dicts).with_tlen(first.len() as u64))?;
    let wire=simterm_core::rules::normalize_hex(&bytes);
    let before:i64=sqlx::query_scalar("SELECT count(*) FROM logs WHERE node_ref=? AND direction='in' AND raw_hex=?").bind(node.id).bind(&wire).fetch_one(pool).await?;
    peer.send_to(&bytes,to).await?;
    tokio::time::timeout(Duration::from_secs(5),async {
        loop {
            let count:i64=sqlx::query_scalar("SELECT count(*) FROM logs WHERE node_ref=? AND direction='in' AND raw_hex=?").bind(node.id).bind(&wire).fetch_one(pool).await?;
            if count>before {break;}
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
        Ok::<_,sqlx::Error>(())
    }).await??;
    Ok(bytes)
}

async fn exercise(pool:&SqlitePool)->Result<()> {
    let peer=UdpSocket::bind("127.0.0.1:0").await?;
    let reservation=std::net::UdpSocket::bind("127.0.0.1:0")?;
    let port=reservation.local_addr()?.port();drop(reservation);
    let id:i64=sqlx::query_scalar("INSERT INTO nodes(name,local_ip,local_port,remote_ip,remote_port,node_id_hex) VALUES ('Device workflow E2E','127.0.0.1',?,'127.0.0.1',?,'01000001') RETURNING id").bind(port as i64).bind(peer.local_addr()?.port() as i64).fetch_one(pool).await?;
    let node=db::get_node(pool,id).await?;
    let app=tauri::test::mock_app();let handle=app.handle();let engine=Arc::new(EngineState::new());
    let result=async {
        for family in ["env","access","gateway"] {
            let config=db::get_protocol(pool,&format!("iot2_{family}_write_req")).await?;
            let register=db::get_protocol(pool,&format!("iot2_{family}_register_req")).await?;
            let mut ids=Vec::new();
            for (suffix,zone) in [(1,"000A"),(2,"000B")] {
                devices::save_profile(pool,TerminalProfile{id:0,name:format!("{family}-{suffix}"),node_ref:id,system_ref:config.system_ref.unwrap(),protocol_ref:config.protocol_id.clone(),device_key:format!("010000{suffix:02X}"),fields_json:serde_json::to_string(&Values::from([("zone".into(),zone.into())]))?,session_hex:Some("DEADBEEF".into())}).await?;
                let pid:i64=sqlx::query_scalar("SELECT id FROM terminal_profiles WHERE node_ref=? AND system_ref=? AND device_key=?").bind(id).bind(config.system_ref).bind(format!("010000{suffix:02X}")).fetch_one(pool).await?;
                assert!(profile(pool,pid).await?.session_hex.is_none(),"client must not write Session");ids.push(pid);
            }
            let (_,to,ra)=sent(handle,pool,&engine,&node,&register,&peer,ids[0],Values::new()).await?;
            let (_,_,rb)=sent(handle,pool,&engine,&node,&register,&peer,ids[1],Values::new()).await?;
            ack(pool,&node,&peer,&register,to,&rb,Values::from([("会话".into(),"22222222".into())])).await?;
            ack(pool,&node,&peer,&register,to,&ra,Values::from([("会话".into(),"11111111".into())])).await?;
            assert_eq!(profile(pool,ids[0]).await?.session_hex.as_deref(),Some("11111111"));
            assert_eq!(profile(pool,ids[1]).await?.session_hex.as_deref(),Some("22222222"));
            let (_,_,a)=sent(handle,pool,&engine,&node,&config,&peer,ids[0],Values::from([("部署区域".into(),"000C".into()),("会话".into(),"BAD0BAD0".into())])).await?;
            assert_eq!(compact_hex(&a["消息码"]),"12");assert_eq!(compact_hex(&a["业务域"]),"02");
            assert_eq!(compact_hex(&a["会话"]),"11111111");assert_eq!(compact_hex(&a["源终端"]),"01000001");
            assert_eq!(properties(pool,ids[0]).await?["zone"],"000A","not persisted before ACK");
            let (_,_,b)=sent(handle,pool,&engine,&node,&config,&peer,ids[1],Values::from([("部署区域".into(),"000D".into())])).await?;
            ack(pool,&node,&peer,&config,to,&a,Values::from([("序号".into(),"EEEE".into())])).await?;
            assert_eq!(properties(pool,ids[0]).await?["zone"],"000A","wrong sequence persisted");
            ack(pool,&node,&peer,&config,to,&a,Values::from([("目标终端".into(),"01000003".into())])).await?;
            assert_eq!(properties(pool,ids[0]).await?["zone"],"000A","wrong device persisted");
            let bogus:i64=sqlx::query_scalar("SELECT count(*) FROM terminal_profiles WHERE node_ref=? AND system_ref=? AND device_key='01000003'").bind(id).bind(config.system_ref).fetch_one(pool).await?;
            assert_eq!(bogus,0,"unmatched configuration ACK created a phantom Session profile");
            ack(pool,&node,&peer,&config,to,&b,Values::new()).await?;
            let replay=ack(pool,&node,&peer,&config,to,&a,Values::new()).await?;
            assert_eq!(properties(pool,ids[0]).await?["zone"],"000C");
            assert_eq!(properties(pool,ids[1]).await?["zone"],"000D");
            let (_,_,failed)=sent(handle,pool,&engine,&node,&config,&peer,ids[0],Values::from([("部署区域".into(),"000E".into())])).await?;
            ack(pool,&node,&peer,&config,to,&failed,Values::from([("结果".into(),"01".into())])).await?;
            assert_eq!(properties(pool,ids[0]).await?["zone"],"000C","failed ACK persisted");
            let (_,_,older)=sent(handle,pool,&engine,&node,&config,&peer,ids[0],Values::from([("部署区域".into(),"000F".into())])).await?;
            let (_,_,newer)=sent(handle,pool,&engine,&node,&config,&peer,ids[0],Values::from([("部署区域".into(),"0010".into())])).await?;
            ack(pool,&node,&peer,&config,to,&newer,Values::new()).await?;
            ack(pool,&node,&peer,&config,to,&older,Values::new()).await?;
            assert_eq!(properties(pool,ids[0]).await?["zone"],"0010","late older ACK rolled back newer configuration");
            peer.send_to(&replay,to).await?;tokio::time::sleep(Duration::from_millis(90)).await;
            assert_eq!(properties(pool,ids[0]).await?["zone"],"0010","replay persisted");
            let (_,_,expired)=sent(handle,pool,&engine,&node,&config,&peer,ids[0],Values::from([("部署区域".into(),"0011".into())])).await?;
            for mut pending in engine.pending_configs.iter_mut() {pending.sent_at=std::time::Instant::now()-Duration::from_secs(301);}
            ack(pool,&node,&peer,&config,to,&expired,Values::new()).await?;
            assert_eq!(properties(pool,ids[0]).await?["zone"],"0010","expired ACK persisted");
            let mut edit=profile(pool,ids[0]).await?;edit.name="已编辑".into();edit.session_hex=Some("FFFFFFFF".into());devices::save_profile(pool,edit).await?;
            assert_eq!(profile(pool,ids[0]).await?.session_hex.as_deref(),Some("11111111"));
            println!("PASS {family}: profile defaults/lists, A/B registration, mapped send, ACK device/sequence/failure/replay/expiry/reordering, protected Session");
        }
        Ok::<_,Box<dyn std::error::Error+Send+Sync>>(())
    }.await;
    udp::stop_server(handle,&engine,id);
    sqlx::query("DELETE FROM nodes WHERE id=? AND name='Device workflow E2E'").bind(id).execute(pool).await?;
    result
}

#[tokio::main]
async fn main()->Result<()> {
    let path=PathBuf::from(std::env::args().nth(1).ok_or("explicit database required")?).canonicalize()?;
    let artifact=PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../artifacts").canonicalize()?;
    if path.parent()!=Some(artifact.as_path()) {return Err("Test requires a project artifact database".into());}
    let pool=SqlitePoolOptions::new().max_connections(3).connect_with(SqliteConnectOptions::new().filename(path).foreign_keys(true)).await?;
    let result=exercise(&pool).await;pool.close().await;result
}
