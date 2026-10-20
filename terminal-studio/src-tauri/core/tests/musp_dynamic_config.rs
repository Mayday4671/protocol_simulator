use simterm_core::{
    fields::{dedupe_field_names, merge_header_specs, render_fields},
    models::{Node, NodeRole},
    render::{DictMap, DictSetLite, RenderCtx},
    xml::parse_protocols_xml,
};
use std::collections::HashMap;
fn rendered(id: &str, params: HashMap<String, String>) -> Result<Vec<u8>, String> {
    let parsed = parse_protocols_xml(include_str!(
        "../../../protocols/MUSP_v0.8_无人机联调_基座试录.xml"
    ))
    .unwrap();
    let p = parsed
        .protocols
        .iter()
        .find(|p| p.protocol_id == id)
        .unwrap();
    let h = &parsed
        .group_headers
        .iter()
        .find(|h| Some(&h.group) == p.group.as_ref())
        .unwrap()
        .fields;
    let (mut fields, body) = merge_header_specs(h, &p.fields).unwrap();
    fields.extend(body);
    dedupe_field_names(&mut fields);
    let n = Node {
        id: 1,
        name: "test".into(),
        role: NodeRole::Simulator,
        local_ip: "127.0.0.1".into(),
        local_port: 1,
        remote_ip: "127.0.0.1".into(),
        remote_port: 2,
        node_id_hex: None,
        session_hex: None,
        captures_json: None,
        group_bind_json: None,
        enabled: true,
        created_at: String::new(),
        updated_at: String::new(),
    };
    let mut dicts = DictMap::new();
    for d in &parsed.dicts {
        dicts.insert(DictSetLite {
            key: d.key.clone(),
            name: d.name.clone(),
            byte_len: d.items[0].0.len() / 2,
            items: d.items.clone(),
        });
    }
    let c = RenderCtx::new(&n, 101, &params, &dicts);
    let first = render_fields(&fields, &c).map_err(|e| e.to_string())?;
    render_fields(&fields, &c.with_tlen(first.len() as u64)).map_err(|e| e.to_string())
}
fn crc(b: &[u8]) -> u16 {
    let mut c = 0xffffu16;
    for x in b {
        c ^= (*x as u16) << 8;
        for _ in 0..8 {
            c = if c & 0x8000 != 0 {
                (c << 1) ^ 0x1021
            } else {
                c << 1
            };
        }
    }
    c
}
#[test]
fn counts_control_packet_length_crc_and_only_selected_records() {
    let gold: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../docs/06_示例与测试数据/golden-vectors.json"
    ))
    .unwrap();
    let g = hex::decode(gold["request"]["hex"].as_str().unwrap().replace(' ', "")).unwrap();
    for count in [0usize, 1, 5] {
        let params = HashMap::from([
            ("配置条目数".into(), format!("{count:02X}")),
            ("通道配置条目".into(), hex::encode(&g[47..47 + count * 10])),
        ]);
        let b = rendered("musp_config_set", params).unwrap();
        assert_eq!(b.len(), 49 + count * 10);
        assert_eq!(&b[21..23], &((18 + count * 10) as u16).to_be_bytes());
        assert_eq!(b[3], 29);
        assert_eq!(&b[4..6], &(b.len() as u16).to_be_bytes());
        assert_eq!(&b[47..b.len() - 2], &g[47..47 + count * 10]);
        assert_eq!(
            crc(&b[..b.len() - 2]),
            u16::from_be_bytes(b[b.len() - 2..].try_into().unwrap())
        );
        let r = rendered(
            "musp_config_result",
            HashMap::from([
                ("配置条目数".into(), format!("{count:02X}")),
                ("配置结果条目".into(), "AA".repeat(count)),
            ]),
        )
        .unwrap();
        assert_eq!(r.len(), 26 + count);
        assert_eq!(
            crc(&r[..r.len() - 2]),
            u16::from_be_bytes(r[r.len() - 2..].try_into().unwrap())
        );
        assert_eq!(r[3], 23);
        assert_eq!(&r[4..6], &(r.len() as u16).to_be_bytes());
        assert_eq!(&r[21..23], &((1 + count) as u16).to_be_bytes());
    }
}
#[test]
fn count_and_encoded_rows_cannot_disagree() {
    assert!(rendered(
        "musp_config_set",
        HashMap::from([
            ("配置条目数".into(), "01".into()),
            ("通道配置条目".into(), "00".repeat(50))
        ])
    )
    .is_err());
}
#[test]
fn mode_one_uses_device_layout() {
    for count in [0usize, 1, 5] {
        let rows = "00003B010000003B0001".repeat(count);
        let b = rendered(
            "musp_config_set",
            HashMap::from([
                ("配置模式".into(), "01".into()),
                ("配置条目数".into(), format!("{count:02X}")),
                ("设备配置条目".into(), rows.clone()),
            ]),
        )
        .unwrap();
        assert_eq!(b.len(), 49 + count * 10);
        assert_eq!(&b[41..44], &[0, 0, 0]);
        assert_eq!(&b[47..b.len() - 2], hex::decode(rows).unwrap());
        assert_eq!(
            crc(&b[..b.len() - 2]),
            u16::from_be_bytes(b[b.len() - 2..].try_into().unwrap())
        );
    }
}
