//! Pure protocol inspection: no engine lock, transaction allocation, session
//! lookup, persistence, endpoint access, or send is involved in these methods.
use crate::{
    application::Studio,
    protocol::{ByteOrder, CompiledProtocol, Field, Message, Node, ValueKind, format_hex},
    runtime::{parse_code, resolve_path, resolve_send_values, resolve_value},
};
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

impl Studio {
    pub fn protocol_catalog(&self, protocol_id: &str) -> Result<Value, String> {
        let schema = self.schema(protocol_id)?;
        let mut budget = 100_000usize;
        let mut messages = Vec::new();
        for message in &schema.messages {
            let mut fields = Vec::new();
            let mut defaults = Map::new();
            let mut diagnostics = BTreeSet::new();
            for section in &message.sections {
                let (mut children, values) = catalog_nodes(
                    &schema,
                    &section.nodes,
                    section.kind.name(),
                    section.kind != crate::protocol::SectionKind::Body,
                    &mut budget,
                    &mut diagnostics,
                )?;
                if section.when.is_some() {
                    diagnostics.insert(
                        "条件字段未自动加入默认值；启用对应条件后，请按字段结构补齐。".to_owned(),
                    );
                    for field in &mut children {
                        field["sectionCondition"] = json!(section.when);
                    }
                } else if values.as_object().is_some_and(|values| !values.is_empty()) {
                    defaults.insert(section.kind.name().into(), values);
                }
                fields.extend(children);
            }
            messages.push(json!({
                "code":message.code,"id":message.id,"name":if message.name_zh.is_empty() {&message.id} else {&message.name_zh},
                "responseCodes":message.response_codes,"direction":message.direction,
                "fields":fields,"defaultValues":defaults,"diagnostics":diagnostics
            }));
        }
        Ok(
            json!({"protocolId":protocol_id,"name":schema.name_zh,"version":schema.version,"messages":messages}),
        )
    }
    pub fn preview_flow_node(&self, request: &Value) -> Result<Value, String> {
        let protocol_id = request["protocolId"].as_str().ok_or("预览缺少protocolId")?;
        let mut result = json!({"protocolId":protocol_id,"previewOnly":true,"missingVariables":[],"diagnostics":[],"expectedCodes":[],"placeholderFields":[]});
        let mut diagnostics = Vec::new();
        let mut env = request
            .get("variables")
            .cloned()
            .unwrap_or_else(|| json!({}));
        if !env.is_object() {
            diagnostics.push("预览variables必须是对象".to_owned());
            result["diagnostics"] = json!(diagnostics);
            return Ok(result);
        }
        env["response"] = request.get("response").cloned().unwrap_or(Value::Null);
        env["step"] = request.get("step").cloned().unwrap_or(Value::Null);
        let node = &request["node"];
        let kind = node["type"].as_str().unwrap_or("");
        if !matches!(kind, "SEND" | "SEND_AND_WAIT" | "WAIT_PACKET") {
            result["diagnostics"] = json!(["仅SEND、SEND_AND_WAIT和WAIT_PACKET节点支持报文预览"]);
            return Ok(result);
        }
        let raw = node.get("data").cloned().unwrap_or_else(|| json!({}));
        let mut missing = BTreeSet::new();
        missing_variables(&raw, &env, &mut missing);
        if !missing.is_empty() {
            result["missingVariables"] = json!(missing);
            result["diagnostics"] =
                json!(["存在未提供的变量；补齐运行变量或步骤/响应上下文后才能编码。"]);
            return Ok(result);
        }
        let data = resolve_value(&raw, &env)?;
        let actual_protocol = data["protocolVersionId"]
            .as_str()
            .or_else(|| data["protocolId"].as_str())
            .unwrap_or(protocol_id);
        let schema = self.schema(actual_protocol)?;
        result["protocolId"] = json!(actual_protocol);
        if actual_protocol != protocol_id {
            diagnostics
                .push("节点显式指定了协议版本，预览使用节点版本，与实际执行优先级一致。".into());
        }
        let expected = data
            .get("expectedCodes")
            .or_else(|| data.get("expectCodes"));
        if let Some(expected) = expected {
            match expected
                .as_array()
                .ok_or("expectedCodes必须是数组")
                .and_then(|codes| {
                    codes
                        .iter()
                        .map(parse_code)
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|_| "期望信令码非法")
                }) {
                Ok(codes) => result["expectedCodes"] = json!(codes),
                Err(error) => diagnostics.push(error.into()),
            }
        } else if kind != "SEND" {
            diagnostics.push("等待节点缺少expectedCodes，实际执行前必须填写。".into());
        }
        let mut values = match resolve_send_values(&data, &env) {
            Ok(values) => values,
            Err(error) => {
                diagnostics.push(error);
                result["diagnostics"] = json!(diagnostics);
                return Ok(result);
            }
        };
        if kind == "WAIT_PACKET" {
            result["values"] = values;
            diagnostics
                .push("WAIT_PACKET只建立接收条件，不编码或发送报文；此预览没有创建等待项。".into());
            result["diagnostics"] = json!(diagnostics);
            return Ok(result);
        }
        let Some(code) = data.get("code").or_else(|| data.get("requestMsgCode")) else {
            diagnostics.push("发送节点缺少信令码code".into());
            result["diagnostics"] = json!(diagnostics);
            return Ok(result);
        };
        let code = match parse_code(code) {
            Ok(code) => code,
            Err(error) => {
                diagnostics.push(error);
                result["diagnostics"] = json!(diagnostics);
                return Ok(result);
            }
        };
        let Some(message) = schema.messages.iter().find(|m| m.code == code) else {
            diagnostics.push(format!("当前协议没有信令码0x{code:02X}"));
            result["diagnostics"] = json!(diagnostics);
            return Ok(result);
        };
        let mut keys = BTreeSet::new();
        for section in &message.sections {
            field_keys(&schema, &section.nodes, &mut keys);
        }
        // The runtime inherits these common aliases. Irrelevant environment
        // aliases are not presented as invented fields in a generic protocol.
        for key in ["srcAddr", "dstAddr", "sessionId"] {
            if !keys.contains(key)
                && data["values"].get(key).is_none()
                && data["values"]["header"].get(key).is_none()
            {
                values.as_object_mut().unwrap().remove(key);
            }
        }
        if needs_header_transaction(&schema, message, &values) {
            values["transactionId"] = json!(1);
            result["placeholderFields"] = json!(["transactionId"]);
            diagnostics.push(
                "transactionId=1仅为预览占位；运行时会分配真实事务号，本次预览未占用任何事务。"
                    .into(),
            );
        }
        result["values"] = values.clone();
        match schema.encode(code, &values) {
            Ok(bytes) => match schema.decode(&bytes) {
                Ok(decoded) => {
                    result["hex"] = json!(format_hex(&bytes));
                    result["decoded"] = json!(decoded);
                }
                Err(error) => diagnostics.push(format!("编码后的解析校验失败：{error}")),
            },
            Err(error) => diagnostics.push(error.to_string()),
        }
        result["diagnostics"] = json!(diagnostics);
        Ok(result)
    }
}

/// Shared by preview and the actual codec host. A business field with the same
/// name does not identify a header transaction. Explicit values, including
/// canonical paths, fragment aliases and XML bindings, keep codec precedence.
pub(crate) fn needs_header_transaction(
    schema: &CompiledProtocol,
    message: &Message,
    values: &Value,
) -> bool {
    if values.get("transactionId").is_some() {
        return false;
    }
    let mut transactions = Vec::new();
    for section in &message.sections {
        if section.kind == crate::protocol::SectionKind::Header {
            header_transactions(
                schema,
                &section.nodes,
                "header",
                true,
                None,
                &mut transactions,
            );
        }
    }
    transactions.iter().any(|field| {
        resolve_path(values, &field.path).is_none()
            && field
                .alias
                .as_ref()
                .is_none_or(|path| resolve_path(values, path).is_none())
            && field
                .field
                .binding
                .as_ref()
                .is_none_or(|path| resolve_path(values, path).is_none())
    })
}

struct HeaderTransaction<'a> {
    path: String,
    alias: Option<String>,
    field: &'a Field,
}

fn header_transactions<'a>(
    schema: &'a CompiledProtocol,
    nodes: &'a [Node],
    path: &str,
    collapse: bool,
    alias_parent: Option<&str>,
    output: &mut Vec<HeaderTransaction<'a>>,
) {
    for node in nodes {
        match node {
            Node::Field(field) if field.key == "transactionId" => output.push(HeaderTransaction {
                path: format!("{path}.{}", field.key),
                alias: alias_parent.map(|parent| format!("{parent}.{}", field.key)),
                field,
            }),
            Node::Use(fragment) if fragment.enabled => {
                let child_path = if collapse {
                    path.into()
                } else {
                    format!("{path}.{}", fragment.reference)
                };
                header_transactions(
                    schema,
                    &schema.fragments[&fragment.reference].nodes,
                    &child_path,
                    collapse,
                    if collapse { None } else { Some(path) },
                    output,
                );
            }
            Node::Struct(object) => header_transactions(
                schema,
                &object.children,
                &format!("{path}.{}", object.key),
                false,
                None,
                output,
            ),
            _ => {}
        }
    }
}

fn missing_variables(value: &Value, env: &Value, missing: &mut BTreeSet<String>) {
    match value {
        Value::String(s) if s.starts_with("${") && s.ends_with('}') => {
            if resolve_path(env, s).is_none() {
                missing.insert(s[2..s.len() - 1].trim().to_owned());
            }
        }
        Value::Array(items) => items
            .iter()
            .for_each(|v| missing_variables(v, env, missing)),
        Value::Object(object) => object
            .values()
            .for_each(|v| missing_variables(v, env, missing)),
        _ => {}
    }
}

fn field_keys(schema: &CompiledProtocol, nodes: &[Node], keys: &mut BTreeSet<String>) {
    for node in nodes {
        match node {
            Node::Field(field) => {
                keys.insert(field.key.clone());
            }
            Node::Use(fragment) if fragment.enabled => {
                field_keys(schema, &schema.fragments[&fragment.reference].nodes, keys)
            }
            Node::Struct(object) => field_keys(schema, &object.children, keys),
            Node::Array(array) => {
                field_keys(schema, &array.children, keys);
                for variant in &array.variants {
                    field_keys(schema, &schema.fragments[&variant.reference].nodes, keys);
                }
            }
            _ => {}
        }
    }
}

fn catalog_nodes(
    schema: &CompiledProtocol,
    nodes: &[Node],
    path: &str,
    collapse: bool,
    budget: &mut usize,
    diagnostics: &mut BTreeSet<String>,
) -> Result<(Vec<Value>, Value), String> {
    let mut fields = Vec::new();
    let mut values = Map::new();
    for node in nodes {
        *budget = budget
            .checked_sub(1)
            .ok_or("协议目录展开字段数超过100000")?;
        match node {
            Node::Field(field) => {
                let field_path = format!("{path}.{}", field.key);
                let default = field
                    .fixed
                    .as_ref()
                    .or(field.default.as_ref())
                    .map(|bytes| field_default(field, bytes));
                let generated = field.computed.is_some()
                    || field.binding.as_deref() == Some("message.code")
                    || field.key == "msgCode"
                    || (field.key == "transactionId"
                        && (path == "header" || path.starts_with("header."))
                        && default.is_none());
                let required = default.is_none() && !generated && field.when.is_none();
                let mut descriptor = json!({"path":field_path,"key":field.key,"label":field.label_zh,"kind":"field","valueKind":field.value_kind,"bytes":field.bytes,"lengthExpression":field.length_expr,"endian":field.endian,"binding":field.binding,"computed":field.computed,"required":required,"generated":generated,"fixed":field.fixed.is_some(),"condition":field.when,"enums":field.enums,"bits":field.bits,"unit":field.unit,"scale":field.scale,"secret":field.secret,"description":field.description_zh,"min":field.min,"max":field.max});
                if let Some(default) = &default {
                    descriptor["defaultValue"] = default.clone();
                }
                if field.when.is_none() && !generated {
                    values.insert(
                        field.key.clone(),
                        default.unwrap_or_else(|| json!(format!("${{{field_path}}}"))),
                    );
                }
                if field.when.is_some() {
                    diagnostics.insert(
                        "条件字段未自动加入默认值；启用对应条件后，请按字段结构补齐。".into(),
                    );
                }
                fields.push(descriptor);
            }
            Node::Use(fragment) if fragment.enabled => {
                let child_path = if collapse {
                    path.to_owned()
                } else {
                    format!("{path}.{}", fragment.reference)
                };
                let (children, defaults) = catalog_nodes(
                    schema,
                    &schema.fragments[&fragment.reference].nodes,
                    &child_path,
                    collapse,
                    budget,
                    diagnostics,
                )?;
                if collapse {
                    fields.extend(children);
                    if fragment.when.is_none() {
                        values.extend(defaults.as_object().unwrap().clone());
                    }
                } else {
                    fields.push(json!({"path":child_path,"key":fragment.reference,"label":schema.fragments[&fragment.reference].name_zh,"kind":"struct","children":children,"condition":fragment.when}));
                    if fragment.when.is_none() {
                        values.insert(fragment.reference.clone(), defaults);
                    }
                }
                if fragment.when.is_some() {
                    diagnostics.insert(
                        "条件字段未自动加入默认值；启用对应条件后，请按字段结构补齐。".into(),
                    );
                }
            }
            Node::Struct(object) => {
                let child_path = format!("{path}.{}", object.key);
                let (children, defaults) = catalog_nodes(
                    schema,
                    &object.children,
                    &child_path,
                    false,
                    budget,
                    diagnostics,
                )?;
                fields.push(json!({"path":child_path,"key":object.key,"label":object.label_zh,"kind":"struct","bytes":object.bytes,"children":children,"condition":object.when}));
                if object.when.is_none() {
                    values.insert(object.key.clone(), defaults);
                } else {
                    diagnostics.insert(
                        "条件字段未自动加入默认值；启用对应条件后，请按字段结构补齐。".into(),
                    );
                }
            }
            Node::Array(array) => {
                let array_path = format!("{path}.{}", array.key);
                let item_path = format!("{array_path}[0]");
                let (children, template) = catalog_nodes(
                    schema,
                    &array.children,
                    &item_path,
                    false,
                    budget,
                    diagnostics,
                )?;
                let mut variants = Vec::new();
                for variant in &array.variants {
                    let (children, template) = catalog_nodes(
                        schema,
                        &schema.fragments[&variant.reference].nodes,
                        &item_path,
                        false,
                        budget,
                        diagnostics,
                    )?;
                    variants.push(json!({"selector":variant.selector,"equals":variant.equals,"reference":variant.reference,"children":children,"itemTemplate":template}));
                }
                fields.push(json!({"path":array_path,"key":array.key,"label":array.label_zh,"kind":"array","countExpression":array.count,"elementSizeExpression":array.element_size,"maxItems":array.max_items,"condition":array.when,"children":children,"itemTemplate":template,"variants":variants}));
                if array.when.is_none() {
                    values.insert(array.key.clone(), json!([]));
                }
                diagnostics.insert("数组默认留空；可按itemTemplate或对应variant的结构添加条目，条目数由实际输入校验。".into());
            }
            Node::Use(_) => {}
        }
    }
    Ok((fields, Value::Object(values)))
}

fn field_default(field: &Field, bytes: &[u8]) -> Value {
    match field.value_kind {
        ValueKind::Ascii => json!(String::from_utf8_lossy(bytes)),
        ValueKind::Bcd => json!(format_hex(bytes).replace(' ', "")),
        ValueKind::Bytes => json!(format_hex(bytes)),
        ValueKind::Uint | ValueKind::Int | ValueKind::Bitset => {
            if bytes.is_empty() || bytes.len() > 8 {
                return json!(format_hex(bytes));
            }
            let number = if field.endian == ByteOrder::Little {
                bytes
                    .iter()
                    .rev()
                    .fold(0u64, |n, byte| (n << 8) | u64::from(*byte))
            } else {
                bytes
                    .iter()
                    .fold(0u64, |n, byte| (n << 8) | u64::from(*byte))
            };
            if field.value_kind == ValueKind::Int {
                let shift = 64 - bytes.len() * 8;
                let signed = ((number << shift) as i64) >> shift;
                if (signed as i128).abs() > 9_007_199_254_740_991 {
                    json!(format_hex(bytes))
                } else {
                    json!(signed)
                }
            } else if number > 9_007_199_254_740_991 {
                json!(format_hex(bytes))
            } else {
                json!(number)
            }
        }
    }
}
