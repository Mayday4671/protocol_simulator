use serde_json::json;
use simterm_core::models::{Protocol, ProtocolMd};
use simterm_core::xml::{export_protocols_xml, parse_protocols_xml};
use simterm_core::{
    datagram::parse_field_details, fields::merge_header_specs, models::ProtocolField,
    render::DictMap,
};

fn specs(value: serde_json::Value) -> Vec<ProtocolField> {
    serde_json::from_value(value).unwrap()
}

fn protocol_row(p: &ProtocolMd) -> Protocol {
    Protocol {
        header_refs_json: p.header_refs.as_ref().map(|refs| serde_json::to_string(refs).unwrap()),
        id: 0,
        protocol_id: p.protocol_id.clone(),
        name: p.name.clone(),
        format: p.format,
        msg_type: p.msg_type,
        def_mode: p.def_mode,
        fields_json: Some(serde_json::to_string(&p.fields).unwrap()),
        payload_template: p.payload_template.clone(),
        match_rule: None,
        match_fields_json: None,
        auto_respond: None,
        respond_delay_ms: 0,
        auto_periodic: None,
        auto_periodic_interval_ms: 0,
        description: None,
        system_ref: None,
        group_ref: None,
        system_name: p.system.clone(),
        group_name: p.group.clone(),
        created_at: String::new(),
        updated_at: String::new(),
    }
}

#[test]
fn xml_preserves_field_and_unit_display_sections() {
    let xml = r#"<simterm-import><system name="S"><group name="G">
      <header><field name="Version" len="2" section="header" section-name="Common A"/></header>
      <protocol id="p" name="P">
        <field name="Records" len="2" value="{vlen}" section="header_extension" section-name="Extension B">
          <unit name="Flag" len="1"/>
          <unit name="Data" len="1" section="payload" section-name="Data area"/>
        </field>
        <field name="Check" len="4" section="trailer"/>
      </protocol>
    </group></system></simterm-import>"#;
    let parsed = parse_protocols_xml(xml).unwrap();
    assert!(parsed.failed.is_empty(), "{:?}", parsed.failed);
    let fields = serde_json::to_value(&parsed.protocols[0].fields).unwrap();
    assert_eq!(fields[0]["section"], "header_extension");
    assert_eq!(fields[0]["section_name"], "Extension B");
    assert_eq!(fields[0]["unit_fields"][1]["section"], "payload");
    assert_eq!(fields[0]["unit_fields"][1]["section_name"], "Data area");
    assert_eq!(fields[1]["section"], "trailer");
    let exported = export_protocols_xml(
        &[protocol_row(&parsed.protocols[0])],
        &[],
        &parsed.group_headers,
        &[],
    );
    let back = parse_protocols_xml(&exported).unwrap();
    assert_eq!(back.group_headers[0].fields, parsed.group_headers[0].fields);
    assert_eq!(back.protocols[0].fields, parsed.protocols[0].fields);
    assert!(exported.contains("section=\"header\""));
    assert!(exported.contains("section-name=\"Common A\""));
}

#[test]
fn xml_rejects_unknown_display_section() {
    let error = parse_protocols_xml(
        r#"<simterm-import><protocol id="bad">
      <field name="Wrong" len="1" section="guess_crc"/>
    </protocol></simterm-import>"#,
    )
    .unwrap_err()
    .to_string();
    assert!(
        error.contains("section") && error.contains("guess_crc"),
        "{error}"
    );
}

#[test]
fn detail_defaults_follow_origin_and_never_guess_names_or_checksums() {
    let fields = specs(json!([
        {"name":"CRC", "len":2, "enc":"hex"},
        {"name":"公共头", "len":1, "enc":"hex"}
    ]));
    let data = parse_field_details(&fields, 0, &[1, 2, 3, 4], &DictMap::new());
    assert_eq!(data.header_len, 0);
    assert_eq!(data.defined_len, 3);
    assert_eq!(
        data.fields
            .iter()
            .map(|f| f.section.as_str())
            .collect::<Vec<_>>(),
        ["payload", "payload", "unparsed"]
    );
    assert_eq!((data.fields[2].start, data.fields[2].len), (3, 1));
    assert!(data.fields.iter().all(|f| !f.is_header));
    let empty = parse_field_details(&[], 0, &[0xAA, 0x55], &DictMap::new());
    assert_eq!(empty.defined_len, 0);
    assert_eq!(empty.fields[0].section, "unparsed");
}

#[test]
fn detail_header_bytes_use_selected_fields_and_actual_lengths() {
    let fields = specs(json!([
        {"name":"Mode", "len":1, "enc":"uint"},
        {"name":"Optional", "len":5, "enc":"hex", "when":{"field":"Mode","value":"10"}},
        {"name":"A", "len":2, "enc":"hex", "section_name":"Common A"},
        {"name":"Content", "len":1, "enc":"hex", "section":"payload"},
        {"name":"B", "len":3, "enc":"hex", "section":"header_extension", "section_name":"Common B"},
        {"name":"End", "len":4, "enc":"hex", "section":"trailer"}
    ]));
    for (mode, header_len, total_len) in [(9, 6, 11), (10, 11, 16)] {
        let mut bytes = vec![0; total_len];
        bytes[0] = mode;
        let data = parse_field_details(&fields, 3, &bytes, &DictMap::new());
        assert_eq!(data.header_len, header_len);
        assert_eq!(data.defined_len, total_len);
        assert_eq!(
            data.fields
                .iter()
                .find(|f| f.field_name == "B")
                .unwrap()
                .section_name
                .as_deref(),
            Some("Common B")
        );
        assert_eq!(data.fields.last().unwrap().section, "trailer");
        assert_eq!(data.fields.last().unwrap().start, total_len - 4);
        assert!(data
            .fields
            .iter()
            .filter(|f| f.section == "header_extension")
            .all(|f| f.is_header));
    }
}

#[test]
fn detail_arrays_keep_zero_count_and_uint_ten_from_swallowing_tail() {
    let fields = specs(json!([
        {"name":"N", "len":1, "enc":"uint", "role":"device_count"},
        {"name":"Rows", "len":2, "enc":"hex", "value":"{vlen}", "section":"header_extension", "section_name":"Options", "unit_fields":[
          {"name":"Flag", "len":1, "enc":"hex"},
          {"name":"Value", "len":1, "enc":"uint", "section":"payload", "section_name":"Values"}
        ]},
        {"name":"Footer", "len":4, "enc":"hex", "section":"trailer"}
    ]));
    for count in [0usize, 1, 10] {
        let mut bytes = vec![count as u8];
        bytes.extend(vec![0x11; count * 2]);
        bytes.extend([0xDE, 0xAD, 0xBE, 0xEF]);
        let data = parse_field_details(&fields, 1, &bytes, &DictMap::new());
        assert_eq!(data.defined_len, bytes.len());
        assert_eq!(data.header_len, 1 + count);
        let tail = data.fields.last().unwrap();
        assert_eq!(
            (tail.start, tail.len, tail.hex.as_str()),
            (1 + count * 2, 4, "0xDEADBEEF")
        );
        assert_eq!(
            data.fields
                .iter()
                .filter(|f| f.record_index.is_some())
                .count(),
            count * 2
        );
        if count > 0 {
            let row = &data.fields[1];
            assert_eq!(
                (
                    row.field_name.as_str(),
                    row.record_name.as_deref(),
                    row.record_index
                ),
                ("Flag", Some("Rows"), Some(1))
            );
            assert_eq!(row.section_name.as_deref(), Some("Options"));
            assert_eq!(data.fields[2].section_name.as_deref(), Some("Values"));
            assert!(!data.fields[2].is_header);
        } else {
            assert_eq!(data.fields[1].len, 0);
        }
    }
}

#[test]
fn detail_truncated_counted_array_does_not_relabel_record_bytes_as_tail() {
    let fields = specs(json!([
        {"name":"N", "len":1, "enc":"uint", "role":"device_count"},
        {"name":"Rows", "len":3, "enc":"hex", "value":"{vlen}", "unit_fields":[
          {"name":"Kind", "len":1, "enc":"hex"}, {"name":"Value", "len":2, "enc":"uint"}
        ]},
        {"name":"Footer", "len":2, "enc":"hex", "section":"trailer"}
    ]));
    let bytes = [2, 0xA1, 0xB2, 0xCC, 0xDD];
    let data = parse_field_details(&fields, 0, &bytes, &DictMap::new());
    let tail = data.fields.last().unwrap();
    assert_eq!(
        (tail.start, tail.len, tail.expected_len, tail.hex.as_str()),
        (5, 0, 2, "—")
    );
    assert_eq!(data.defined_len, bytes.len());
    let partial_unit = data
        .fields
        .iter()
        .find(|field| field.field_name == "Value" && field.record_index == Some(2))
        .unwrap();
    assert_eq!((partial_unit.len, partial_unit.expected_len), (0, 2));
    let missing_records = data
        .fields
        .iter()
        .find(|field| field.meaning.starts_with("数组截断"))
        .unwrap();
    assert_eq!((missing_records.len, missing_records.expected_len), (0, 2));
    assert!(data
        .fields
        .iter()
        .any(|f| f.meaning.contains("截断") || f.meaning.contains("缺")));
    assert!(data.fields.iter().all(|f| f.start + f.len <= bytes.len()));
    assert_eq!(
        data.fields.iter().map(|f| f.len).sum::<usize>(),
        bytes.len()
    );
}

#[test]
fn detail_counted_array_preserves_declared_boundary_when_trailer_is_missing() {
    let fields = specs(json!([
        {"name":"N", "len":1, "enc":"uint", "role":"device_count"},
        {"name":"Rows", "len":2, "enc":"hex", "value":"{vlen}", "unit_fields":[
          {"name":"Value", "len":2, "enc":"hex"}
        ]},
        {"name":"Tail", "len":2, "enc":"hex", "section":"trailer"}
    ]));
    for bytes in [&[2, 0xAA, 0xBB, 0xCC, 0xDD][..], &[2, 0xAA, 0xBB, 0xCC][..]] {
        let data = parse_field_details(&fields, 0, bytes, &DictMap::new());
        let second = data
            .fields
            .iter()
            .find(|field| field.record_index == Some(2))
            .unwrap();
        assert_eq!(
            (second.start, second.len, second.expected_len),
            (3, bytes.len() - 3, 2)
        );
        let tail = data.fields.last().unwrap();
        assert_eq!(
            (tail.start, tail.len, tail.expected_len),
            (bytes.len(), 0, 2)
        );
        assert_eq!(tail.hex, "—");
        assert!(tail.meaning.contains("缺 2B"));
        assert_eq!(data.defined_len, bytes.len());
        assert_eq!(
            data.fields.iter().map(|field| field.len).sum::<usize>(),
            bytes.len()
        );
    }
}

#[test]
fn detail_fixed_truncation_never_invents_offsets_or_byte_lengths() {
    let fields = specs(json!([
        {"name":"Header", "len":4, "enc":"hex"},
        {"name":"Body", "len":2, "enc":"hex"},
        {"name":"End", "len":1, "enc":"hex", "section":"trailer"}
    ]));
    let data = parse_field_details(&fields, 1, &[0xAA, 0xBB], &DictMap::new());
    assert_eq!(data.header_len, 2);
    assert_eq!(data.defined_len, 2);
    assert_eq!(
        data.fields
            .iter()
            .map(|field| field.expected_len)
            .collect::<Vec<_>>(),
        [4, 2, 1]
    );
    assert_eq!(
        data.fields
            .iter()
            .map(|f| (f.start, f.len))
            .collect::<Vec<_>>(),
        [(0, 2), (2, 0), (2, 0)]
    );
    assert!(data.fields.iter().all(|f| f.meaning.contains("缺")));
}

#[test]
fn detail_unbounded_variable_header_reserves_declared_fixed_trailer() {
    let fields = specs(json!([
        {"name":"Options", "len":1, "enc":"hex", "value":"{vlen}", "section":"header_extension"},
        {"name":"Tail", "len":3, "enc":"hex", "section":"trailer"}
    ]));
    let data = parse_field_details(&fields, 0, &[1, 2, 3, 4, 5], &DictMap::new());
    assert_eq!(data.header_len, 2);
    assert_eq!(data.fields[0].len, 2);
    assert_eq!(
        (data.fields[1].start, data.fields[1].hex.as_str()),
        (2, "0x030405")
    );
}

#[test]
fn header_overrides_preserve_or_explicitly_replace_display_metadata() {
    let header = specs(
        json!([{"name":"Base", "len":2, "enc":"hex", "section":"header_extension", "section_name":"A"}]),
    );
    let body =
        specs(json!([{"name":"Override", "len":1, "enc":"hex", "hdr_ref":"Base", "value":"1234"}]));
    let (merged, body) = merge_header_specs(&header, &body).unwrap();
    assert!(body.is_empty());
    assert_eq!(merged[0].section, header[0].section);
    assert_eq!(merged[0].section_name, header[0].section_name);
    let body = specs(
        json!([{"name":"Override", "len":1, "enc":"hex", "hdr_ref":"Base", "section":"payload", "section_name":"B"}]),
    );
    let (merged, _) = merge_header_specs(&header, &body).unwrap();
    let data = parse_field_details(&merged, 1, &[1, 2], &DictMap::new());
    assert_eq!(data.header_len, 0);
    assert_eq!(data.fields[0].section, "payload");
    assert_eq!(data.fields[0].section_name.as_deref(), Some("B"));
}

#[test]
fn legacy_json_omits_display_metadata_and_rejects_unknown_section() {
    let legacy = specs(json!([{"name":"Byte", "len":1, "enc":"hex"}]));
    assert!(legacy[0].section.is_none());
    assert!(legacy[0].section_name.is_none());
    assert!(serde_json::from_value::<ProtocolField>(
        json!({"name":"Byte", "len":1, "enc":"hex", "section":"checksum_guess"})
    )
    .is_err());
}
