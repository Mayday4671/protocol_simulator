use serde_json::json;
use simterm_core::fields::{dedupe_field_names, merge_header_specs, render_fields};
use simterm_core::headers::{
    group_definitions, resolve_headers, selected_header_ids, validate_definitions, LEGACY_HEADER_ID,
};
use simterm_core::models::{
    FieldSection, HeaderDefinition, Node, NodeRole, ProtocolField, ProtocolGroup,
};
use simterm_core::render::RenderCtx;

#[test]
fn header_identity_survives_field_serialization() {
    let field: ProtocolField = serde_json::from_value(json!({
        "name": "Type", "len": 1, "enc": "hex", "header_key": "A::Type"
    }))
    .unwrap();
    assert_eq!(
        serde_json::to_value(field).unwrap()["header_key"],
        "A::Type"
    );
}

#[test]
fn legacy_duplicate_fields_keep_first_override_and_unique_keys_without_reordering() {
    let legacy = group(None, true, Some(vec![field("Reserved", "A1"), field("Reserved", "A2"), field("Reserved#2", "A3")]));
    let selected = resolve_headers(Some(&legacy), None).unwrap();
    assert_eq!(wire(&selected), [0xA1, 0xA2, 0xA3]);
    assert_eq!(selected.iter().map(|f| f.header_key.as_deref().unwrap()).collect::<Vec<_>>(), ["legacy-default::Reserved", "legacy-default::Reserved#3", "legacy-default::Reserved#2"]);
    let (first, _) = merge_header_specs(&selected, &[override_field("Old override", "Reserved", "09")]).unwrap();
    assert_eq!(wire(&first), [0x09, 0xA2, 0xA3]);
    let (second, _) = merge_header_specs(&selected, &[override_field("Second override", "legacy-default::Reserved#3", "08")]).unwrap();
    assert_eq!(wire(&second), [0xA1, 0x08, 0xA3]);
    let mut combined = selected;
    let new_header = resolve_headers(Some(&group(Some(vec![definition("A", "New header", vec![field("Reserved", "BB")])]), false, None)), Some(&refs(&["A"]))).unwrap();
    combined.extend(new_header);
    assert!(merge_header_specs(&combined, &[override_field("Ambiguous", "Reserved", "07")]).unwrap_err().to_string().contains("歧义"));
}

#[test]
fn old_empty_header_columns_mean_no_header_even_when_enabled() {
    let legacy = group(None, true, Some(vec![]));
    assert!(group_definitions(&legacy).unwrap().is_empty());
    assert!(resolve_headers(Some(&legacy), None).unwrap().is_empty());
}

fn field(name: &str, value: &str) -> ProtocolField {
    serde_json::from_value(json!({ "name": name, "len": 1, "enc": "hex", "value": value })).unwrap()
}

fn definition(id: &str, name: &str, fields: Vec<ProtocolField>) -> HeaderDefinition {
    HeaderDefinition {
        id: id.into(),
        name: name.into(),
        fields,
    }
}

fn group(
    headers: Option<Vec<HeaderDefinition>>,
    legacy_enabled: bool,
    legacy: Option<Vec<ProtocolField>>,
) -> ProtocolGroup {
    ProtocolGroup {
        id: 1,
        system_ref: 10,
        name: "分组甲".into(),
        header_profiles_json: headers.map(|headers| serde_json::to_string(&headers).unwrap()),
        header_enabled: legacy_enabled,
        header_fields_json: legacy.map(|fields| serde_json::to_string(&fields).unwrap()),
        created_at: String::new(),
    }
}

fn two_headers() -> ProtocolGroup {
    group(
        Some(vec![
            definition(
                "A",
                "基础头",
                vec![field("类型", "A1"), field("版本", "A2")],
            ),
            definition("B", "扩展头", vec![field("类型", "B1")]),
        ]),
        false,
        None,
    )
}

fn refs(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| (*id).to_string()).collect()
}

fn wire(fields: &[ProtocolField]) -> Vec<u8> {
    wire_with_params(fields, &std::collections::HashMap::new())
}

fn wire_with_params(fields: &[ProtocolField], params: &std::collections::HashMap<String, String>) -> Vec<u8> {
    let node = Node {
        id: 1,
        name: "测试终端".into(),
        role: NodeRole::Simulator,
        local_ip: "127.0.0.1".into(),
        local_port: 5000,
        remote_ip: "127.0.0.1".into(),
        remote_port: 6000,
        node_id_hex: None,
        session_hex: None,
        captures_json: None,
        group_bind_json: None,
        enabled: true,
        created_at: String::new(),
        updated_at: String::new(),
    };
    render_fields(fields, &RenderCtx::new(&node, 0, params, &simterm_core::render::DictMap::new())).unwrap()
}

#[test]
fn frame_name_dedupe_reserves_literal_suffixes_and_keeps_params_independent() {
    let mut fields = vec![field("Reserved", "A1"), field("Reserved", "A2"), field("Reserved#2", "A3")];
    dedupe_field_names(&mut fields);
    assert_eq!(fields.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["Reserved", "Reserved#3", "Reserved#2"]);
    let params = std::collections::HashMap::from([("Reserved".into(), "11".into()), ("Reserved#3".into(), "22".into()), ("Reserved#2".into(), "33".into())]);
    let bytes = wire_with_params(&fields, &params);
    assert_eq!(bytes, [0x11, 0x22, 0x33]);
    let extracted = simterm_core::fields::extract_field_values(&fields, &bytes, &simterm_core::render::DictMap::new());
    assert_eq!(extracted, params);
    let once = fields.clone();
    dedupe_field_names(&mut fields);
    assert_eq!(fields, once);
}

fn override_field(name: &str, target: &str, value: &str) -> ProtocolField {
    let mut override_field = field(name, value);
    override_field.hdr_ref = Some(target.into());
    override_field
}

#[test]
fn legacy_enabled_header_is_default_for_null_refs() {
    let group = group(None, true, Some(vec![field("类型", "AA")]));
    let definitions = group_definitions(&group).unwrap();
    assert_eq!(definitions.len(), 1);
    assert_eq!(definitions[0].id, LEGACY_HEADER_ID);
    assert_eq!(definitions[0].name, "分组甲 公共头");
    assert_eq!(
        selected_header_ids(Some(&group), None).unwrap(),
        refs(&[LEGACY_HEADER_ID])
    );
    assert_eq!(wire(&resolve_headers(Some(&group), None).unwrap()), [0xAA]);
    assert!(resolve_headers(Some(&group), Some(&[])).unwrap().is_empty());
}

#[test]
fn disabled_legacy_header_stays_available_only_when_explicitly_selected() {
    let group = group(None, false, Some(vec![field("类型", "AA")]));
    assert_eq!(group_definitions(&group).unwrap()[0].id, LEGACY_HEADER_ID);
    assert!(selected_header_ids(Some(&group), None).unwrap().is_empty());
    assert!(resolve_headers(Some(&group), None).unwrap().is_empty());
    let explicit = refs(&[LEGACY_HEADER_ID]);
    assert_eq!(
        wire(&resolve_headers(Some(&group), Some(&explicit)).unwrap()),
        [0xAA]
    );
}

#[test]
fn no_group_or_unconfigured_group_has_no_default_header() {
    let group = group(None, false, None);
    assert!(group_definitions(&group).unwrap().is_empty());
    assert!(resolve_headers(Some(&group), None).unwrap().is_empty());
    assert!(resolve_headers(None, None).unwrap().is_empty());
    assert!(resolve_headers(None, Some(&[])).unwrap().is_empty());
    assert!(resolve_headers(None, Some(&refs(&["A"]))).is_err());
}

#[test]
fn new_header_library_never_implicitly_selects_a_named_header() {
    let mut group = two_headers();
    for legacy_enabled in [false, true] {
        group.header_enabled = legacy_enabled;
        assert!(selected_header_ids(Some(&group), None).unwrap().is_empty());
        assert!(resolve_headers(Some(&group), None).unwrap().is_empty());
    }
}

#[test]
fn explicit_empty_library_blocks_stale_legacy_state() {
    let group = group(Some(vec![]), true, Some(vec![field("旧类型", "AA")]));
    assert!(group_definitions(&group).unwrap().is_empty());
    assert!(selected_header_ids(Some(&group), None).unwrap().is_empty());
    assert!(resolve_headers(Some(&group), None).unwrap().is_empty());
    assert!(resolve_headers(Some(&group), Some(&[])).unwrap().is_empty());
    let error = resolve_headers(Some(&group), Some(&refs(&[LEGACY_HEADER_ID])))
        .unwrap_err()
        .to_string();
    assert!(error.contains(LEGACY_HEADER_ID), "{error}");
}

#[test]
fn selected_header_order_controls_actual_wire_bytes() {
    let group = two_headers();
    for (ids, bytes) in [
        (vec!["A"], vec![0xA1, 0xA2]),
        (vec!["B"], vec![0xB1]),
        (vec!["A", "B"], vec![0xA1, 0xA2, 0xB1]),
        (vec!["B", "A"], vec![0xB1, 0xA1, 0xA2]),
        (vec![], vec![]),
    ] {
        let ids = refs(&ids);
        assert_eq!(selected_header_ids(Some(&group), Some(&ids)).unwrap(), ids);
        let fields = resolve_headers(Some(&group), Some(&ids)).unwrap();
        assert_eq!(wire(&fields), bytes, "refs={ids:?}");
    }
}

#[test]
fn unknown_and_duplicate_header_references_are_rejected() {
    let group = two_headers();
    let unknown = resolve_headers(Some(&group), Some(&refs(&["missing"])))
        .unwrap_err()
        .to_string();
    assert!(
        unknown.contains("missing") && unknown.contains("分组甲"),
        "{unknown}"
    );
    let duplicate = resolve_headers(Some(&group), Some(&refs(&["A", "A"])))
        .unwrap_err()
        .to_string();
    assert!(
        duplicate.contains('A') && duplicate.contains("重复"),
        "{duplicate}"
    );
}

#[test]
fn header_lookup_is_scoped_to_the_selected_group() {
    let first = two_headers();
    let mut second = group(
        Some(vec![definition("A", "另组头", vec![field("类型", "D1")])]),
        false,
        None,
    );
    second.id = 2;
    second.system_ref = 20;
    second.name = "分组乙".into();
    let a = refs(&["A"]);
    assert_eq!(
        wire(&resolve_headers(Some(&first), Some(&a)).unwrap()),
        [0xA1, 0xA2]
    );
    assert_eq!(
        wire(&resolve_headers(Some(&second), Some(&a)).unwrap()),
        [0xD1]
    );
    let error = resolve_headers(Some(&second), Some(&refs(&["B"])))
        .unwrap_err()
        .to_string();
    assert!(error.contains('B') && error.contains("分组乙"), "{error}");
}

#[test]
fn resolution_preserves_explicit_sections_and_fills_header_defaults() {
    let mut explicit = field("认证", "AA");
    explicit.section = Some(FieldSection::HeaderExtension);
    explicit.section_name = Some("认证区".into());
    explicit.header_key = Some("stale::old".into());
    let group = group(
        Some(vec![definition(
            "A",
            "公共定义",
            vec![explicit.clone(), field("类型", "BB")],
        )]),
        false,
        None,
    );
    let fields = resolve_headers(Some(&group), Some(&refs(&["A"]))).unwrap();
    assert_eq!(fields[0].section, Some(FieldSection::HeaderExtension));
    assert_eq!(fields[0].section_name.as_deref(), Some("认证区"));
    assert_eq!(fields[0].header_key.as_deref(), Some("A::认证"));
    assert_eq!(fields[1].section, Some(FieldSection::Header));
    assert_eq!(fields[1].section_name.as_deref(), Some("公共定义"));
    assert_eq!(fields[1].header_key.as_deref(), Some("A::类型"));
    assert_eq!(group_definitions(&group).unwrap()[0].fields[0], explicit);
}

#[test]
fn qualified_overrides_change_only_the_selected_header_field() {
    let group = two_headers();
    let selected = resolve_headers(Some(&group), Some(&refs(&["B", "A"]))).unwrap();
    let body = vec![
        override_field("甲类型覆盖", "A::类型", "09"),
        field("类型", "CC"),
        override_field("乙类型覆盖", "B::类型", "08"),
    ];
    let (head, body_only) = merge_header_specs(&selected, &body).unwrap();
    assert_eq!(wire(&head), [0x08, 0x09, 0xA2]);
    assert_eq!(head[0].header_key.as_deref(), Some("B::类型"));
    assert_eq!(head[1].header_key.as_deref(), Some("A::类型"));
    assert_eq!(body_only, vec![body[1].clone()]);
    assert_eq!(wire(&selected), [0xB1, 0xA1, 0xA2]);
}

#[test]
fn bare_override_is_allowed_only_when_the_target_name_is_unique() {
    let group = two_headers();
    let selected = resolve_headers(Some(&group), Some(&refs(&["A", "B"]))).unwrap();
    let unique = vec![override_field("版本覆盖", "版本", "03")];
    assert_eq!(
        wire(&merge_header_specs(&selected, &unique).unwrap().0),
        [0xA1, 0x03, 0xB1]
    );
    let ambiguous = vec![override_field("类型覆盖", "类型", "03")];
    let error = merge_header_specs(&selected, &ambiguous)
        .unwrap_err()
        .to_string();
    assert!(error.contains("类型") && error.contains("歧义"), "{error}");
    let only_a = resolve_headers(Some(&group), Some(&refs(&["A"]))).unwrap();
    assert_eq!(
        wire(&merge_header_specs(&only_a, &ambiguous).unwrap().0),
        [0x03, 0xA2]
    );
}

#[test]
fn multiple_body_overrides_of_one_header_slot_are_rejected() {
    let group = two_headers();
    let selected = resolve_headers(Some(&group), Some(&refs(&["A"]))).unwrap();
    for targets in [["A::类型", "A::类型"], ["类型", "A::类型"]] {
        let body = vec![
            override_field("覆盖一", targets[0], "01"),
            override_field("覆盖二", targets[1], "02"),
        ];
        let error = merge_header_specs(&selected, &body)
            .unwrap_err()
            .to_string();
        assert!(error.contains("重复覆盖"), "{error}");
    }
}

#[test]
fn duplicate_display_names_are_renamed_without_changing_wire_order_or_identity() {
    let group = two_headers();
    let selected = resolve_headers(Some(&group), Some(&refs(&["B", "A"]))).unwrap();
    let (mut combined, body) = merge_header_specs(&selected, &[field("类型", "CC")]).unwrap();
    combined.extend(body);
    let before = wire(&combined);
    dedupe_field_names(&mut combined);
    assert_eq!(
        combined
            .iter()
            .map(|field| field.name.as_str())
            .collect::<Vec<_>>(),
        ["类型", "类型#2", "版本", "类型#3"]
    );
    assert_eq!(combined[0].header_key.as_deref(), Some("B::类型"));
    assert_eq!(combined[1].header_key.as_deref(), Some("A::类型"));
    assert_eq!(wire(&combined), before);
    assert_eq!(before, [0xB1, 0xA1, 0xA2, 0xCC]);
}

#[test]
fn invalid_header_library_id_and_duplicate_field_names_are_rejected() {
    for id in ["", " A", "A ", "A,B", "A::B"] {
        assert!(
            validate_definitions(&[definition(id, "名称", vec![field("类型", "AA")])]).is_err(),
            "id={id:?}"
        );
    }
    let duplicate_ids = [
        definition("A", "头一", vec![field("类型", "AA")]),
        definition("A", "头二", vec![field("类型", "BB")]),
    ];
    assert!(validate_definitions(&duplicate_ids)
        .unwrap_err()
        .to_string()
        .contains("重复"));
    let duplicate_names = [definition(
        "A",
        "头一",
        vec![field("类型", "AA"), field("类型", "BB")],
    )];
    assert!(validate_definitions(&duplicate_names)
        .unwrap_err()
        .to_string()
        .contains("重复"));
}
