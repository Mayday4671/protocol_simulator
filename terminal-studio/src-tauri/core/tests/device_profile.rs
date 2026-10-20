use simterm_core::{models::ProtocolField, xml::parse_protocols_xml};
use std::collections::HashMap;
use simterm_core::{profile, models::{Node, NodeRole}, render::DictMap};

fn node() -> Node {
    Node { id: 1, name: "测试".into(), role: NodeRole::Simulator, local_ip: "127.0.0.1".into(), local_port: 0,
        remote_ip: "127.0.0.1".into(), remote_port: 0, node_id_hex: Some("00000001".into()), session_hex: None,
        captures_json: None, group_bind_json: None, enabled: true, created_at: String::new(), updated_at: String::new() }
}
fn fields() -> Vec<ProtocolField> {
    serde_json::from_str(r#"[
        {"name":"消息码","len":1,"enc":"hex","value":"12","role":"type_code"},
        {"name":"业务域","len":1,"enc":"hex","value":"02"},
        {"name":"模式","len":1,"enc":"hex","value":"01","profile_key":"mode"},
        {"name":"区域","len":2,"enc":"hex","value":"0001","profile_key":"zone","when":{"field":"模式","value":"01"}},
        {"name":"备用区域","len":2,"enc":"hex","value":"0002","profile_key":"backup_zone","when":{"field":"模式","value":"02"}},
        {"name":"条目数","len":1,"enc":"hex","value":"02","role":"device_count","profile_key":"count"},
        {"name":"条目","len":2,"enc":"hex","value":"{vlen}","role":"device_list","profile_key":"entries","unit_fields":[
            {"name":"端口","len":1,"enc":"hex","value":"01"},{"name":"标志","len":1,"enc":"hex","value":"80"}]}
    ]"#).unwrap()
}

#[test]
fn defaults_materialize_lists_and_only_active_mapped_properties() {
    let p = profile::normalize_properties(&fields(), &HashMap::new(), &node(), &DictMap::new()).unwrap();
    assert_eq!(p["entries"], "01800180");
    assert_eq!(p["count"], "02");
    assert_eq!(p["zone"], "0001");
    assert!(!p.contains_key("backup_zone"));
    assert!(!p.contains_key("业务域"));
}

#[test]
fn stable_keys_reuse_values_without_overwriting_command_identity() {
    let f = fields();
    let stored = HashMap::from([("zone".into(), "000F".into()), ("业务域".into(), "01".into())]);
    let mut renamed = f.clone(); renamed[3].name = "安装位置".into();
    let params = profile::params_for(&renamed, &stored).unwrap();
    assert_eq!(params["安装位置"], "000F");
    assert!(!params.contains_key("业务域"));
    assert!(!params.contains_key("消息码"));
}

#[test]
fn invalid_properties_width_and_list_count_are_rejected() {
    for p in [HashMap::from([("wrong".into(), "01".into())]),
        HashMap::from([("zone".into(), "01".into())]),
        HashMap::from([("entries".into(), "0102".into())]),
        HashMap::from([("zone".into(), "{seq}".into())])] {
        assert!(profile::normalize_properties(&fields(), &p, &node(), &DictMap::new()).is_err());
    }
    let zero = HashMap::from([("count".into(), "00".into()), ("entries".into(), "".into())]);
    assert_eq!(profile::normalize_properties(&fields(), &zero, &node(), &DictMap::new()).unwrap()["entries"], "");
}

#[test]
fn packet_runtime_identity_cannot_become_profile_property() {
    let mut f = fields(); f[0].profile_key = Some("message".into());
    assert!(profile::params_for(&f, &HashMap::new()).is_err());
}

#[test]
fn sending_without_list_overrides_uses_default_count_and_unit_values() {
    let node=node();let params=HashMap::new();let dicts=DictMap::new();
    let bytes=simterm_core::fields::render_fields(&fields(), &simterm_core::render::RenderCtx::new(&node,0,&params,&dicts)).unwrap();
    assert_eq!(hex::encode_upper(bytes),"12020100010201800180");
}

#[test]
fn parent_field_overrides_do_not_leak_into_same_named_list_columns() {
    let node=node();let params=HashMap::from([("模式".into(),"02".into())]);let dicts=DictMap::new();
    let mut f=fields(); f.last_mut().unwrap().unit_fields[0].name="模式".into();
    let bytes=simterm_core::fields::render_fields(&f, &simterm_core::render::RenderCtx::new(&node,0,&params,&dicts)).unwrap();
    assert!(hex::encode_upper(bytes).ends_with("01800180"));
}

#[test]
fn device_property_binding_survives_json_round_trip() {
    let f: ProtocolField = serde_json::from_str(r#"{"name":"安装区域","len":2,"enc":"hex","value":"0001","profile_key":"zone"}"#).unwrap();
    assert_eq!(serde_json::to_value(f).unwrap()["profile_key"], "zone");
}

#[test]
fn xml_keeps_device_property_binding_and_request_correlation() {
    let parsed = parse_protocols_xml(r#"<simterm-import><protocol id="configure" name="配置" type="Request"><field name="流水号" len="2" enc="hex" value="{seq}" role="correlation"/><field name="安装区域" len="2" enc="hex" value="0001" profile-key="zone"/></protocol></simterm-import>"#).unwrap();
    assert!(parsed.failed.is_empty());
    let fields = serde_json::to_value(&parsed.protocols[0].fields).unwrap();
    assert_eq!(fields[0]["role"], "correlation");
    assert_eq!(fields[1]["profile_key"], "zone");
}

fn system_union() -> (Vec<ProtocolField>, Vec<ProtocolField>, Node, DictMap) {
    let a: Vec<ProtocolField> = serde_json::from_str(r#"[
        {"name":"消息码","len":1,"enc":"hex","value":"11","role":"type_code"},
        {"name":"模式","len":1,"enc":"hex","value":"01","profile_key":"mode"},
        {"name":"安装区域","len":2,"enc":"hex","value":"0001","profile_key":"zone"}]"#).unwrap();
    let b: Vec<ProtocolField> = serde_json::from_str(r#"[
        {"name":"消息码","len":1,"enc":"hex","value":"22","role":"type_code"},
        {"name":"工作模式","len":1,"enc":"hex","value":"01","profile_key":"mode"},
        {"name":"上报地址","len":4,"enc":"hex","value":"0A000001","profile_key":"ip"}]"#).unwrap();
    (a, b, node(), DictMap::new())
}

#[test]
fn device_profile_covers_union_of_system_protocols() {
    let (a, b, n, dicts) = system_union();
    let schemas = [
        profile::ProfileSchema { protocol_name: "视频设备注册", fields: &a, dicts: &dicts },
        profile::ProfileSchema { protocol_name: "终端状态查询", fields: &b, dicts: &dicts },
    ];
    let out = profile::normalize_profile(&schemas, &HashMap::new(), &n).unwrap();
    assert_eq!(out["mode"], "01");
    assert_eq!(out["zone"], "0001");
    assert_eq!(out["ip"], "0A000001");
    // 同一 Key 两套协议各取一次值: 共享键只保留一份
    assert_eq!(out.len(), 3);
}

#[test]
fn shared_key_with_conflicting_defaults_is_an_explicit_error() {
    let (a, mut b, n, dicts) = system_union();
    b[1].value = "02".into();
    let schemas = [
        profile::ProfileSchema { protocol_name: "视频设备注册", fields: &a, dicts: &dicts },
        profile::ProfileSchema { protocol_name: "终端状态查询", fields: &b, dicts: &dicts },
    ];
    let err = profile::normalize_profile(&schemas, &HashMap::new(), &n).unwrap_err().to_string();
    assert!(err.contains("mode") && err.contains("不一致"), "{err}");
    // 显式填写后不再依赖默认值, 可以保存
    let filled = HashMap::from([("mode".into(), "01".into())]);
    assert!(profile::normalize_profile(&schemas, &filled, &n).is_ok());
}

#[test]
fn system_profile_rejects_keys_declared_by_no_protocol() {
    let (a, b, n, dicts) = system_union();
    let schemas = [
        profile::ProfileSchema { protocol_name: "A", fields: &a, dicts: &dicts },
        profile::ProfileSchema { protocol_name: "B", fields: &b, dicts: &dicts },
    ];
    let ghost = HashMap::from([("ghost".into(), "01".into())]);
    let err = profile::normalize_profile(&schemas, &ghost, &n).unwrap_err().to_string();
    assert!(err.contains("ghost"), "{err}");
}

#[test]
fn branch_inactive_key_survives_through_other_protocol() {
    let (a, _b, n, dicts) = system_union();
    // A 中 zone 挂在未激活分支; B(终端查询)无条件声明同一 Key
    let mut a2 = a.clone();
    a2[2].when = Some(serde_json::from_str(r#"{"field":"模式","value":"02"}"#).unwrap());
    let c: Vec<ProtocolField> = serde_json::from_str(r#"[{"name":"区域","len":2,"enc":"hex","value":"0002","profile_key":"zone"}]"#).unwrap();
    let schemas = [
        profile::ProfileSchema { protocol_name: "视频设备注册", fields: &a2, dicts: &dicts },
        profile::ProfileSchema { protocol_name: "智能设备上报", fields: &c, dicts: &dicts },
    ];
    let stored = HashMap::from([("zone".into(), "000F".into())]);
    let out = profile::normalize_profile(&schemas, &stored, &n).unwrap();
    assert_eq!(out["zone"], "000F");
}
