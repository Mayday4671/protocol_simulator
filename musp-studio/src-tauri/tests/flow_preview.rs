use musp_studio::{
    application::Studio,
    protocol::compile_xml,
    runtime::{FlowAction, FlowDefinition},
};
use serde_json::{Value, json};
use std::sync::Arc;

const XML: &str = r#"<protocol id="sensor-demo" nameZh="传感器协议" version="2" schemaVersion="1" byteOrder="little">
<fragments><fragment id="common"><field key="marker" bytes="1" fixed="D1"/><field key="opcode" bytes="1" binding="message.code"/><field key="transactionId" bytes="2"/></fragment>
<fragment id="entry"><field key="reading" bytes="2" valueKind="int"/><field key="enabled" bytes="1" default="01"/></fragment></fragments>
<messages><message id="SET_SENSOR" code="0xA4" nameZh="写入传感器" responseCodes="0xA5">
<header ref="common"/><body><field key="mode" bytes="1" default="02"/><field key="label" bytes="2" valueKind="ascii" default="4F 4B"/><field key="count" bytes="1" default="00"/><array key="samples" countRef="count" elementBytes="3"><use ref="entry"/></array><struct key="settings"><field key="gain" bytes="2" default="34 12"/></struct></body></message>
<message id="RESULT" code="0xA5" nameZh="传感器结果"><header ref="common"/><body><field key="status" bytes="1" default="00"/></body></message></messages></protocol>"#;

fn fixture() -> (tempfile::TempDir, Arc<Studio>) {
    let directory = tempfile::tempdir().unwrap();
    let (app, _) = Studio::open(directory.path()).unwrap();
    app.schemas
        .write()
        .unwrap()
        .insert("custom-version".into(), Arc::new(compile_xml(XML).unwrap()));
    (directory, app)
}
fn node(values: Value) -> Value {
    json!({"id":"send","type":"SEND_AND_WAIT","data":{"code":"0xA4","values":values,"expectedCodes":["0xA5"]}})
}

#[test]
fn catalog_uses_arbitrary_xml_messages_fragments_defaults_and_array_structure() {
    let (_directory, app) = fixture();
    let result = app.protocol_catalog("custom-version").unwrap();
    let messages = result["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 2);
    let message = &messages[0];
    assert_eq!(message["code"], 0xA4);
    assert_eq!(message["name"], "写入传感器");
    assert_eq!(message["responseCodes"], json!([0xA5]));
    assert_eq!(message["defaultValues"]["body"]["label"], "OK");
    assert_eq!(message["defaultValues"]["body"]["settings"]["gain"], 0x1234);
    assert_eq!(message["defaultValues"]["body"]["samples"], json!([]));
    let samples = message["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["path"] == "body.samples")
        .unwrap();
    assert_eq!(samples["kind"], "array");
    assert_eq!(samples["children"][0]["path"], "body.samples[0].entry");
    assert_eq!(samples["itemTemplate"]["entry"]["enabled"], 1);
    assert_eq!(
        samples["itemTemplate"]["entry"]["reading"],
        "${body.samples[0].entry.reading}"
    );
}

#[test]
fn preview_uses_runtime_resolution_with_arrays_response_step_and_header_precedence() {
    let (_directory, app) = fixture();
    let values = json!({"header":{"transactionId":42},"body":{"label":"${step.label}","mode":"${response.body.mode}","samples":[{"entry":{"reading":"${samples[0].value}"}}]}});
    let result = app.preview_flow_node(&json!({"protocolId":"custom-version","node":node(values),"variables":{"samples":[{"value":-12}],"transactionId":99},"response":{"body":{"mode":3}},"step":{"label":"Hi"}})).unwrap();
    assert_eq!(result["missingVariables"], json!([]));
    assert_eq!(result["placeholderFields"], json!([]));
    assert_eq!(result["expectedCodes"], json!([0xA5]));
    let expected = app.schema("custom-version").unwrap().encode(0xA4, &json!({"transactionId":42,"body":{"label":"Hi","mode":3,"samples":[{"entry":{"reading":-12}}]}})).unwrap();
    assert_eq!(result["hex"], musp_studio::protocol::format_hex(&expected));
    assert_eq!(
        result["decoded"]["values"]["body"]["samples"][0]["entry"]["reading"],
        -12
    );
}

#[test]
fn missing_variables_are_all_reported_without_zero_substitution_or_partial_hex() {
    let (_directory, app) = fixture();
    let result = app.preview_flow_node(&json!({"protocolId":"custom-version","node":node(json!({"transactionId":"${session.txn}","body":{"label":"${name}","mode":"${missing}","samples":[{"entry":{"reading":"${missing}"}}]}})),"variables":{}})).unwrap();
    assert_eq!(
        result["missingVariables"],
        json!(["missing", "name", "session.txn"])
    );
    assert!(result.get("hex").is_none());
    assert!(
        result["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d.as_str().unwrap().contains("变量"))
    );
}

#[test]
fn preview_dynamic_transaction_is_explicit_and_does_not_consume_runtime_state() {
    let (_directory, app) = fixture();
    let before_domain = serde_json::to_value(&*app.domain.lock().unwrap()).unwrap();
    for _ in 0..3 {
        let result = app.preview_flow_node(&json!({"protocolId":"custom-version","node":node(json!({"body":{"samples":[]}})),"variables":{}})).unwrap();
        assert_eq!(result["placeholderFields"], json!(["transactionId"]));
        assert_eq!(result["decoded"]["values"]["header"]["transactionId"], 1);
        assert!(!result["diagnostics"].as_array().unwrap().is_empty());
    }
    assert_eq!(
        serde_json::to_value(&*app.domain.lock().unwrap()).unwrap(),
        before_domain
    );
    let mut execution = app.execution.lock().unwrap();
    assert!(execution.engine.pending.requests.is_empty());
    assert!(execution.engine.runs.is_empty());
    let definition: FlowDefinition = serde_json::from_value(json!({"id":"preview-counter","nodes":[{"id":"start","type":"START"},{"id":"send","type":"SEND","data":{"code":164,"values":{},"endpointId":"local","protocolVersionId":"custom-version","remote":"127.0.0.1:9000"}},{"id":"end","type":"END"}],"edges":[{"source":"start","target":"send"},{"source":"send","target":"end"}]})).unwrap();
    execution.engine.publish(definition).unwrap();
    execution
        .engine
        .start("preview-counter", json!({}), 1)
        .unwrap();
    let sends = execution.engine.poll(1);
    let send = sends
        .iter()
        .find_map(|a| {
            if let FlowAction::Send(s) = a {
                Some(s)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(send.values["transactionId"], 1);
}

#[test]
fn wait_preview_does_not_encode_and_missing_required_field_is_a_codec_diagnostic() {
    let (_directory, app) = fixture();
    let waiting = app.preview_flow_node(&json!({"protocolId":"custom-version","node":{"id":"wait","type":"WAIT_PACKET","data":{"expectedCodes":[164,165],"values":{}}},"variables":{}})).unwrap();
    assert_eq!(waiting["expectedCodes"], json!([164, 165]));
    assert!(waiting.get("hex").is_none());
    let missing = app.preview_flow_node(&json!({"protocolId":"custom-version","node":node(json!({"body":{"samples":[{"entry":{}}]}})),"variables":{}})).unwrap();
    assert!(missing.get("hex").is_none());
    assert!(
        missing["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d.as_str().unwrap().contains("reading"))
    );
}

#[test]
fn protocol_without_transaction_or_musp_header_has_no_fabricated_fields() {
    let (_directory, app) = fixture();
    let xml = r#"<protocol id="simple" version="1" schemaVersion="1" byteOrder="big"><messages><message id="PING" code="0x49" nameZh="自定义探测"><header><field key="opcode" bytes="1" binding="message.code"/></header><body><field key="value" bytes="1"/></body></message></messages></protocol>"#;
    app.schemas
        .write()
        .unwrap()
        .insert("simple".into(), Arc::new(compile_xml(xml).unwrap()));
    let result = app.preview_flow_node(&json!({"protocolId":"simple","node":{"id":"send","type":"SEND","data":{"code":73,"values":{"body":{"value":7}}}},"variables":{}})).unwrap();
    assert_eq!(result["hex"], "49 07");
    assert_eq!(result["placeholderFields"], json!([]));
    assert!(result["values"].get("transactionId").is_none());
    assert!(result["values"].get("srcAddr").is_none());
    assert!(result["values"].get("header").is_none());
}

#[test]
fn business_transaction_field_is_not_presented_as_a_runtime_header_transaction() {
    let (_directory, app) = fixture();
    let xml = r#"<protocol id="business-key" version="1" schemaVersion="1" byteOrder="big"><messages><message id="BUSINESS" code="0x49"><header><field key="opcode" bytes="1" binding="message.code"/></header><body><field key="transactionId" bytes="2"/></body></message></messages></protocol>"#;
    app.schemas
        .write()
        .unwrap()
        .insert("business-key".into(), Arc::new(compile_xml(xml).unwrap()));
    let result = app.preview_flow_node(&json!({"protocolId":"business-key","node":{"type":"SEND","data":{"code":73,"values":{}}}})).unwrap();
    assert_eq!(result["placeholderFields"], json!([]));
    assert!(result.get("hex").is_none());
    let catalog = app.protocol_catalog("business-key").unwrap();
    assert_eq!(
        catalog["messages"][0]["defaultValues"]["body"]["transactionId"],
        "${body.transactionId}"
    );
    let explicit = app.preview_flow_node(&json!({"protocolId":"business-key","node":{"type":"SEND","data":{"code":73,"values":{"body":{"transactionId":9}}}}})).unwrap();
    assert_eq!(explicit["hex"], "49 00 09");
    assert_eq!(explicit["placeholderFields"], json!([]));
}

#[test]
fn explicit_schema_transaction_binding_has_precedence_without_placeholder_claim() {
    let (_directory, app) = fixture();
    let xml = XML.replace(
        "key=\"transactionId\" bytes=\"2\"",
        "key=\"transactionId\" bytes=\"2\" binding=\"flow.transactionId\"",
    );
    app.schemas.write().unwrap().insert(
        "bound-transaction".into(),
        Arc::new(compile_xml(&xml).unwrap()),
    );
    let result = app.preview_flow_node(&json!({"protocolId":"bound-transaction","node":node(json!({"flow":{"transactionId":23},"body":{"samples":[]}}))})).unwrap();
    assert_eq!(result["placeholderFields"], json!([]));
    assert_eq!(result["decoded"]["values"]["header"]["transactionId"], 23);
}

#[test]
fn large_integer_defaults_survive_javascript_json_without_precision_loss() {
    let (_directory, app) = fixture();
    let xml = XML.replace(
        "key=\"gain\" bytes=\"2\" default=\"34 12\"",
        "key=\"gain\" bytes=\"8\" default=\"FE FF FF FF FF FF FF FF\"",
    );
    app.schemas
        .write()
        .unwrap()
        .insert("wide-integer".into(), Arc::new(compile_xml(&xml).unwrap()));
    let catalog = app.protocol_catalog("wide-integer").unwrap();
    assert_eq!(
        catalog["messages"][0]["defaultValues"]["body"]["settings"]["gain"],
        "FE FF FF FF FF FF FF FF"
    );
}
