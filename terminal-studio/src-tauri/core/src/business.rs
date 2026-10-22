use crate::{
    error::{AppError, AppResult},
    fields::{extract_field_values, is_variable_len, when_satisfied},
    models::{FieldEnc, ProtocolField},
    render::{compact_hex, DictMap},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Mapping {
    pub property: String,
    pub source: String,
    pub field: String,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SaveRule {
    pub request: String,
    pub response: String,
    pub kind: String,
    #[serde(default = "default_operation")]
    pub operation: String,
    #[serde(default = "default_scope")]
    pub key_scope: String,
    pub request_array: String,
    pub response_array: String,
    pub pairing: String,
    pub request_match: String,
    pub response_match: String,
    pub request_sequence: String,
    pub response_sequence: String,
    pub serial: bool,
    pub timeout_ms: u64,
    pub success_field: String,
    pub success_hex: String,
    pub identity: String,
    pub name_field: String,
    pub device_field: String,
    pub mappings: Vec<Mapping>,
}
fn default_operation() -> String { "upsert".into() }
fn default_scope() -> String { "channel".into() }

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct RecordType {
    pub kind: String,
    pub name: String,
    pub device_required: bool,
    pub key_scope: String,
    pub properties: Vec<String>,
}
impl RecordType {
    pub fn validate(&self) -> AppResult<()> {
        let valid = |s: &str| !s.is_empty() && s.trim() == s && s.len() <= 96 && !s.chars().any(char::is_control);
        if !valid(&self.kind) || !valid(&self.name) { return Err(AppError::msg("类型标识和名称不能为空，最长 96 字节")); }
        if !["channel", "device"].contains(&self.key_scope.as_str()) || (self.key_scope == "device" && !self.device_required) {
            return Err(AppError::msg("设备内唯一的资料须关联设备"));
        }
        if self.kind == "device" && (self.device_required || self.key_scope != "channel") { return Err(AppError::msg("设备档案自身不关联所属设备")); }
        let mut names = HashSet::new();
        if self.properties.len() > 128 || self.properties.iter().any(|p| !valid(p) || !names.insert(p)) { return Err(AppError::msg("属性名称不能为空或重复，最多 128 项")); }
        Ok(())
    }
    pub fn check_rule(&self, rule: &SaveRule) -> AppResult<()> {
        self.validate()?;
        if rule.key_scope != self.key_scope { return Err(AppError::msg("资料唯一范围已变更，请重新选择资料类型")); }
        if self.device_required && rule.device_field.is_empty() { return Err(AppError::msg("此资料类型必须关联所属设备")); }
        if rule.operation != "delete" && rule.mappings.iter().any(|m| !self.properties.contains(&m.property)) { return Err(AppError::msg("保存属性不在资料类型定义中，请先维护资料类型")); }
        Ok(())
    }
}

pub fn row_identity(rule: &SaveRule, row: &Row) -> AppResult<String> {
    let key = &get(row, &rule.identity)?.hex;
    Ok(if rule.key_scope == "device" { format!("{}:{key}", get(row, &rule.device_field)?.hex) } else { key.clone() })
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cell {
    pub value: String,
    pub hex: String,
    pub enc: FieldEnc,
    pub bytes: u32,
}
pub type Row = BTreeMap<String, Cell>;
#[derive(Clone, Debug)]
pub struct Write {
    pub key: String,
    pub name: String,
    pub device: Option<String>,
    pub properties: BTreeMap<String, Cell>,
}

pub fn get<'a>(row: &'a Row, field: &str) -> AppResult<&'a Cell> {
    row.get(field)
        .ok_or_else(|| AppError::msg(format!("保存规则字段 [{field}] 在本次报文中不存在")))
}

pub fn evaluate(rule: &SaveRule, request: &[Row], response: &[Row]) -> AppResult<Vec<Write>> {
    if rule.response_array.is_empty() {
        if response.len() != 1 {
            return Err(AppError::msg("整体应答必须恰好包含一组结果"));
        }
    } else if request.len() != response.len() {
        return Err(AppError::msg(format!(
            "请求 {} 条、应答 {} 条，数量不一致，未保存",
            request.len(),
            response.len()
        )));
    }
    let mut responses = BTreeMap::new();
    if !rule.response_array.is_empty() && rule.pairing == "key" {
        for row in response {
            let k = get(row, &rule.response_match)?.hex.clone();
            if responses.insert(k, row).is_some() {
                return Err(AppError::msg("应答条目标识重复，未保存"));
            }
        }
    }
    let mut identities = HashSet::new();
    let mut paired = HashSet::new();
    let mut writes = Vec::new();
    for (i, row) in request.iter().enumerate() {
        let key = get(row, &rule.identity)?.hex.clone();
        if key.is_empty() || !identities.insert(row_identity(rule, row)?) {
            return Err(AppError::msg("请求资料唯一标识为空或重复，未保存"));
        }
        let ack = if rule.response_array.is_empty() {
            &response[0]
        } else if rule.pairing == "key" {
            let k = &get(row, &rule.request_match)?.hex;
            if !paired.insert(k.clone()) {
                return Err(AppError::msg("请求条目标识重复，未保存"));
            }
            responses
                .get(k)
                .copied()
                .ok_or_else(|| AppError::msg("应答缺少对应条目标识，未保存"))?
        } else {
            &response[i]
        };
        if get(ack, &rule.success_field)?.hex != compact_hex(&rule.success_hex) {
            continue;
        }
        let mut properties = BTreeMap::new();
        for m in rule.mappings.iter().filter(|_| rule.operation != "delete") {
            let source = if m.source == "request" { row } else { ack };
            properties.insert(m.property.clone(), get(source, &m.field)?.clone());
        }
        let name = if rule.name_field.is_empty() || rule.operation == "delete" {
            key.clone()
        } else {
            get(row, &rule.name_field)?.value.clone()
        };
        let device = if rule.device_field.is_empty() {
            None
        } else {
            Some(get(row, &rule.device_field)?.hex.clone())
        };
        writes.push(Write {
            key,
            name,
            device,
            properties,
        });
    }
    Ok(writes)
}

pub fn catalog(
    fields: &[ProtocolField],
    array: &str,
) -> AppResult<BTreeMap<String, ProtocolField>> {
    let mut out = BTreeMap::new();
    for f in fields
        .iter()
        .filter(|f| f.unit_fields.is_empty() && !is_variable_len(f))
    {
        if out.insert(format!("frame.{}", f.name), f.clone()).is_some() {
            return Err(AppError::msg("字段重名，请先消除歧义"));
        }
    }
    if !array.is_empty() {
        let f = fields
            .iter()
            .find(|f| f.name == array && !f.unit_fields.is_empty())
            .ok_or_else(|| AppError::msg("选择的数组不存在"))?;
        if f.unit_fields
            .iter()
            .any(|u| !u.unit_fields.is_empty() || u.when.is_some() || is_variable_len(u))
            || f.unit_fields.iter().map(|u| u.length).sum::<u32>() != f.length
        {
            return Err(AppError::msg(
                "保存数组须为定长、无嵌套、无子字段条件的条目",
            ));
        }
        for u in &f.unit_fields {
            if out.insert(format!("item.{}", u.name), u.clone()).is_some() {
                return Err(AppError::msg("数组子字段重名"));
            }
        }
    }
    Ok(out)
}

pub fn validate(
    rule: &SaveRule,
    request: &[ProtocolField],
    response: &[ProtocolField],
) -> AppResult<()> {
    if rule.request == rule.response
        || rule.kind.trim().is_empty()
        || rule.kind.trim() != rule.kind
        || rule.kind.len() > 96
    {
        return Err(AppError::msg("请指定不同的请求/应答协议以及资料类型"));
    }
    if !(100..=300000).contains(&rule.timeout_ms) {
        return Err(AppError::msg("等待时间应为 100～300000 毫秒"));
    }
    if !["upsert", "delete"].contains(&rule.operation.as_str()) || !["channel", "device"].contains(&rule.key_scope.as_str()) { return Err(AppError::msg("资料操作或唯一范围无效")); }
    if rule.key_scope == "device" && rule.device_field.is_empty() { return Err(AppError::msg("设备内唯一的资料须指定所属设备")); }
    if rule.kind == "device" && rule.key_scope != "channel" { return Err(AppError::msg("设备标识须在通道和系统内唯一")); }
    if !["index", "key"].contains(&rule.pairing.as_str()) {
        return Err(AppError::msg("条目对应方式无效"));
    }
    let req = catalog(request, &rule.request_array)?;
    let resp = catalog(response, &rule.response_array)?;
    let field = |map: &BTreeMap<String, ProtocolField>, name: &str| -> AppResult<ProtocolField> {
        map.get(name)
            .cloned()
            .ok_or_else(|| AppError::msg(format!("请选择有效字段：{name}")))
    };
    field(&req, &rule.identity)?;
    if !rule.name_field.is_empty() {
        field(&req, &rule.name_field)?;
    }
    if rule.kind == "device" && !rule.device_field.is_empty() {
        return Err(AppError::msg("设备资料不能关联自身作为所属设备"));
    }
    if !rule.device_field.is_empty() {
        field(&req, &rule.device_field)?;
    }
    let result = field(&resp, &rule.success_field)?;
    let success = hex::decode(compact_hex(&rule.success_hex))
        .map_err(|_| AppError::msg("成功值请输入 Hex"))?;
    if success.len() != result.length as usize {
        return Err(AppError::msg("成功值长度须与结果字段一致"));
    }
    if !rule.serial {
        if !rule.request_sequence.starts_with("frame.")
            || !rule.response_sequence.starts_with("frame.")
        {
            return Err(AppError::msg("流水号须选择公共头或净荷的单值字段"));
        }
        if field(&req, &rule.request_sequence)?.length
            != field(&resp, &rule.response_sequence)?.length
        {
            return Err(AppError::msg("请求/应答流水号长度不一致"));
        }
    }
    if !rule.response_array.is_empty() && rule.pairing == "key" {
        if field(&req, &rule.request_match)?.length != field(&resp, &rule.response_match)?.length {
            return Err(AppError::msg("条目标识长度不一致"));
        }
    }
    let mut props = HashSet::new();
    if rule.mappings.is_empty() && rule.operation != "delete" {
        return Err(AppError::msg("至少配置一个保存属性"));
    }
    for m in rule.mappings.iter().filter(|_| rule.operation != "delete") {
        if m.property.trim().is_empty()
            || m.property.trim() != m.property
            || m.property.len() > 96
            || !props.insert(&m.property)
        {
            return Err(AppError::msg("属性名称不能为空或重复"));
        }
        let map = match m.source.as_str() {
            "request" => &req,
            "response" => &resp,
            _ => return Err(AppError::msg("属性来源无效")),
        };
        field(map, &m.field)?;
    }
    Ok(())
}

fn cell(field: &ProtocolField, value: &str, raw: &[u8]) -> AppResult<Cell> {
    let enc = if field.dict_key().is_some() {
        FieldEnc::Hex
    } else {
        field.enc
    };
    let hex = hex::encode_upper(raw);
    let value = match enc {
        FieldEnc::Hex => hex.clone(),
        FieldEnc::Ascii => std::str::from_utf8(raw)
            .map_err(|_| AppError::msg(format!("字段 {} 含无效文本字节，未保存", field.name)))?
            .trim_end_matches('\0')
            .to_owned(),
        FieldEnc::Uint => value.to_owned(),
    };
    Ok(Cell {
        value,
        hex,
        enc,
        bytes: raw.len() as u32,
    })
}

/// Parse only complete frames. Save rules read actual wire values, never defaults.
pub fn snapshot(
    fields: &[ProtocolField],
    data: &[u8],
    dicts: &DictMap,
    array: &str,
) -> AppResult<Vec<Row>> {
    catalog(fields, array)?;
    let values = extract_field_values(fields, data, dicts);
    let mut root = Row::new();
    let mut total = 0usize;
    for f in fields {
        if !when_satisfied(fields, f, &values, dicts) {
            continue;
        }
        let v = values
            .get(&f.name)
            .ok_or_else(|| AppError::msg(format!("报文不完整：{}", f.name)))?;
        let size = if is_variable_len(f) {
            if f.enc != FieldEnc::Hex {
                return Err(AppError::msg("业务保存的变长字段须使用 HEX 编码"));
            }
            hex::decode(compact_hex(v))
                .map_err(|_| AppError::msg("变长字段无法读取"))?
                .len()
        } else {
            f.length as usize
        };
        let raw = data
            .get(total..total + size)
            .ok_or_else(|| AppError::msg("报文字段不完整"))?;
        let c = cell(f, v, raw)?;
        total += size;
        root.insert(format!("frame.{}", f.name), c);
    }
    if total != data.len() {
        return Err(AppError::msg("报文实际长度与协议定义不一致，未保存"));
    }
    if array.is_empty() {
        return Ok(vec![root]);
    }
    let f = fields
        .iter()
        .find(|f| f.name == array)
        .ok_or_else(|| AppError::msg("数组不存在"))?;
    let raw = hex::decode(&get(&root, &format!("frame.{array}"))?.hex)
        .map_err(|_| AppError::msg("数组数据无效"))?;
    if f.length == 0 || raw.len() % f.length as usize != 0 {
        return Err(AppError::msg("数组条目不完整"));
    }
    let mut rows = Vec::new();
    for chunk in raw.chunks(f.length as usize) {
        let vals = extract_field_values(&f.unit_fields, chunk, dicts);
        let mut row = root.clone();
        let mut offset = 0usize;
        for u in &f.unit_fields {
            let v = vals
                .get(&u.name)
                .ok_or_else(|| AppError::msg("数组字段不完整"))?;
            row.insert(
                format!("item.{}", u.name),
                cell(u, v, &chunk[offset..offset + u.length as usize])?,
            );
            offset += u.length as usize;
        }
        rows.push(row);
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshot_preserves_exact_text_bytes_and_rejects_lossy_text() {
        let f: ProtocolField =
            serde_json::from_value(serde_json::json!({"name":"result","len":1,"enc":"ascii"}))
                .unwrap();
        assert!(snapshot(&[f.clone()], &[0xFF], &DictMap::new(), "").is_err());
        let rows = snapshot(&[f], &[0x41], &DictMap::new(), "").unwrap();
        assert_eq!(rows[0]["frame.result"].hex, "41");
        assert_eq!(rows[0]["frame.result"].value, "A");
    }
    fn cell(n: u8) -> Cell {
        Cell {
            value: format!("{n:02X}"),
            hex: format!("{n:02X}"),
            enc: FieldEnc::Hex,
            bytes: 1,
        }
    }
    fn row(id: u8, result: u8) -> Row {
        BTreeMap::from([
            ("item.id".into(), cell(id)),
            ("item.result".into(), cell(result)),
        ])
    }
    fn rule() -> SaveRule {
        SaveRule {
            request: "request".into(),
            response: "response".into(),
            kind: "device".into(),
            operation: "upsert".into(),
            key_scope: "channel".into(),
            request_array: "items".into(),
            response_array: "results".into(),
            pairing: "index".into(),
            request_match: String::new(),
            response_match: String::new(),
            request_sequence: String::new(),
            response_sequence: String::new(),
            serial: true,
            timeout_ms: 3000,
            success_field: "item.result".into(),
            success_hex: "AA".into(),
            identity: "item.id".into(),
            name_field: String::new(),
            device_field: String::new(),
            mappings: vec![Mapping {
                property: "address".into(),
                source: "request".into(),
                field: "item.id".into(),
            }],
        }
    }
    #[test]
    fn partial_success_saves_only_successful_item() {
        let writes =
            evaluate(&rule(), &[row(1, 0), row(2, 0)], &[row(1, 0xAA), row(2, 0)]).unwrap();
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].key, "01");
    }
    #[test]
    fn device_scoped_identity_and_delete_operation() {
        let mut raw = serde_json::to_value(rule()).unwrap();
        raw["kind"] = "port".into();
        raw["device_field"] = "item.device".into();
        raw["key_scope"] = "device".into();
        raw["operation"] = "delete".into();
        // Deleting is an identity-only operation; stale property mappings must not be read.
        raw["mappings"][0]["field"] = "item.missing".into();
        let r: SaveRule = serde_json::from_value(raw).unwrap();
        let mut first = row(1, 0);
        first.insert("item.device".into(), cell(10));
        let mut second = row(1, 0);
        second.insert("item.device".into(), cell(11));
        let writes = evaluate(&r, &[first, second], &[row(1, 0xAA), row(1, 0xAA)]).unwrap();
        assert_eq!(writes.len(), 2);
        assert!(writes[0].properties.is_empty());
        assert_eq!(writes[0].device.as_deref(), Some("0A"));
        assert_eq!(writes[1].device.as_deref(), Some("0B"));
    }
    #[test]
    fn mismatch_and_duplicate_identity_rejected() {
        assert!(evaluate(&rule(), &[row(1, 0), row(2, 0)], &[row(1, 0xAA)]).is_err());
        assert!(evaluate(
            &rule(),
            &[row(1, 0), row(1, 0)],
            &[row(1, 0xAA), row(1, 0xAA)]
        )
        .is_err());
    }
    #[test]
    fn key_pairing_handles_reordered_results() {
        let mut r = rule();
        r.pairing = "key".into();
        r.request_match = "item.id".into();
        r.response_match = "item.id".into();
        let writes = evaluate(&r, &[row(1, 0), row(2, 0)], &[row(2, 0xAA), row(1, 0)]).unwrap();
        assert_eq!(writes.len(), 1);
        assert_eq!(writes[0].key, "02");
    }
    #[test]
    fn phone_requires_explicit_device_link() {
        let mut r = rule();
        r.kind = "phone".into();
        r.device_field = "item.id".into();
        assert_eq!(
            evaluate(&r, &[row(1, 0)], &[row(1, 0xAA)]).unwrap()[0]
                .device
                .as_deref(),
            Some("01")
        );
    }
}
