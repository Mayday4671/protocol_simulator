use simterm_core::{fields::{merge_header_specs,render_fields}, models::{Node,NodeRole}, render::{DictMap,DictSetLite,RenderCtx}, xml::parse_protocols_xml};
use std::collections::HashMap;

fn crc(data:&[u8])->u16 { let mut c=0xffffu16; for &b in data {c^=(b as u16)<<8; for _ in 0..8 {c=if c&0x8000!=0 {(c<<1)^0x1021} else {c<<1};}} c }

#[test]
fn musp_trial_import_and_static_samples_match_base_renderer() {
    let parsed=parse_protocols_xml(include_str!("../../../protocols/MUSP_v0.8_无人机联调_基座试录.xml")).unwrap();
    assert!(parsed.failed.is_empty(),"{:?}",parsed.failed);
    assert_eq!(parsed.protocols.len(),34);
    let samples:HashMap<String,String>=serde_json::from_str(include_str!("../../../protocols/MUSP_v0.8_试录静态样例.json")).unwrap();
    let gold:serde_json::Value=serde_json::from_str(include_str!("../../../../docs/06_示例与测试数据/golden-vectors.json")).unwrap();
    let node=Node{id:1,name:"MUSP trial".into(),role:NodeRole::Simulator,local_ip:"127.0.0.1".into(),local_port:1,remote_ip:"127.0.0.1".into(),remote_port:2,node_id_hex:Some("00003B".into()),session_hex:None,captures_json:None,group_bind_json:None,enabled:true,created_at:String::new(),updated_at:String::new()};
    let mut dicts=DictMap::new();
    for d in &parsed.dicts {dicts.insert(DictSetLite{key:d.key.clone(),name:d.name.clone(),byte_len:d.items[0].0.len()/2,items:d.items.clone()});}
    let params=HashMap::new();
    for p in &parsed.protocols {
        let h=&parsed.group_headers.iter().find(|h|Some(&h.group)==p.group.as_ref()).unwrap().fields;
        let (mut fields,body)=merge_header_specs(h,&p.fields).unwrap(); fields.extend(body);
        let ctx=RenderCtx::new(&node,101,&params,&dicts);
        let first=render_fields(&fields,&ctx).unwrap_or_else(|e|panic!("{}: {}",p.protocol_id,e));
        let result=render_fields(&fields,&ctx.with_tlen(first.len() as u64)).unwrap();
        assert_eq!(hex::encode_upper(&result),samples[&p.protocol_id].replace(' ',""),"{}",p.protocol_id);
        assert_eq!(crc(&result[..result.len()-2]),u16::from_be_bytes(result[result.len()-2..].try_into().unwrap()));
        assert_eq!(result.len(),u16::from_be_bytes([result[4],result[5]]) as usize);
        assert_eq!(result.len(),result[3] as usize+u16::from_be_bytes([result[21],result[22]]) as usize+2);
        if p.protocol_id=="musp_config_set" || p.protocol_id=="musp_config_result" {
            let key=if p.protocol_id=="musp_config_set" {"request"} else {"response"};
            assert_eq!(hex::encode_upper(&result),gold[key]["hex"].as_str().unwrap().replace(' ',""));
        }
        assert!(p.auto_respond.is_none());
    }
}
