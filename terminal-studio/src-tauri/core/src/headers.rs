//! 分组公共头定义和协议有序组合；不通过字段名推测头的选择。
use std::collections::HashSet;
use crate::{error::{AppError, AppResult}, models::{FieldSection, HeaderDefinition, ProtocolField, ProtocolGroup}};

pub const LEGACY_HEADER_ID: &str = "legacy-default";

pub fn validate_definitions(headers: &[HeaderDefinition]) -> AppResult<()> {
    let mut ids = HashSet::new();
    for header in headers {
        if header.id.is_empty() || header.id.trim() != header.id || header.id.contains(',') || header.id.contains("::") {
            return Err(AppError::msg(format!("公共头标识 [{}] 非法：不能为空、含首尾空白、逗号或 ::", header.id)));
        }
        if !ids.insert(&header.id) { return Err(AppError::msg(format!("公共头标识 [{}] 重复", header.id))); }
        if header.name.trim().is_empty() { return Err(AppError::msg(format!("公共头 [{}] 缺少名称", header.id))); }
        if header.fields.is_empty() { return Err(AppError::msg(format!("公共头 [{}] 至少需要一个字段", header.name))); }
        let mut names = HashSet::new();
        for field in &header.fields {
            if field.name.trim().is_empty() || (!names.insert(&field.name) && header.id != LEGACY_HEADER_ID) {
                return Err(AppError::msg(format!("公共头 [{}] 字段名为空或重复: [{}]", header.name, field.name)));
            }
            if field.length == 0 || field.length > 65535 {
                return Err(AppError::msg(format!("公共头 [{}] 字段 [{}] 长度非法", header.name, field.name)));
            }
            if field.hdr_ref.as_deref().is_some_and(|s| !s.trim().is_empty()) {
                return Err(AppError::msg(format!("公共头 [{}] 的字段不能再次覆盖公共头", header.name)));
            }
        }
    }
    Ok(())
}

pub fn group_definitions(group: &ProtocolGroup) -> AppResult<Vec<HeaderDefinition>> {
    let headers = match group.header_profiles_json.as_deref() {
        Some(json) => serde_json::from_str(json).map_err(|e| AppError::msg(format!("分组 [{}] 公共头定义无效: {e}", group.name)))?,
        None => {
            let fields: Vec<ProtocolField> = serde_json::from_str(group.header_fields_json.as_deref().unwrap_or("[]"))
                .map_err(|e| AppError::msg(format!("分组 [{}] 旧公共头无效: {e}", group.name)))?;
            if fields.is_empty() { Vec::new() } else { vec![HeaderDefinition {
                id: LEGACY_HEADER_ID.into(), name: format!("{} 公共头", group.name), fields,
            }] }
        },
    };
    validate_definitions(&headers)?;
    Ok(headers)
}

pub fn selected_header_ids(group: Option<&ProtocolGroup>, refs: Option<&[String]>) -> AppResult<Vec<String>> {
    let ids = match refs {
        Some(refs) => refs.to_vec(),
        None if match group {
            Some(g) if g.header_enabled => group_definitions(g)?.iter().any(|h| h.id == LEGACY_HEADER_ID),
            _ => false,
        } => vec![LEGACY_HEADER_ID.into()],
        None => Vec::new(),
    };
    let mut seen = HashSet::new();
    for id in &ids {
        if !seen.insert(id) { return Err(AppError::msg(format!("公共头引用 [{id}] 重复"))); }
    }
    Ok(ids)
}

pub fn resolve_headers(group: Option<&ProtocolGroup>, refs: Option<&[String]>) -> AppResult<Vec<ProtocolField>> {
    let ids = selected_header_ids(group, refs)?;
    let headers = group.map(group_definitions).transpose()?.unwrap_or_default();
    let mut fields = Vec::new();
    for id in ids {
        let header = headers.iter().find(|h| h.id == id).ok_or_else(|| AppError::msg(format!(
            "公共头引用 [{id}] 不在所选分组 [{}] 中", group.map(|g| g.name.as_str()).unwrap_or("未选择分组")
        )))?;
        fields.extend(resolve_definition_fields(header));
    }
    Ok(fields)
}

/// Preserve field names and wire order while giving legacy duplicate fields distinct override keys.
pub fn resolve_definition_fields(header: &HeaderDefinition) -> Vec<ProtocolField> {
    let mut identities = header.fields.clone();
    crate::fields::dedupe_field_names(&mut identities);
    header.fields.iter().zip(identities).map(|(source, identity)| {
        let mut field = source.clone();
        field.header_key = Some(format!("{}::{}", header.id, identity.name));
        field.section.get_or_insert(FieldSection::Header);
        field.section_name.get_or_insert_with(|| header.name.clone());
        field
    }).collect()
}
