//! Installed-binary verification; all observations originate in real UDP sockets.
use crate::application::Studio;
use serde_json::{Value, json};
use std::path::Path;
pub async fn run(directory: &Path) -> Result<Value, String> {
    let (app, rx) = Studio::open(directory)?;
    app.start_receiver(rx);
    app.start_scheduler();
    let result=async{
        let snap=app.request(json!({"action":"snapshot"})).await?;
        let protocol=snap["protocols"].as_array().ok_or("协议缺失")?.iter().find(|p|p["published"]==true).ok_or("没有发布协议")?["id"].as_str().ok_or("协议ID无效")?;
        let a=app.request(json!({"action":"endpoint_start","name":"安装自测A","bind":"127.0.0.1:0","protocolId":protocol})).await?;
        let b=app.request(json!({"action":"endpoint_start","name":"安装自测B","bind":"127.0.0.1:0","protocolId":protocol,"virtualPeer":true})).await?;
        let values=json!({"srcAddr":"00 00 01","dstAddr":"00 00 3B","transactionId":1,"sessionId":0,"body":{"configMode":1,"targetDeviceAddr":0,"items":[{"deviceAddr":59,"deviceType":1,"deviceNumericId":59,"initialParameter":1}]}});
        let flow=json!({"id":"installed-self-test","nameZh":"安装后真实UDP测试","protocol":{"id":protocol},"nodes":[{"id":"s","type":"START"},{"id":"request","type":"SEND_AND_WAIT","data":{"code":33,"expectedCodes":[34],"timeoutMs":1000,"values":values}},{"id":"end","type":"END"},{"id":"error","type":"ERROR"}],"edges":[{"source":"s","target":"request"},{"source":"request","target":"end","label":"matched"},{"source":"request","target":"error","label":"timeout"}]});
        app.request(json!({"action":"flow_save","flow":flow})).await?;app.request(json!({"action":"flow_publish","flowId":"installed-self-test"})).await?;
        let run=app.request(json!({"action":"flow_run","flowId":"installed-self-test","endpointId":a["id"],"remote":b["bind"]})).await?;
        let observation=tokio::time::timeout(std::time::Duration::from_secs(5),async{loop{
            let s=app.request(json!({"action":"snapshot"})).await?;
            if let Some(r)=s["runs"].as_array().unwrap().iter().find(|r|r["id"]==run["runId"]){if r["status"]=="COMPLETED"{break Ok::<_,String>(s);}else if r["status"]=="FAILED"{break Err(format!("自测流程失败: {}",r["failure"]));}}
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }}).await.map_err(|_|"安装自测UDP超时".to_string())??;
        if observation["devices"].as_array().unwrap().len()!=1{return Err("配置确认未产生预期设备".into());}
        let packets=app.request(json!({"action":"packet_query"})).await?;
        Ok(json!({"passed":true,"version":env!("CARGO_PKG_VERSION"),"checks":["SQLite迁移","内置XML编译","Rust真实UDP双端点","编码与CRC","响应唯一匹配","0x22业务提交","报文SQLite落库"],"stats":observation["stats"],"packetCount":packets["total"],"dataDirectory":directory}))
    }.await;
    app.shutdown.cancel();
    app.udp.stop_all().await;
    result
}
