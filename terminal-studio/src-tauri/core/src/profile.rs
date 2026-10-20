//! Device properties are explicitly bound to protocol fields, never inferred from display names.
use std::collections::{HashMap, HashSet};
use crate::{error::{AppError, AppResult}, fields::{is_variable_len, raw_field_value, render_fields, when_satisfied},
    models::{FieldEnc, FieldRole, Node, ProtocolField}, render::{compact_hex, DictMap, RenderCtx}};

pub fn key(field: &ProtocolField) -> Option<&str> {
    field.profile_key.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

pub fn validate_schema(fields: &[ProtocolField]) -> AppResult<()> {
    let mut keys = HashSet::new();
    for (i, f) in fields.iter().enumerate() {
        let Some(k) = key(f) else { continue };
        if k.len() > 64 || !k.as_bytes()[0].is_ascii_alphabetic()
            || !k.bytes().all(|c| c.is_ascii_alphanumeric() || b"_.-".contains(&c)) || !keys.insert(k) {
            return Err(AppError::msg(format!("字段[{}] 的设备属性 Key 无效或重复", f.name)));
        }
        if matches!(f.role, FieldRole::Session | FieldRole::Capture | FieldRole::TypeCode | FieldRole::Correlation | FieldRole::SrcId | FieldRole::DstId | FieldRole::DeviceSn | FieldRole::Result)
            || (f.value.contains('{') && !is_variable_len(f)) {
            return Err(AppError::msg(format!("字段[{}] 是报文运行时字段，不能绑定设备属性", f.name)));
        }
        if let Some(w) = &f.when {
            if !fields[..i].iter().any(|c| c.name == w.field && key(c).is_some()) {
                return Err(AppError::msg(format!("字段[{}] 的分支条件必须绑定前面的设备属性", f.name)));
            }
        }
        if is_variable_len(f) {
            let width: u32 = f.unit_fields.iter().map(|u| u.length).sum();
            if f.enc != FieldEnc::Hex || width == 0 || width != f.length
                || f.unit_fields.iter().any(|u| !u.unit_fields.is_empty() || u.when.is_some() || u.value.contains('{'))
                || !fields[..i].iter().any(|c| c.role == FieldRole::DeviceCount && key(c).is_some()) {
                return Err(AppError::msg(format!("字段[{}] 的设备列表需要已绑定的条目数及定长子字段", f.name)));
            }
        }
    }
    Ok(())
}

/// A profile can be shared by commands with different field labels and different constant headers.
pub fn params_for(fields: &[ProtocolField], properties: &HashMap<String, String>) -> AppResult<HashMap<String, String>> {
    validate_schema(fields)?;
    Ok(fields.iter().filter_map(|f| key(f).and_then(|k| properties.get(k)).map(|v| (f.name.clone(), v.clone()))).collect())
}

pub fn collect_properties(fields: &[ProtocolField], values: &HashMap<String, String>) -> HashMap<String, String> {
    fields.iter().filter_map(|f| {
        let k = key(f)?;
        let v = values.get(&f.name)?;
        Some((k.to_string(), if f.enc == FieldEnc::Hex || f.dict_key().is_some() { compact_hex(v) } else { v.clone() }))
    }).collect()
}

/// Validate a complete saved profile against ONE protocol, fill literal defaults, discard inactive branches.
pub fn normalize_properties(fields: &[ProtocolField], properties: &HashMap<String, String>, node: &Node, dicts: &DictMap) -> AppResult<HashMap<String, String>> {
    validate_schema(fields)?;
    let allowed: HashSet<&str> = fields.iter().filter_map(key).collect();
    for (k,v) in properties {
        if !allowed.contains(k.as_str()) { return Err(AppError::msg(format!("配置协议未声明设备属性 [{k}]"))); }
        if v.contains('{') { return Err(AppError::msg("设备资料只接受实际值，不能填写运行时变量")); }
    }
    normalize_fields(fields, properties, node, dicts)
}

/// One protocol's view of a device profile; a device covers every schema of its system.
pub struct ProfileSchema<'a> {
    pub protocol_name: &'a str,
    pub fields: &'a [ProtocolField],
    pub dicts: &'a DictMap,
}

/// Normalize a device profile against the WHOLE protocol system: every key is checked
/// against each protocol that declares it; disagreeing values or defaults are an
/// explicit error instead of a silent pick. Stored keys stay shared across protocols.
pub fn normalize_profile(schemas: &[ProfileSchema<'_>], properties: &HashMap<String, String>, node: &Node) -> AppResult<HashMap<String, String>> {
    if schemas.is_empty() { return Err(AppError::msg("该协议系统内尚无协议绑定设备属性")); }
    let mut declared: HashSet<&str> = HashSet::new();
    for s in schemas { declared.extend(s.fields.iter().filter_map(key)); }
    for (k,v) in properties {
        if !declared.contains(k.as_str()) { return Err(AppError::msg(format!("设备属性 [{k}] 未被该系统内任何协议声明"))); }
        if v.contains('{') { return Err(AppError::msg("设备资料只接受实际值，不能填写运行时变量")); }
    }
    let mut result: HashMap<String, String> = HashMap::new();
    let mut owner: HashMap<String, &str> = HashMap::new();
    for s in schemas {
        for (k, v) in normalize_fields(s.fields, properties, node, s.dicts)? {
            if let Some(prev) = result.get(&k) {
                if prev != &v {
                    let first = owner.get(&k).copied().unwrap_or("?");
                    let second = s.protocol_name;
                    return Err(AppError::msg(format!("设备属性 [{k}] 在协议「{first}」与「{second}」中的取值或默认值不一致，请显式填写或统一协议定义")));
                }
            } else {
                result.insert(k.clone(), v);
                owner.insert(k.clone(), s.protocol_name);
            }
        }
    }
    Ok(result)
}

/// Fill literal defaults, validate each stored value, discard branch-inactive fields.
/// Which keys are legal is the caller's decision (single protocol or system union).
fn normalize_fields(fields: &[ProtocolField], properties: &HashMap<String, String>, node: &Node, dicts: &DictMap) -> AppResult<HashMap<String, String>> {
    validate_schema(fields)?;
    let mut values = HashMap::new();
    let mut last_count = None;
    let mut result = HashMap::new();
    for f in fields {
        let Some(k) = key(f) else { continue };
        if !when_satisfied(fields, f, &values, dicts) { continue; }
        let mut check = f.clone(); check.when = None;
        let bytes = if is_variable_len(f) {
            let n: usize = last_count.ok_or_else(|| AppError::msg(format!("字段[{}] 缺少当前分支的条目数", f.name)))?;
            if n > 200 { return Err(AppError::msg("设备配置列表最多 200 条")); }
            let width = f.length as usize;
            let bytes = if let Some(v) = properties.get(k) {
                hex::decode(compact_hex(v)).map_err(|_| AppError::msg(format!("字段[{}] 不是合法 Hex", f.name)))?
            } else {
                let params = HashMap::new();
                let unit = render_fields(&f.unit_fields, &RenderCtx::new(node, 0, &params, dicts))?;
                unit.repeat(n)
            };
            if bytes.len() != width * n { return Err(AppError::msg(format!("字段[{}] 需 {n} 条、{} 字节，实际 {} 字节", f.name, width*n, bytes.len()))); }
            // Validate each stored record against the current unit schema, including dictionaries.
            for row in bytes.chunks(width) {
                let mut at = 0;
                for u in &f.unit_fields {
                    let end = at + u.length as usize;
                    let p = HashMap::from([(u.name.clone(), raw_field_value(u, &row[at..end]))]);
                    render_fields(std::slice::from_ref(u), &RenderCtx::new(node,0,&p,dicts))?;
                    at = end;
                }
            }
            bytes
        } else {
            if let Some(v) = properties.get(k) { check.value = v.clone(); }
            let params = HashMap::new();
            render_fields(&[check], &RenderCtx::new(node, 0, &params, dicts))?
        };
        let raw = raw_field_value(f, &bytes);
        if f.role == FieldRole::DeviceCount {
            last_count = Some(if f.enc == FieldEnc::Uint && f.dict_key().is_none() { raw.parse::<usize>().ok() }
                else { usize::from_str_radix(&compact_hex(&raw),16).ok() }.ok_or_else(|| AppError::msg("条目数无效"))?);
        }
        values.insert(f.name.clone(), raw.clone());
        result.insert(k.to_string(), if f.enc == FieldEnc::Hex || f.dict_key().is_some() { compact_hex(&raw) } else { raw });
    }
    Ok(result)
}
