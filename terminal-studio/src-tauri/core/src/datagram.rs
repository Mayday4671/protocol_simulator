//! 报文详情的纯解析视图，不参与发送编码和协议匹配。

use std::collections::HashMap;

use crate::fields::{
    display_field_name, fixed_tail_length, is_variable_len, raw_field_value, when_satisfied,
    DEVICE_SN_KEY,
};
use crate::models::{FieldDetail, FieldEnc, FieldRole, FieldSection, ProtocolField};
use crate::render::{compact_hex, DictMap};

#[derive(Debug, Default)]
pub struct ParsedDetails {
    pub fields: Vec<FieldDetail>,
    pub header_len: usize,
    pub defined_len: usize,
}

/// 按发送布局的顺序解析。公共头字段数仅提供未标记字段的来源默认值；
/// 显式 section、分区名称和记录索引只影响显示，不参与长度/条件计算。
pub fn parse_field_details(
    specs: &[ProtocolField],
    header_field_count: usize,
    bytes: &[u8],
    dicts: &DictMap,
) -> ParsedDetails {
    let mut parsed = ParsedDetails::default();
    let mut offset = 0usize;
    let mut values = HashMap::new();
    for (index, field) in specs.iter().enumerate() {
        if !when_satisfied(specs, field, &values, dicts) {
            continue;
        }
        let section = field.section.unwrap_or(if index < header_field_count {
            FieldSection::Header
        } else {
            FieldSection::Payload
        });
        let variable = is_variable_len(field);
        let unit_total: usize = field
            .unit_fields
            .iter()
            .map(|unit| unit.length as usize)
            .sum();
        // 与编码器使用同一数量来源：当前分支中最近的 device_count。
        // 已声明的记录边界不能因帧尾截断而向前移动。
        let declared_need = if !variable {
            Some(field.length as usize)
        } else if unit_total > 0 {
            specs[..index]
                .iter()
                .rev()
                .find(|source| {
                    source.role == FieldRole::DeviceCount && values.contains_key(&source.name)
                })
                .and_then(|source| {
                    let value: &str = values[&source.name].as_str();
                    if source.enc == FieldEnc::Uint && source.dict_key().is_none() {
                        value.trim().parse::<usize>().ok()
                    } else {
                        usize::from_str_radix(&compact_hex(value), 16).ok()
                    }
                })
                .map(|count| count.saturating_mul(unit_total))
        } else {
            None
        };
        let remaining = bytes.len() - offset;
        // 仅无界变长字段使用尾部布局界定长度；确定长度优先按声明读取。
        let available = if declared_need.is_none() {
            remaining.saturating_sub(fixed_tail_length(specs, index, &values, dicts))
        } else {
            remaining
        };
        let need = declared_need.unwrap_or(available);
        let end = offset + need.min(available);
        let slice = &bytes[offset..end];
        if slice.len() == need {
            // Uint 必须保存十进制实际值，才能正确用于条目数和后续条件。
            values.insert(field.name.clone(), raw_field_value(field, slice));
        }

        if variable && unit_total > 0 && !slice.is_empty() {
            let record_name = display_field_name(&field.name);
            for (record, chunk) in slice.chunks(unit_total).enumerate() {
                let mut unit_offset = 0usize;
                for unit in &field.unit_fields {
                    let start = unit_offset.min(chunk.len());
                    let end = unit_offset
                        .saturating_add(unit.length as usize)
                        .min(chunk.len());
                    let mut detail = field_detail(
                        unit,
                        offset + record * unit_total + start,
                        &chunk[start..end],
                        unit.length as usize,
                        unit.section.unwrap_or(section),
                        unit.section_name.as_ref().or(field.section_name.as_ref()),
                        dicts,
                    );
                    detail.name = format!("{record_name}#{}.{}", record + 1, unit.name);
                    detail.field_name = unit.name.clone();
                    detail.record_name = Some(record_name.clone());
                    detail.record_index = Some(record + 1);
                    parsed.fields.push(detail);
                    unit_offset = unit_offset.saturating_add(unit.length as usize);
                }
            }
            if slice.len() < need {
                let mut detail = field_detail(
                    field,
                    end,
                    &[],
                    need - slice.len(),
                    section,
                    field.section_name.as_ref(),
                    dicts,
                );
                detail.meaning = format!(
                    "数组截断：需 {need}B，实际 {}B，缺 {}B",
                    slice.len(),
                    need - slice.len()
                );
                parsed.fields.push(detail);
            }
        } else {
            let mut detail = field_detail(
                field,
                offset,
                slice,
                need,
                section,
                field.section_name.as_ref(),
                dicts,
            );
            if variable && unit_total > 0 && need == 0 {
                detail.meaning = "0 条记录".into();
            }
            parsed.fields.push(detail);
        }
        offset = end;
    }
    parsed.defined_len = offset;
    parsed.header_len = parsed
        .fields
        .iter()
        .filter(|field| field.is_header)
        .map(|field| field.len)
        .sum();
    if offset < bytes.len() {
        parsed.fields.push(FieldDetail {
            name: "未解析字节".into(),
            field_name: "未解析字节".into(),
            section: "unparsed".into(),
            section_name: None,
            record_name: None,
            record_index: None,
            meaning: "超出协议定义的剩余字节".into(),
            start: offset,
            len: bytes.len() - offset,
            expected_len: bytes.len() - offset,
            hex: format!("0x{}", hex::encode_upper(&bytes[offset..])),
            dec: None,
            dict_label: None,
            is_header: false,
        });
    }
    parsed
}

fn field_detail(
    field: &ProtocolField,
    start: usize,
    bytes: &[u8],
    expected_len: usize,
    section: FieldSection,
    section_name: Option<&String>,
    dicts: &DictMap,
) -> FieldDetail {
    let compact = hex::encode_upper(bytes);
    let complete = bytes.len() == expected_len;
    let dict_label = if complete && !bytes.is_empty() {
        field
            .dict_key()
            .and_then(|key| dicts.get(key))
            .and_then(|set| {
                set.label_of(&compact).map(str::to_string).or_else(|| {
                    (set.key == DEVICE_SN_KEY
                        && bytes.iter().all(|byte| (0x20..0x7f).contains(byte)))
                    .then(|| String::from_utf8_lossy(bytes).to_string())
                })
            })
    } else {
        None
    };
    let mut meaning = dict_label.clone().unwrap_or_else(|| match field.enc {
        FieldEnc::Hex => "十六进制定长值".into(),
        FieldEnc::Uint => "无符号整数 (大端)".into(),
        FieldEnc::Ascii => "文本 (右补 0x00)".into(),
    });
    if !complete {
        meaning.push_str(&format!(
            "；缺 {}B（需 {expected_len}B，实际 {}B）",
            expected_len - bytes.len(),
            bytes.len()
        ));
    }
    FieldDetail {
        name: display_field_name(&field.name),
        field_name: display_field_name(&field.name),
        section: section.as_str().into(),
        section_name: section_name.cloned(),
        record_name: None,
        record_index: None,
        meaning,
        start,
        len: bytes.len(),
        expected_len,
        hex: if bytes.is_empty() {
            "—".into()
        } else {
            format!("0x{compact}")
        },
        dec: (complete && !bytes.is_empty() && bytes.len() <= 8).then(|| {
            bytes
                .iter()
                .fold(0u64, |value, byte| (value << 8) | *byte as u64)
                .to_string()
        }),
        dict_label,
        is_header: section.is_header(),
    }
}
