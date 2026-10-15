use musp_studio::protocol::{compile_xml, crc16_ccitt_false, parse_hex, set_bits};
use serde_json::{Value, json};

const XML: &str = include_str!("../../../docs/06_示例与测试数据/musp-demo.xml");
fn golden(name: &str) -> Vec<u8> {
    let vectors: Value = serde_json::from_str(include_str!(
        "../../../docs/06_示例与测试数据/golden-vectors.json"
    ))
    .unwrap();
    parse_hex(vectors[name]["hex"].as_str().unwrap()).unwrap()
}

#[test]
fn supplied_demo_schema_is_a_valid_executable_protocol() {
    assert!(
        compile_xml(XML).is_ok(),
        "The supplied DSL contract must compile"
    );
}

#[test]
fn rejects_dtd_and_entity_declarations_with_location() {
    let xml = "<!DOCTYPE protocol [<!ENTITY x SYSTEM 'file:///secret'>]><protocol/>";
    let error = compile_xml(xml).expect_err("DTDs must be rejected");
    assert_eq!(error.code, "xmlSecurity");
    assert!(error.line.is_some());
}

#[test]
fn duplicate_field_keys_are_rejected_within_a_fragment() {
    let xml = XML.replace("key=\"version\"", "key=\"magic\"");
    assert_eq!(compile_xml(&xml).err().unwrap().code, "duplicateKey");
}

#[test]
fn arbitrary_function_calls_cannot_enter_length_expressions() {
    let xml = XML.replace("lengthExpr=\"tokenLength\"", "lengthExpr=\"system(1)\"");
    assert_eq!(compile_xml(&xml).err().unwrap().code, "expression");
}

#[test]
fn bit_ranges_cannot_overlap() {
    let xml = XML.replace(
        "key=\"end\" labelZh=\"结束标识（候选）\" start=\"0\"",
        "key=\"end\" labelZh=\"结束标识（候选）\" start=\"7\"",
    );
    assert_eq!(compile_xml(&xml).err().unwrap().code, "bitOverlap");
}

#[test]
fn cyclic_fragment_references_are_rejected() {
    let xml = XML.replace(
        "<fragment id=\"deviceItem10\"",
        "<fragment id=\"cycle\"><use ref=\"cycle\"/></fragment><fragment id=\"deviceItem10\"",
    );
    assert_eq!(compile_xml(&xml).err().unwrap().code, "cyclicReference");
}

#[test]
fn both_generated_golden_packets_roundtrip_exactly() {
    let schema = compile_xml(XML).unwrap();
    for (name, size, code) in [("request", 99, 0x21), ("response", 31, 0x22)] {
        let bytes = golden(name);
        assert_eq!(bytes.len(), size);
        let decoded = schema.decode(&bytes).unwrap();
        assert_eq!(decoded.message_code, code);
        assert_eq!(schema.encode(code, &decoded.values).unwrap(), bytes);
    }
}
#[test]
fn golden_packet_exposes_bits_arrays_and_chinese_enum_with_precise_spans() {
    let packet = compile_xml(XML)
        .unwrap()
        .decode(&golden("request"))
        .unwrap();
    let flags = packet.field("body.configCommon8.configFlags").unwrap();
    assert_eq!(flags.byte_start1, 30);
    assert_eq!(flags.decimal.as_deref(), Some("129"));
    assert_eq!(flags.binary, "10000001");
    assert_eq!(flags.children[0].decoded_value, 1);
    assert_eq!(flags.children[1].decoded_value, 1);
    let items = packet.field("body.items").unwrap();
    assert_eq!(items.children.len(), 5);
    assert_eq!(items.byte_length, 50);
    let freq = packet.field("body.items[0].centerFreqHz").unwrap();
    assert_eq!((freq.byte_start1, freq.byte_end1), (50, 53));
    assert_eq!(freq.decimal.as_deref(), Some("1000000"));
    let result = compile_xml(XML)
        .unwrap()
        .decode(&golden("response"))
        .unwrap();
    let results = result.field("body.results").unwrap();
    assert_eq!(results.children.len(), 5);
    assert_eq!(
        results.children[0].children[0].meaning_zh.as_deref(),
        Some("配置成功")
    );
}
#[test]
fn resizing_array_recomputes_header_body_packet_lengths_and_crc() {
    let schema = compile_xml(XML).unwrap();
    let packet = schema.decode(&golden("request")).unwrap();
    let mut values = packet.values;
    values["body"]["items"].as_array_mut().unwrap().truncate(3);
    let bytes = schema.encode(0x21, &values).unwrap();
    assert_eq!(bytes.len(), 79);
    let packet = schema.decode(&bytes).unwrap();
    assert_eq!(packet.values["header"]["bodyLength"], 48);
    assert_eq!(packet.values["body"]["itemCount"], 3);
}
#[test]
fn optional_common_segment_can_be_disabled_without_changing_codec() {
    let old = compile_xml(XML).unwrap();
    let values = old.decode(&golden("request")).unwrap().values;
    let schema = compile_xml(&XML.replace(
        "ref=\"configCommon8\" enabled=\"true\"",
        "ref=\"configCommon8\" enabled=\"false\"",
    ))
    .unwrap();
    let bytes = schema.encode(0x21, &values).unwrap();
    assert_eq!(bytes.len(), 91);
    assert_eq!(
        schema.decode(&bytes).unwrap().values["header"]["bodyLength"],
        60
    );
}
#[test]
fn malformed_lengths_crc_and_array_limits_are_rejected() {
    let schema = compile_xml(XML).unwrap();
    let mut bytes = golden("request");
    bytes[98] ^= 1;
    assert_eq!(schema.decode(&bytes).unwrap_err().code, "crcMismatch");
    let mut bytes = golden("request");
    bytes[39] = 21;
    assert!(schema.decode(&bytes).is_err());
    let mut bytes = golden("request");
    bytes[40] = 9;
    assert!(schema.decode(&bytes).is_err());
    let mut bytes = golden("response");
    bytes.remove(28);
    assert!(schema.decode(&bytes).is_err());
    let mut bytes = golden("response");
    bytes[3] = 22;
    assert_eq!(schema.decode(&bytes).unwrap_err().code, "lengthMismatch");
}
#[test]
fn unknown_result_values_are_preserved_without_invented_failure_codes() {
    let schema = compile_xml(XML).unwrap();
    let mut values = schema.decode(&golden("response")).unwrap().values;
    values["body"]["results"][2]["result"] = json!(0x19);
    let bytes = schema.encode(0x22, &values).unwrap();
    let packet = schema.decode(&bytes).unwrap();
    let field = packet.field("body.results[2].result").unwrap();
    assert_eq!(field.raw_hex, "19");
    assert!(field.meaning_zh.as_ref().unwrap().contains("未定义"));
}
#[test]
fn signed_little_endian_and_unsigned_fields_share_the_same_schema_codec() {
    let xml = r#"<protocol id="test" version="1" schemaVersion="1"><messages><message id="TEST" code="0x09"><header><field key="code" bytes="1" binding="message.code"/></header><body><field key="signed" bytes="2" valueKind="int"/><field key="unsigned" bytes="2"/><field key="little" bytes="2" endian="little"/></body></message></messages></protocol>"#;
    let schema = compile_xml(xml).unwrap();
    let bytes = parse_hex("09 FF FE FF FE 34 12").unwrap();
    let decoded = schema.decode(&bytes).unwrap();
    assert_eq!(decoded.values["body"]["signed"], -2);
    assert_eq!(decoded.values["body"]["unsigned"], 65534);
    assert_eq!(decoded.values["body"]["little"], 0x1234);
    assert_eq!(schema.encode(9, &decoded.values).unwrap(), bytes);
}
#[test]
fn secret_spans_can_be_redacted_before_logging() {
    let schema = compile_xml(XML).unwrap();
    let bytes = golden("request");
    let packet = schema.decode(&bytes).unwrap();
    assert_eq!(packet.secret_spans(), vec![(25, 4)]);
    let redacted = packet.redacted_bytes(&bytes);
    assert_eq!(&redacted[25..29], &[0, 0, 0, 0]);
    assert_eq!(&redacted[..25], &bytes[..25]);
}
#[test]
fn crc_matches_published_check_value_and_bit_edits_preserve_other_bits() {
    assert_eq!(crc16_ccitt_false(b"123456789"), 0x29B1);
    assert_eq!(set_bits(0x45, 7, 1, 1).unwrap(), 0xC5);
    assert_eq!(set_bits(0xC5, 0, 1, 0).unwrap(), 0xC4);
    assert!(set_bits(0, 7, 1, 2).is_err());
}

#[test]
fn expanded_v08_templates_roundtrip_original_registration_and_call_vectors() {
    let schema = compile_xml(include_str!("../../protocols/musp-v08.xml")).unwrap();
    assert_eq!(schema.messages.len(), 34);
    for hex in [
        "AA 55 01 17 00 23 01 00 00 3B 00 00 01 00 00 00 00 00 01 00 00 00 0A 01 00 00 00 3B 01 00 03 00 01 80 73",
        "AA 55 01 17 00 20 02 00 00 01 00 00 3B 02 00 00 00 00 01 12 34 00 07 00 00 00 0E 10 01 00 0E A8",
        "AA 55 01 1D 00 2B 60 00 00 3B 00 00 01 01 00 00 00 00 65 12 34 00 0C A1 04 DE AD BE EF 00 00 3C 02 08 00 02 00 00 03 01 00 94 AF",
        "AA 55 01 1D 00 27 65 00 00 3B 00 00 01 01 00 00 00 01 91 12 34 00 08 A1 04 DE AD BE EF 01 00 40 00 40 01 01 00 DF 81",
        "AA 55 01 1D 00 63 21 00 00 01 00 00 3B 01 00 00 00 00 65 12 34 00 44 A1 04 DE AD BE EF 81 00 01 00 01 00 00 00 02 01 05 0A 00 00 3B 00 01 00 01 01 00 0F 42 A4 00 0A 01 00 02 01 00 0F 43 08 00 0A 01 00 03 01 00 0F 43 6C 00 0A 01 00 04 01 00 0F 43 D0 00 0A 01 00 05 01 00 0F 44 34 00 0A 01 00 EB 46",
    ] {
        let bytes = parse_hex(hex).unwrap();
        let packet = schema.decode(&bytes).unwrap();
        assert_eq!(
            schema.encode(packet.message_code, &packet.values).unwrap(),
            bytes
        );
    }
}
#[test]
fn default_item_size_must_agree_with_variant_structure() {
    let xml = XML.replace("default=\"0A\"", "default=\"09\"");
    assert_eq!(compile_xml(&xml).unwrap_err().code, "elementSize");
}
#[test]
fn computed_len_requires_an_existing_byte_field() {
    let xml = XML.replace("computed=\"len(token)\"", "computed=\"len(missingToken)\"");
    assert_eq!(compile_xml(&xml).unwrap_err().code, "missingReference");
}
#[test]
fn all_compiler_errors_have_source_location() {
    let xml = XML.replace("lengthExpr=\"tokenLength\"", "lengthExpr=\"laterField\"");
    let error = compile_xml(&xml).unwrap_err();
    assert!(error.line.is_some());
    assert!(error.column.is_some());
}
#[test]
fn typo_in_field_constraint_is_rejected_instead_of_silently_ignored() {
    let xml = XML.replace("fixed=\"AA 55\"", "fixedd=\"AA 55\"");
    assert_eq!(compile_xml(&xml).unwrap_err().code, "unknownAttribute");
}
#[test]
fn malicious_lengths_and_every_truncated_prefix_are_bounded_errors() {
    let schema = compile_xml(XML).unwrap();
    let bytes = golden("request");
    for len in 0..bytes.len() {
        assert!(schema.decode(&bytes[..len]).is_err(), "truncation at {len}");
    }
    for offset in [3, 4, 5, 21, 22, 24, 39, 40] {
        let mut altered = bytes.clone();
        altered[offset] = 255;
        assert!(schema.decode(&altered).is_err());
    }
    assert!(schema.decode(&vec![0; 65508]).is_err());
}

fn minimal_schema(body: &str) -> String {
    format!(
        r#"<protocol id="scoped" version="1"><messages><message id="TEST" code="0x01"><header><field key="code" bytes="1" binding="message.code"/></header><body>{body}</body></message></messages></protocol>"#
    )
}

#[test]
fn nested_array_counts_cannot_overwrite_outer_length_scope() {
    let schema=compile_xml(&minimal_schema(r#"<field key="n" bytes="1"/><array key="groups" countRef="n" maxItems="4"><field key="n" bytes="1"/><array key="items" countRef="n" elementBytes="1" maxItems="4"><field key="value" bytes="1"/></array></array><field key="tail" lengthExpr="n" valueKind="bytes"/>"#)).unwrap();
    let bytes = parse_hex("01 02 01 AA 03 BB CC DD EE FF").unwrap();
    let packet = schema.decode(&bytes).unwrap();
    assert_eq!(packet.values["body"]["tail"], "EE FF");
    assert_eq!(schema.encode(1, &packet.values).unwrap(), bytes);
}

#[test]
fn structures_preserve_local_scopes_and_can_be_referenced_by_full_path() {
    let schema=compile_xml(&minimal_schema(r#"<struct key="one" labelZh="结构一" bytes="3"><field key="n" bytes="1"/><field key="data" lengthExpr="n" valueKind="bytes"/></struct><struct key="two" labelZh="结构二"><field key="n" bytes="1"/><array key="items" countRef="n" elementBytes="1" maxItems="4"><field key="v" bytes="1"/></array></struct><field key="tail" lengthExpr="body.one.n" valueKind="bytes"/>"#)).unwrap();
    let bytes = parse_hex("01 02 AA BB 01 CC DD EE").unwrap();
    let packet = schema.decode(&bytes).unwrap();
    assert_eq!(packet.field("body.one.data").unwrap().raw_hex, "AA BB");
    assert_eq!(packet.field("body.two.items[0].v").unwrap().byte_start1, 6);
    assert_eq!(schema.encode(1, &packet.values).unwrap(), bytes);
}

#[test]
fn fragment_fields_can_be_referenced_by_unambiguous_qualified_path() {
    let xml = XML
        .replace(
            "countRef=\"body.itemCount\"",
            "countRef=\"body.configSetPrefix10.itemCount\"",
        )
        .replacen(
            "countRef=\"body.configSetPrefix10.itemCount\" elementBytes",
            "countRef=\"body.itemCount\" elementBytes",
            1,
        );
    let schema = compile_xml(&xml).unwrap();
    let bytes = golden("request");
    let decoded = schema.decode(&bytes).unwrap();
    assert_eq!(schema.encode(0x21, &decoded.values).unwrap(), bytes);
}

#[test]
fn unguarded_reference_to_conditional_field_cannot_publish() {
    let xml = minimal_schema(
        r#"<field key="flag" bytes="1"/><field key="n" bytes="1" when="flag==1"/><field key="raw" lengthExpr="n" valueKind="bytes"/>"#,
    );
    assert_eq!(
        compile_xml(&xml)
            .expect_err("conditional length must be guarded")
            .code,
        "forwardReference"
    );
}

#[test]
fn byte_values_and_forward_lengths_are_not_numeric_expression_inputs() {
    let raw = minimal_schema(
        r#"<field key="data" bytes="2" valueKind="bytes"/><field key="bad" lengthExpr="data+1" valueKind="bytes"/>"#,
    );
    assert!(compile_xml(&raw).is_err());
    let forward = minimal_schema(
        r#"<field key="bad" lengthExpr="len(later)" valueKind="bytes"/><field key="later" bytes="2" valueKind="bytes"/>"#,
    );
    assert!(compile_xml(&forward).is_err());
}

#[test]
fn secret_fields_do_not_leak_values_through_meaning_or_error_messages() {
    let schema=compile_xml(&minimal_schema(r#"<field key="token" bytes="1" secret="true" valueKind="bitset"><enum hex="01" labelZh="secret-one"/><bits><bit key="enabled" start="0" width="1" labelZh="启用"/></bits></field>"#)).unwrap();
    let packet = schema.decode(&[1, 0x19]).unwrap();
    let token = packet.field("body.token").unwrap();
    assert!(token.meaning_zh.is_none());
    assert!(token.children[0].meaning_zh.is_none());
    let error = schema
        .encode(1, &json!({"body":{"token":42424242}}))
        .unwrap_err();
    assert!(!error.to_string().contains("42424242"));
}

#[test]
fn basic_header_cannot_have_optional_fields_that_move_the_message_code() {
    let xml = r#"<protocol id="bad" version="1"><messages><message id="TEST" code="0x01"><header><field key="optional" bytes="1" when="0"/><field key="code" bytes="1" binding="message.code"/></header><body/></message></messages></protocol>"#;
    assert!(compile_xml(xml).is_err());
}

#[test]
fn malformed_secret_packets_report_locations_without_echoing_token_bytes() {
    let schema = compile_xml(XML).unwrap();
    let mut bytes = golden("request");
    bytes[98] ^= 1;
    let error = serde_json::to_string(&schema.decode(&bytes).unwrap_err()).unwrap();
    assert!(!error.contains("DE AD BE EF"));
    assert!(!error.contains("3735928559"));
}
