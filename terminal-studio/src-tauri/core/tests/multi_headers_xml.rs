use simterm_core::models::{Protocol, ProtocolMd};
use simterm_core::xml::{export_protocols_xml, parse_protocols_xml};

fn rejection(xml: &str) -> String {
    match parse_protocols_xml(xml) {
        Ok(parsed) => {
            assert!(parsed.protocols.is_empty(), "invalid protocol was accepted");
            assert!(!parsed.failed.is_empty(), "invalid XML was accepted");
            parsed.failed.join("\n")
        }
        Err(error) => error.to_string(),
    }
}

#[test]
fn xml_rejects_unknown_header_reference() {
    let error = rejection(
        r#"<simterm-import><system name="S"><group name="G">
      <header id="A" name="头 A"><field name="类型" len="1"/></header>
      <protocol id="p" header-refs="missing"><field name="数据" len="1"/></protocol>
    </group></system></simterm-import>"#,
    );
    assert!(
        error.contains("missing") && error.contains("公共头"),
        "{error}"
    );
}

#[test]
fn xml_rejects_duplicate_header_references() {
    let error = rejection(
        r#"<simterm-import><system name="S"><group name="G">
      <header id="A"><field name="类型" len="1"/></header>
      <protocol id="p" header-refs="A,A"><field name="数据" len="1"/></protocol>
    </group></system></simterm-import>"#,
    );
    assert!(error.contains('A') && error.contains("重复"), "{error}");
}

#[test]
fn xml_rejects_duplicate_header_ids() {
    let error = rejection(
        r#"<simterm-import><system name="S"><group name="G">
      <header id="A"><field name="首字节" len="1"/></header>
      <header id="A"><field name="次字节" len="1"/></header>
      <protocol id="p" header-refs="A"><field name="数据" len="1"/></protocol>
    </group></system></simterm-import>"#,
    );
    assert!(error.contains('A') && error.contains("重复"), "{error}");
}

#[test]
fn xml_rejects_refs_from_another_group_or_system() {
    for other_scope in [
        r#"<system name="S"><group name="other"><header id="A"><field name="类型" len="1"/></header></group></system>"#,
        r#"<system name="other"><group name="G"><header id="A"><field name="类型" len="1"/></header></group></system>"#,
    ] {
        let xml = format!(
            r#"<simterm-import>{other_scope}
          <system name="S"><group name="G"><protocol id="p" header-refs="A"/></group></system>
        </simterm-import>"#
        );
        let error = rejection(&xml);
        assert!(error.contains('A') && error.contains("公共头"), "{error}");
    }
}

#[test]
fn xml_rejects_ambiguous_ids_and_malformed_ref_lists() {
    for id in ["", " A", "A ", "A,B", "A::B"] {
        let xml = format!(
            r#"<simterm-import><system name="S"><group name="G">
          <header id="{id}"><field name="类型" len="1"/></header>
        </group></system></simterm-import>"#
        );
        let error = rejection(&xml);
        assert!(error.contains("id"), "{error}");
    }
    for refs in ["A,", ",A", "A,,B", "A, B", " A", "A::类型"] {
        let xml = format!(
            r#"<simterm-import><system name="S"><group name="G">
          <header id="A"><field name="类型" len="1"/></header>
          <header id="B"><field name="类型" len="1"/></header>
          <protocol id="p" header-refs="{refs}"/>
        </group></system></simterm-import>"#
        );
        let error = rejection(&xml);
        assert!(
            error.contains("header-refs") || error.contains("公共头"),
            "{error}"
        );
    }
}

#[test]
fn xml_rejects_duplicate_fields_within_one_header() {
    let error = rejection(
        r#"<simterm-import><system name="S"><group name="G">
      <header id="A"><field name="类型" len="1"/><field name="类型" len="2"/></header>
    </group></system></simterm-import>"#,
    );
    assert!(error.contains("类型") && error.contains("重复"), "{error}");
}

fn protocol_row(p: &ProtocolMd) -> Protocol {
    Protocol {
        id: 0,
        protocol_id: p.protocol_id.clone(),
        name: p.name.clone(),
        format: p.format,
        msg_type: p.msg_type,
        def_mode: p.def_mode,
        fields_json: Some(serde_json::to_string(&p.fields).unwrap()),
        header_refs_json: p
            .header_refs
            .as_ref()
            .map(|refs| serde_json::to_string(refs).unwrap()),
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
fn xml_round_trip_keeps_all_headers_and_ordered_optional_refs() {
    let xml = r#"<simterm-import><system name="S"><group name="G">
      <header id="A" name="头 A"><field name="类型" len="1" value="01"/></header>
      <header id="B" name="头 B"><field name="类型" len="2" value="02 03"/></header>
      <protocol id="ordered" header-refs="B,A">
        <field name="覆盖" len="2" value="04 05" header-ref="B::类型" header-key="B::类型"/>
      </protocol>
      <protocol id="none" header-refs=""/>
      <protocol id="inherit"/>
    </group></system></simterm-import>"#;
    let parsed = parse_protocols_xml(xml).unwrap();
    assert!(parsed.failed.is_empty(), "{:?}", parsed.failed);
    assert_eq!(parsed.group_headers.len(), 2);
    assert_eq!(parsed.group_headers[0].id, "A");
    assert_eq!(parsed.group_headers[0].name, "头 A");
    assert_eq!(parsed.group_headers[1].id, "B");
    assert_eq!(
        parsed.protocols[0].header_refs,
        Some(vec!["B".into(), "A".into()])
    );
    assert_eq!(parsed.protocols[1].header_refs, Some(vec![]));
    assert_eq!(parsed.protocols[2].header_refs, None);
    assert_eq!(
        parsed.protocols[0].fields[0].header_key.as_deref(),
        Some("B::类型")
    );
    let rows: Vec<_> = parsed.protocols.iter().map(protocol_row).collect();
    let exported = export_protocols_xml(&rows, &[], &parsed.group_headers, &[]);
    assert!(exported.contains("header-refs=\"B,A\""), "{exported}");
    assert!(exported.contains("header-refs=\"\""), "{exported}");
    assert!(exported.contains("header-key=\"B::类型\""), "{exported}");
    let back = parse_protocols_xml(&exported).unwrap();
    assert!(back.failed.is_empty(), "{:?}", back.failed);
    assert_eq!(back.group_headers.len(), 2);
    for (before, after) in parsed.group_headers.iter().zip(&back.group_headers) {
        assert_eq!(before.id, after.id);
        assert_eq!(before.name, after.name);
        assert_eq!(before.enabled, after.enabled);
        assert_eq!(before.fields, after.fields);
    }
    for (before, after) in parsed.protocols.iter().zip(&back.protocols) {
        assert_eq!(before.header_refs, after.header_refs);
        assert_eq!(before.fields, after.fields);
    }
}

#[test]
fn xml_legacy_system_header_is_scoped_and_disabled_override_survives_export() {
    let xml = r#"<simterm-import><system name="S">
      <group name="G"><protocol id="inherit"/></group>
      <group name="Off"><header enabled="false"/><protocol id="off"/></group>
      <header><field name="类型" len="1" value="AA"/></header>
    </system><system name="Other"><group name="G"><protocol id="other"/></group></system></simterm-import>"#;
    let parsed = parse_protocols_xml(xml).unwrap();
    assert!(parsed.failed.is_empty(), "{:?}", parsed.failed);
    assert_eq!(parsed.group_headers.len(), 2);
    let inherited = parsed
        .group_headers
        .iter()
        .find(|h| h.group == "G")
        .unwrap();
    assert_eq!(inherited.id, "legacy-default");
    assert_eq!(inherited.name, "G 公共头");
    assert!(inherited.enabled);
    assert_eq!(inherited.fields[0].value, "AA");
    let disabled = parsed
        .group_headers
        .iter()
        .find(|h| h.group == "Off")
        .unwrap();
    assert!(!disabled.enabled);
    let rows: Vec<_> = parsed.protocols.iter().map(protocol_row).collect();
    let exported = export_protocols_xml(&rows, &[], &parsed.group_headers, &[]);
    let back = parse_protocols_xml(&exported).unwrap();
    assert_eq!(back.group_headers.len(), 2);
    assert!(
        !back
            .group_headers
            .iter()
            .find(|h| h.group == "Off")
            .unwrap()
            .enabled
    );
    assert!(back.protocols.iter().all(|p| p.header_refs.is_none()));
}

#[test]
fn xml_legacy_unresolved_field_override_remains_importable_without_explicit_refs() {
    let parsed = parse_protocols_xml(
        r#"<simterm-import><protocol id="legacy">
      <field name="覆盖" len="1" header-ref="外部旧公共头字段"/>
    </protocol></simterm-import>"#,
    )
    .unwrap();
    assert!(parsed.failed.is_empty());
    assert_eq!(parsed.protocols.len(), 1);
    assert_eq!(parsed.protocols[0].header_refs, None);
}

#[test]
fn xml_explicit_empty_header_library_survives_export_and_blocks_legacy_inheritance() {
    let parsed = parse_protocols_xml(
        r#"<simterm-import><system name="S">
      <header><field name="类型" len="1"/></header>
      <group name="G" header-profiles="[]"><protocol id="p" header-refs=""/></group>
    </system></simterm-import>"#,
    )
    .unwrap();
    assert!(parsed.failed.is_empty());
    assert_eq!(parsed.group_headers.len(), 1);
    let marker = &parsed.group_headers[0];
    assert!(marker.id.is_empty() && marker.name.is_empty());
    assert!(!marker.enabled && marker.fields.is_empty());
    let exported = export_protocols_xml(
        &[protocol_row(&parsed.protocols[0])],
        &[],
        &parsed.group_headers,
        &[],
    );
    assert!(exported.contains("header-profiles=\"[]\""), "{exported}");
    assert!(!exported.contains("<header "), "{exported}");
    let back = parse_protocols_xml(&exported).unwrap();
    assert_eq!(back.group_headers.len(), 1);
    assert!(back.group_headers[0].id.is_empty());
    assert_eq!(back.protocols[0].header_refs, Some(vec![]));
}

#[test]
fn xml_empty_library_marker_cannot_coexist_with_definitions() {
    let error = rejection(
        r#"<simterm-import><system name="S"><group name="G" header-profiles="[]">
      <header id="A"><field name="类型" len="1"/></header>
    </group></system></simterm-import>"#,
    );
    assert!(
        error.contains("header-profiles") && error.contains("公共头"),
        "{error}"
    );
}

#[test]
fn xml_explicit_refs_reject_ambiguous_or_unselected_field_override() {
    for (refs, target, expected) in [("A,B", "类型", "歧义"), ("A", "B::类型", "没有该字段")]
    {
        let xml = format!(
            r#"<simterm-import><system name="S"><group name="G">
          <header id="A"><field name="类型" len="1"/></header>
          <header id="B"><field name="类型" len="1"/></header>
          <protocol id="p" header-refs="{refs}">
            <field name="覆盖" len="1" header-ref="{target}"/>
          </protocol>
        </group></system></simterm-import>"#
        );
        let error = rejection(&xml);
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn xml_exports_header_libraries_without_protocols() {
    let parsed = parse_protocols_xml(
        r#"<simterm-import><system name="S">
      <group name="G"><header id="A"><field name="类型" len="1"/></header></group>
      <group name="Empty" header-profiles="[]"/>
    </system></simterm-import>"#,
    )
    .unwrap();
    let exported = export_protocols_xml(&[], &[], &parsed.group_headers, &[]);
    let back = parse_protocols_xml(&exported).unwrap();
    assert_eq!(back.group_headers.len(), 2);
    assert_eq!(back.group_headers[0].id, "A");
    assert_eq!(back.group_headers[1].group, "Empty");
    assert!(back.group_headers[1].id.is_empty());
}

#[test]
fn xml_exports_system_level_protocols_without_inventing_a_group() {
    let parsed = parse_protocols_xml(r#"<simterm-import><system name="S">
      <protocol id="system-empty" header-refs=""><field name="数据" len="1" value="AA"/></protocol>
      <protocol id="system-inherit"><field name="数据" len="1" value="BB"/></protocol>
      <group name="G"><protocol id="grouped" header-refs=""><field name="数据" len="1" value="CC"/></protocol></group>
    </system></simterm-import>"#).unwrap();
    let rows: Vec<_> = parsed.protocols.iter().map(protocol_row).collect();
    let exported = export_protocols_xml(&rows, &[], &parsed.group_headers, &[]);
    assert!(!exported.contains("<group name=\"\""), "{exported}");
    let back = parse_protocols_xml(&exported).unwrap();
    assert!(back.failed.is_empty(), "{:?}", back.failed);
    assert_eq!(back.protocols.len(), 3, "{exported}");
    for before in &parsed.protocols {
        let after = back
            .protocols
            .iter()
            .find(|p| p.protocol_id == before.protocol_id)
            .unwrap();
        assert_eq!(after.system, before.system);
        assert_eq!(after.group, before.group);
        assert_eq!(after.header_refs, before.header_refs);
        assert_eq!(after.fields, before.fields);
    }
}

#[test]
fn xml_legacy_duplicate_fields_keep_metadata_and_qualified_override_after_round_trip() {
    let parsed = parse_protocols_xml(r#"<simterm-import><system name="S"><group name="G">
      <header>
        <field name="保留" len="1" value="A1" section="header" section-name="基础区"/>
        <field name="保留" len="1" value="A2" section="header_extension" section-name="扩展区" header-key="old::key"/>
        <field name="保留#2" len="1" value="A3"/>
      </header>
      <protocol id="p" header-refs="legacy-default"><field name="覆盖第二项" len="1" value="09" header-ref="legacy-default::保留#3"/></protocol>
    </group></system></simterm-import>"#).unwrap();
    assert!(parsed.failed.is_empty(), "{:?}", parsed.failed);
    let exported = export_protocols_xml(
        &[protocol_row(&parsed.protocols[0])],
        &[],
        &parsed.group_headers,
        &[],
    );
    let back = parse_protocols_xml(&exported).unwrap();
    assert!(back.failed.is_empty(), "{:?}", back.failed);
    assert_eq!(back.group_headers[0].fields, parsed.group_headers[0].fields);
    assert_eq!(back.protocols[0].fields, parsed.protocols[0].fields);
    let header = &back.group_headers[0];
    let resolved =
        simterm_core::headers::resolve_definition_fields(&simterm_core::models::HeaderDefinition {
            id: header.id.clone(),
            name: header.name.clone(),
            fields: header.fields.clone(),
        });
    assert_eq!(
        resolved
            .iter()
            .map(|f| f.header_key.as_deref().unwrap())
            .collect::<Vec<_>>(),
        [
            "legacy-default::保留",
            "legacy-default::保留#3",
            "legacy-default::保留#2"
        ]
    );
    let (merged, body) =
        simterm_core::fields::merge_header_specs(&resolved, &back.protocols[0].fields).unwrap();
    assert!(body.is_empty());
    assert_eq!(
        merged.iter().map(|f| f.value.as_str()).collect::<Vec<_>>(),
        ["A1", "09", "A3"]
    );
    assert_eq!(
        merged[1].section,
        Some(simterm_core::models::FieldSection::HeaderExtension)
    );
    assert_eq!(merged[1].section_name.as_deref(), Some("扩展区"));
}
