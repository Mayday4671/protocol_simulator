//! 协议字段级定义: 字段表 → 字节流渲染, 字节流 → 逐字段解析展示
//!
//! 字段取值有三条来源, 优先级从高到低:
//!   1. 发送参数（控制台按字段名传入, 非空即生效）
//!   2. 字段定义值 `ProtocolField::value`
//!   3. 绑定字典的默认条目（字段绑定了字典且上面两者都为空时）
//! 自动计算整值标记由渲染器负责，不允许实际参数覆盖；变长数组的空参数表示零条。
//!
//! 字段一旦绑定字典（[`ProtocolField::dict`]）, 取值被限定为该字典的条目:
//! 值可写条目的十六进制值, 也可写条目含义; 长度由字典的 byte_len 统一保证。

use crate::error::{AppError, AppResult};
use crate::models::{FieldEnc, FieldRole, ProtocolField};
use crate::render::{compact_hex, DictMap, RenderCtx};
use crate::template::{render_hex_template, render_text_template};

/// 字段表 → 字节流（发送渲染）
///
/// 字段表可为空: 表示「纯公共头报文」（消息体 0 字节, 整帧只由系统公共头构成）。
pub fn render_fields(fields: &[ProtocolField], ctx: &RenderCtx) -> AppResult<Vec<u8>> {
    if fields.is_empty() {
        return Ok(Vec::new());
    }
    let computed = fields.iter().map(|f| {
        computed_value(&f.value).map_err(|e| AppError::msg(format!("字段[{}] {e}", f.name)))
    }).collect::<AppResult<Vec<_>>>()?;
    // 自动计算值在布局确定后才产生，不能反过来控制分支或数组条数。
    for (index, f) in fields.iter().enumerate() {
        if let Some(condition) = &f.when {
            if fields.iter().enumerate().any(|(i, source)| source.name == condition.field.trim() && computed[i].is_some()) {
                return Err(AppError::msg(format!("字段[{}] 的条件不能引用自动计算字段[{}]，会形成布局循环依赖", f.name, condition.field)));
            }
        }
        if computed[index].is_some() && (f.role == FieldRole::DeviceCount || !f.unit_fields.is_empty()) {
            return Err(AppError::msg(format!("字段[{}] 自动计算值不能作为条目数或数组，避免布局循环依赖", f.name)));
        }
    }
    let mut out = Vec::new();
    let mut spans = vec![None; fields.len()];
    // 分支条件求值用: 条件字段的本次取值(参数优先, 其次定义值; 归一为紧凑hex可比形式)
    let mut vals: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for (index, f) in fields.iter().enumerate() {
        if !when_satisfied(fields, f, &vals, ctx.dicts) {
            continue; // 分支未选中: 本字段不发送
        }
        if f.length == 0 || f.length > 65535 {
            return Err(AppError::msg(format!("字段[{}] 长度 {} 非法 (需 1-65535)", f.name, f.length)));
        }
        let has_units = is_variable_len(f) && !f.unit_fields.is_empty();
        let count = if has_units {
            let total: usize = f.unit_fields.iter().map(|u| u.length as usize).sum();
            if total != f.length as usize || f.unit_fields.iter().any(is_variable_len) {
                return Err(AppError::msg(format!("字段[{}] 子字段必须为定长，且总长与单条长度一致", f.name)));
            }
            let count = preceding_count(fields, index, &vals)
                .map_err(|e| AppError::msg(format!("字段[{}] {e}", f.name)))?;
            if f.role == FieldRole::DeviceList && count.is_none() {
                return Err(AppError::msg(format!("字段[{}] 缺少当前分支中位于数组前的条目数字段", f.name)));
            }
            count
        } else { None };
        let default_count = count.filter(|_| f.value.trim() == "{vlen}" && !ctx.params.contains_key(&f.name));
        let bytes = if let Some(kind) = &computed[index] {
            if f.enc == FieldEnc::Ascii || f.dict_key().is_some() || f.length > 16 {
                return Err(AppError::msg(format!("字段[{}] 自动计算仅支持未绑定字典的 1-16 字节 Hex/Uint 字段", f.name)));
            }
            if matches!(kind, ComputedValue::Crc16CcittFalse) && f.length != 2 {
                return Err(AppError::msg(format!("字段[{}] CRC16 必须为 2 字节", f.name)));
            }
            if ctx.params.get(&f.name).is_some_and(|v| !v.trim().is_empty() && v.trim() != f.value.trim()) {
                return Err(AppError::msg(format!("字段[{}] 由系统自动计算，不允许参数覆盖", f.name)));
            }
            vec![0; f.length as usize]
        } else if let Some(count) = default_count {
            if count > 200 { return Err(AppError::msg("默认列表最多生成 200 条，请检查条目数")); }
            if count == 0 {
                Vec::new()
            } else {
                let unit_params = std::collections::HashMap::new();
                let unit_ctx = RenderCtx::new(ctx.node,ctx.seq,&unit_params,ctx.dicts).with_tlen(ctx.tlen).with_device_key(ctx.device_key.clone());
                let unit = render_fields(&f.unit_fields, &unit_ctx)?;
                if unit.len() != f.length as usize { return Err(AppError::msg(format!("字段[{}] 子字段总长与单条长度不一致",f.name))); }
                unit.repeat(count)
            }
        } else {
            render_field(f, ctx).map_err(|e| AppError::msg(format!("字段[{}] {}", f.name, e)))?
        };
        if let Some(count) = count {
            let expected = count.checked_mul(f.length as usize)
                .ok_or_else(|| AppError::msg(format!("字段[{}] 条目数超出可支持长度", f.name)))?;
            if bytes.len() != expected {
                return Err(AppError::msg(format!("字段[{}] 需 {count} 条、{expected} 字节，实际 {} 字节", f.name, bytes.len())));
            }
        }
        let actual = raw_field_value(f, &bytes);
        vals.insert(f.name.clone(), actual);
        spans[index] = Some((out.len(), out.len() + bytes.len()));
        out.extend_from_slice(&bytes);
    }
    // 所有长度先写入，CRC 再覆盖前面的完整实际字节（包括已填好的长度和 tlen）。
    for (index, kind) in computed.iter().enumerate() {
        let (Some(ComputedValue::ByteLength { start, end }), Some((at, to))) = (kind, spans[index]) else { continue };
        let resolve = |name: &str| -> AppResult<(usize, usize)> {
            let mut matches = fields.iter().enumerate()
                .filter(|(_, f)| f.name == name)
                .filter_map(|(i, _)| spans[i].map(|span| (i, span.0)));
            let offset = matches.next().ok_or_else(|| AppError::msg(format!("字段[{}] byte_length 范围字段[{name}] 不存在或未选中", fields[index].name)))?;
            if matches.next().is_some() {
                return Err(AppError::msg(format!("字段[{}] byte_length 范围字段[{name}] 重名，请使用唯一字段名", fields[index].name)));
            }
            Ok(offset)
        };
        let (start_index, start_at) = resolve(start)?;
        let (end_index, end_at) = resolve(end)?;
        if end_index < start_index {
            return Err(AppError::msg(format!("字段[{}] byte_length 结束字段不能位于起始字段之前", fields[index].name)));
        }
        let length = end_at - start_at;
        let width = to - at;
        if width < 16 && (length as u128) >= (1u128 << (width * 8)) {
            return Err(AppError::msg(format!("字段[{}] 计算长度 {length} 超出 {width} 字节范围", fields[index].name)));
        }
        out[at..to].copy_from_slice(&(length as u128).to_be_bytes()[16 - width..]);
    }
    for (index, kind) in computed.iter().enumerate() {
        let (Some(ComputedValue::Crc16CcittFalse), Some((at, to))) = (kind, spans[index]) else { continue };
        if to != out.len() || spans[index + 1..].iter().any(Option::is_some) {
            return Err(AppError::msg(format!("字段[{}] CRC16 自动计算字段必须位于实际报文末尾", fields[index].name)));
        }
        let crc = crc16_ccitt_false(&out[..at]);
        out[at..to].copy_from_slice(&crc.to_be_bytes());
    }
    Ok(out)
}

/// 字段模式专用整值标记，依赖实际渲染后的字段边界，不能混排普通模板。
/// byte_length 的起始字段包含、结束字段不包含；CRC 覆盖其前全部字节。
enum ComputedValue<'a> {
    ByteLength { start: &'a str, end: &'a str },
    Crc16CcittFalse,
}

fn computed_value(value: &str) -> Result<Option<ComputedValue<'_>>, String> {
    let value = value.trim();
    if value == "{crc16_ccitt_false}" { return Ok(Some(ComputedValue::Crc16CcittFalse)); }
    if let Some(range) = value.strip_prefix("{byte_length:").and_then(|v| v.strip_suffix('}')) {
        let names: Vec<_> = range.split(':').map(str::trim).collect();
        if names.len() == 2 && names.iter().all(|v| !v.is_empty() && !v.contains(['{', '}'])) {
            return Ok(Some(ComputedValue::ByteLength { start: names[0], end: names[1] }));
        }
    }
    if value.contains("{byte_length") || value.contains("{crc16_ccitt_false") {
        return Err("自动计算值必须完整写为 {byte_length:起始字段名:结束字段名} 或 {crc16_ccitt_false}".into());
    }
    Ok(None)
}

fn crc16_ccitt_false(bytes: &[u8]) -> u16 {
    let mut crc = 0xffffu16;
    for byte in bytes {
        crc ^= (*byte as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 { (crc << 1) ^ 0x1021 } else { crc << 1 };
        }
    }
    crc
}

fn preceding_count(
    fields: &[ProtocolField],
    index: usize,
    values: &std::collections::HashMap<String, String>,
) -> Result<Option<usize>, String> {
    fields[..index].iter().rev()
        .find(|f| f.role == FieldRole::DeviceCount && values.contains_key(&f.name))
        .map(|f| {
            let value = values[&f.name].trim();
            if f.enc == FieldEnc::Uint && f.dict_key().is_none() { value.parse::<usize>() }
            else { usize::from_str_radix(&compact_hex(value), 16) }
        }).transpose().map_err(|_| "条目数无效或超出可支持范围".into())
}

/// 值形如 `{req:字段名}` 时返回其中的字段名（自动应答显式回填请求值的标记）
///
/// 字段名可为中文; 该标记由引擎在发送前替换为请求报文中同名解析值, 不走模板渲染。
pub fn req_ref_name(value: &str) -> Option<&str> {
    let v = value.trim();
    v.strip_prefix("{req:")
        .and_then(|s| s.strip_suffix('}'))
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// 分组公共头与协议字段的**显式**覆盖合并
///
/// 协议字段带 `hdr_ref = Some("头字段名")` 时, 该字段作为「本协议对此公共头字段的取值」:
/// 值非空即覆盖公共头字段的值, 字典绑定跟随协议字段, 并从消息体剔除;
/// 长度/编码仍以公共头定义为准（字节布局不变）。
/// section / section_name 仅在覆盖字段显式设置时替换，缺省保留公共头显示元数据。
///
/// **未标记 hdr_ref 的字段即使与公共头同名也独立留在消息体**（各自长度/取值, 互不干扰）。
/// hdr_ref 指向的头字段不存在时返回错误（显式配置指向了不存在的目标）。
///
/// 返回 (覆盖后的公共头字段表, 纯消息体字段表)。
pub fn merge_header_specs(
    header: &[ProtocolField],
    body: &[ProtocolField],
) -> AppResult<(Vec<ProtocolField>, Vec<ProtocolField>)> {
    let mut head: Vec<ProtocolField> = header.to_vec();
    let mut overridden = std::collections::HashSet::new();
    let mut body_only: Vec<ProtocolField> = Vec::with_capacity(body.len());
    for f in body {
        let target = f.hdr_ref.as_deref().map(str::trim).filter(|s| !s.is_empty());
        match target {
            Some(hname) => {
                let qualified = head.iter().position(|h| h.header_key.as_deref() == Some(hname));
                let index = if let Some(index) = qualified {
                    index
                } else {
                    let matches: Vec<_> = head.iter().enumerate().filter(|(_, h)| h.name == hname).map(|(i, _)| i).collect();
                    match matches.as_slice() {
                        [index] => *index,
                        [] => return Err(AppError::msg(format!("字段 [{}] 的公共头覆盖指向 [{}], 但所选公共头中没有该字段", f.name, hname))),
                        _ if matches.iter().all(|i| head[*i].header_key.as_deref().is_none_or(|key| key.starts_with("legacy-default::"))) => matches[0],
                        _ => return Err(AppError::msg(format!("字段 [{}] 的公共头覆盖 [{}] 存在歧义，请使用 公共头标识::字段名", f.name, hname))),
                    }
                };
                if !overridden.insert(index) {
                    return Err(AppError::msg(format!("公共头字段 [{}] 被多个协议字段重复覆盖", hname)));
                }
                let h = &mut head[index];
                if !f.value.trim().is_empty() {
                    h.value = f.value.clone();
                }
                if f.dict_key().is_some() {
                    h.dict = f.dict.clone();
                }
                if f.role != FieldRole::None {
                    h.role = f.role;
                }
                if f.profile_key.is_some() {
                    h.profile_key = f.profile_key.clone();
                }
                if f.section.is_some() {
                    h.section = f.section;
                }
                if f.section_name.is_some() {
                    h.section_name = f.section_name.clone();
                }
            }
            None => body_only.push(f.clone()),
        }
    }
    Ok((head, body_only))
}

/// 重名字段的全帧唯一键（fqn）: 协议内允许中文字段名重复,
/// 从第二个同名者起 name 追加 `#2`/`#3`…, 使参数/回填/提取的按名索引互不踩踏。
/// 展示层用 [`display_field_name`] 把后缀转回可读形式。
pub fn dedupe_field_names(fields: &mut [ProtocolField]) {
    let reserved: std::collections::HashSet<String> = fields.iter().map(|f| f.name.clone()).collect();
    let mut used = std::collections::HashSet::new();
    for f in fields.iter_mut() {
        if used.insert(f.name.clone()) { continue; }
        let mut n = 2;
        loop {
            let candidate = format!("{}#{}", f.name, n);
            if !reserved.contains(&candidate) && used.insert(candidate.clone()) { f.name = candidate; break; }
            n += 1;
        }
    }
}

/// 展示用字段名: `序号#2` → `序号(2)`; 无后缀原样返回
pub fn display_field_name(name: &str) -> String {
    match name.rfind('#') {
        Some(pos) if pos + 1 < name.len() && name[pos + 1..].chars().all(|c| c.is_ascii_digit()) => {
            format!("{}({})", &name[..pos], &name[pos + 1..])
        }
        _ => name.to_string(),
    }
}

fn render_field(f: &ProtocolField, ctx: &RenderCtx) -> Result<Vec<u8>, String> {
    let len = f.length as usize;
    if len == 0 || len > 65535 {
        return Err(format!("长度 {} 非法 (需 1-65535)", f.length));
    }
    // 变长数组的显式空参数表示零条记录；普通字段的空参数仍沿用定义值。
    if is_variable_len(f) && ctx.params.get(&f.name).is_some_and(|v| v.trim().is_empty()) {
        return Ok(Vec::new());
    }
    // 控制台按字段名传入的参数优先于协议定义值（发送前可改参数）
    let value = match ctx.params.get(f.name.as_str()) {
        Some(v) if !v.trim().is_empty() => v.trim().to_string(),
        _ => f.value.trim().to_string(),
    };

    // ---- 绑定字典: 取值必须是该字典的条目, 字节由字典长度保证 ----
    if let Some(key) = f.dict_key() {
        return render_dict_field(key, &value, len, f.role == FieldRole::DeviceSn, ctx);
    }

    // 值留空且未绑字典: 按声明长度补 0（宽度由协议声明推导, 免逐字段手填占位值）
    if value.is_empty() {
        return Ok(vec![0u8; len]);
    }

    match f.enc {
        FieldEnc::Hex => {
            // 支持 {变量} 与 {dict:key} 混排; 无变量时按字面 hex 处理
            let digits = if value.contains('{') {
                render_hex_template(&value, len, ctx).map_err(|e| e.to_string())?
            } else {
                compact_hex(&value)
            };
            let bytes =
                hex::decode(&digits).map_err(|_| format!("值 \"{value}\" 不是合法 hex"))?;
            // 字面值长度必须精确; 变长字段({rlen}/{vlen})允许实际长度与声明不同
            if bytes.len() != len && !is_variable_len(f) {
                return Err(format!("hex 值需 {len} 字节, 实际 {} 字节", bytes.len()));
            }
            Ok(bytes)
        }
        FieldEnc::Uint => {
            let digits = uint_value_digits(&value, len, ctx)?;
            let bytes = hex::decode(&digits)
                .map_err(|_| format!("值 \"{value}\" 渲染结果不是合法 hex: {digits}"))?;
            // 变量/字典结果长度与字段长度不一致时必须报错, 否则整帧字节会错位
            if bytes.len() != len {
                return Err(format!(
                    "值 \"{value}\" 渲染为 {} 字节, 与字段长度 {len} 字节不符",
                    bytes.len()
                ));
            }
            Ok(bytes)
        }
        FieldEnc::Ascii => {
            let text = if value.contains('{') {
                render_text_template(&value, ctx).map_err(|e| e.to_string())?
            } else {
                value
            };
            let mut bytes = text.into_bytes();
            if bytes.len() > len {
                return Err(format!("文本超长: {} 字节 > {len} 字节", bytes.len()));
            }
            bytes.resize(len, 0x00);
            Ok(bytes)
        }
    }
}

/// 绑定字典的字段: 定位条目 → 取其字节
/// 保留字典 Key: 设备档案。绑定它的字段 = 设备序列号,
/// 应答成功后由引擎自动写入条目; 渲染时放行任意合法 hex（首次配置新设备无需预录）
pub const DEVICE_SN_KEY: &str = "device_sn";


/// 分支条件判定: 字段的 when(出现条件)对已解析/已渲染的前序字段值是否成立。
/// 条件字段不存在(在前?残缺?)或值不符 → 本字段跳过(分支不选)。字典条件值可写条目hex或含义。
pub fn when_satisfied(
    fields: &[ProtocolField],
    f: &ProtocolField,
    vals: &std::collections::HashMap<String, String>,
    dicts: &DictMap,
) -> bool {
    let Some(w) = &f.when else { return true };
    let Some(actual) = vals.get(w.field.trim()) else {
        return false;
    };
    let cond_field = fields.iter().find(|x| x.name == w.field.trim());
    crate::rules::cond_value_matches(&w.value, actual, cond_field, dicts)
}

/// 变长字段: 定义值含 `{rlen}`（随机内容）或 `{vlen}`（占位标记）,
/// 或 字段用途=设备列表(变长) 且声明了子字段布局 时——
/// role+unit_fields 本身就是显式的变长声明, 不再强制要求值里写占位符。
/// 实际长度随渲染值/报文而定, 不按声明长度(=单条长度)精确校验; 解析按 条目数×单条 或剩余字节
pub fn is_variable_len(f: &ProtocolField) -> bool {
    f.value.contains("{rlen") || f.value.contains("{vlen}")
        || (f.role == FieldRole::DeviceList && !f.unit_fields.is_empty())
}

/// 变长字段({vlen}/{rlen})在当前报文里的实际字节数。
///
/// 优先按「条目数字段(role=DeviceCount)的实际值 × 单台记录长度」推导——
/// 设备列表之后还有尾随字段(如 CRC 校验/保留)时, 「吃掉剩余全部」会把它们
/// 吞进列表导致尾部字段解析不到; 找不到条目数或无 unit 布局时才退回剩余全部。
/// `values` 为本轮已解析字段的原始值(条目数需位于列表之前)。
/// 返回期望长度，可能超过 remaining；调用方须检查残缺且使用饱和加法计算边界。
pub fn variable_field_need(
    fields: &[ProtocolField],
    idx: usize,
    values: &std::collections::HashMap<String, String>,
    remaining: usize,
) -> usize {
    let f = &fields[idx];
    let unit_total: usize = f.unit_fields.iter().map(|u| u.length as usize).sum();
    if unit_total > 0 {
        if let Ok(Some(n)) = preceding_count(fields, idx, values) {
            // 返回期望长度，不能把残缺报文误当成完整数组提取。
            return n.saturating_mul(unit_total);
        }
    }
    remaining
}

/// 仅当数组之后的布局完全确定时，才能在截断报文中为 CRC 等尾字段保留字节。
pub(crate) fn fixed_tail_length(
    fields: &[ProtocolField],
    idx: usize,
    values: &std::collections::HashMap<String, String>,
    dicts: &DictMap,
) -> usize {
    let mut total = 0usize;
    for f in &fields[idx + 1..] {
        if f.when.as_ref().is_some_and(|w| !values.contains_key(w.field.trim())) {
            return 0; // 条件依赖尚未解析的字段，不能猜测尾部布局。
        }
        if !when_satisfied(fields, f, values, dicts) { continue; }
        if is_variable_len(f) { return 0; }
        total = total.saturating_add(f.length as usize);
    }
    total
}

/// 条目数字段的值 → 台数: 十进制(uint 字段)或十六进制(hex 字段, 如 "03" / "00 03")
pub(crate) fn raw_field_value(f: &ProtocolField, bytes: &[u8]) -> String {
    match f.enc {
        FieldEnc::Uint if f.dict_key().is_none() && bytes.len() <= 16 => bytes.iter().fold(0u128, |n,b| (n << 8) | *b as u128).to_string(),
        FieldEnc::Ascii if f.dict_key().is_none() => String::from_utf8_lossy(bytes).trim_end_matches('\0').to_string(),
        _ => spaced_hex(bytes),
    }
}

fn render_dict_field(
    key: &str,
    value: &str,
    len: usize,
    is_device_sn_field: bool,
    ctx: &RenderCtx,
) -> Result<Vec<u8>, String> {
    let set = ctx.dicts.get(key).ok_or_else(|| {
        format!("绑定的字典 [{key}] 不存在（请在「字典管理」中创建, 或检查协议所属系统）")
    })?;
    if set.byte_len != len {
        return Err(format!(
            "字典 [{}] 每条目 {} 字节, 与字段长度 {len} 字节不符",
            set.name, set.byte_len
        ));
    }
    // 设备序列号字段(role=device_sn): 允许任意合法 hex（新序列号无需预先录入）; 条目含义/值仍可匹配
    if is_device_sn_field && !value.contains('{') {
        let digits = compact_hex(value);
        if let Ok(bytes) = hex::decode(&digits) {
            if bytes.len() == len {
                return Ok(bytes);
            }
            return Err(format!(
                "值 \"{value}\" 需 {len} 字节, 实际 {} 字节",
                bytes.len()
            ));
        }
        // 非 hex 写法（如条目含义 SN000001）→ 落回下方条目匹配
    }
    // 值为空 → 取默认条目; 允许写条目含义或条目十六进制值
    let digits = if value.is_empty() {
        set.default_value()
            .ok_or_else(|| format!("字典 [{}] 还没有任何条目", set.name))?
            .to_string()
    } else {
        set.find_value(value)
            .ok_or_else(|| {
                format!(
                    "值 \"{value}\" 不属于字典 [{}], 可选: {}",
                    set.name,
                    set.options_hint()
                )
            })?
            .to_string()
    };
    let bytes = hex::decode(&digits)
        .map_err(|_| format!("字典 [{}] 条目 \"{digits}\" 不是合法 hex", set.name))?;
    if bytes.len() != len {
        return Err(format!(
            "字典 [{}] 条目 \"{digits}\" 为 {} 字节, 与字段长度 {len} 字节不符",
            set.name,
            bytes.len()
        ));
    }
    Ok(bytes)
}

/// Uint 字段: 值为 {变量} 或 十进制/0x 前缀 → 2*len 位大端 hex 数字串
fn uint_value_digits(value: &str, len: usize, ctx: &RenderCtx) -> Result<String, String> {
    let v = value.trim();
    // 含变量（含 {dict:key}）时交给模板渲染器, 未写宽度的变量按字段长度取
    if v.contains('{') {
        return render_hex_template(v, len, ctx).map_err(|e| format!("\"{v}\" 渲染失败: {e}"));
    }
    if v.is_empty() || v == "0" {
        return Ok("00".repeat(len));
    }
    let num = if let Some(hexs) = v.strip_prefix("0x").or_else(|| v.strip_prefix("0X")) {
        u128::from_str_radix(hexs, 16).map_err(|_| format!("值 \"{v}\" 不是合法十六进制"))?
    } else {
        v.parse::<u128>()
            .map_err(|_| format!("值 \"{v}\" 不是合法十进制整数"))?
    };
    let max = if len >= 16 { u128::MAX } else { (1u128 << (len * 8)) - 1 };
    if num > max {
        return Err(format!("值 {num} 超出 {len} 字节范围"));
    }
    let bytes = num.to_be_bytes(); // 16 字节
    Ok(hex::encode_upper(&bytes[16 - len..]))
}

// ==================== 收包解析 ====================

/// 单字段解析结果
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedField {
    pub name: String,
    pub display: String,
}

/// 字节流 → 逐字段解析（人读展示）; 超出字段表的剩余字节记为 payload, 长度不足的标注
///
/// 绑定字典的字段会附带条目含义, 如 `01 (温度传感器)`; 值不在字典内则标注 `<未定义>`。
pub fn parse_fields(fields: &[ProtocolField], bytes: &[u8], dicts: &DictMap) -> Vec<ParsedField> {
    let mut out = Vec::new();
    let mut off = 0usize;
    // 已解析字段的原始值(紧凑 hex): 供后续变长字段按 条目数×单台 推导实际长度
    let mut raw_values: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for (idx, f) in fields.iter().enumerate() {
        if !when_satisfied(fields, f, &raw_values, dicts) {
            continue; // 分支未选中: 本字段不占字节
        }
        // 变长字段: 按条目数推导; 列表后还可解析尾随字段(crc/保留等)
        let need = if is_variable_len(f) {
            variable_field_need(fields, idx, &raw_values, bytes.len().saturating_sub(off))
        } else {
            f.length as usize
        };
        let remaining = bytes.len().saturating_sub(off);
        let available = if is_variable_len(f) && preceding_count(fields, idx, &raw_values).ok().flatten().is_some() {
            remaining.saturating_sub(fixed_tail_length(fields, idx, &raw_values, dicts))
        } else { remaining };
        let end = off.saturating_add(need.min(available)).min(bytes.len());
        let slice = &bytes[off..end];
        if !slice.is_empty() && slice.len() == need {
            raw_values.insert(f.name.clone(), raw_field_value(f, slice));
        }
        // 带记录布局的变长字段(如设备列表): 按单台拆开, 每台给出子字段中文含义
        let unit_total: usize = f.unit_fields.iter().map(|u| u.length as usize).sum();
        if is_variable_len(f)
            && !f.unit_fields.is_empty()
            && unit_total > 0
            && slice.len() % unit_total == 0
            && slice.len() == need
            && !slice.is_empty()
        {
            let count = slice.len() / unit_total;
            let parts: Vec<String> = slice
                .chunks(unit_total)
                .enumerate()
                .map(|(i, chunk)| {
                    let sub: Vec<String> = {
                        let mut v = Vec::new();
                        let mut o = 0usize;
                        for u in &f.unit_fields {
                            let s2 = &chunk[o..o + u.length as usize];
                            v.push(format!("{}={}", u.name, display_value(u, s2, dicts)));
                            o += u.length as usize;
                        }
                        v
                    };
                    format!("{}: {}", i + 1, sub.join(", "))
                })
                .collect();
            out.push(ParsedField {
                name: format!("{}({}台)", display_field_name(&f.name), count),
                display: parts.join("; "),
            });
            off = end;
            continue;
        }
        let display = if slice.len() < need {
            format!("{} <缺{}B>", spaced_hex(slice), need - slice.len())
        } else {
            display_value(f, slice, dicts)
        };
        out.push(ParsedField {
            name: display_field_name(&f.name),
            display,
        });
        off = end;
    }
    if off < bytes.len() {
        out.push(ParsedField {
            name: "payload".to_string(),
            display: spaced_hex(&bytes[off..]),
        });
    }
    out
}

/// 解析为一行摘要文本: "目标ID=A1 · 注册地址=256(0x0100) · ..."
pub fn parse_fields_summary(fields: &[ProtocolField], bytes: &[u8], dicts: &DictMap) -> String {
    parse_fields(fields, bytes, dicts)
        .into_iter()
        .map(|p| format!("{}={}", p.name, p.display))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// 序列号字节展示: 全可打印 ASCII 则解码为文本（如 SN000009）, 否则空格 hex
fn sn_display(bytes: &[u8]) -> String {
    if !bytes.is_empty() && bytes.iter().all(|b| (0x20..0x7f).contains(b)) {
        String::from_utf8_lossy(bytes).to_string()
    } else {
        spaced_hex(bytes)
    }
}

fn display_value(f: &ProtocolField, slice: &[u8], dicts: &DictMap) -> String {
    // 绑定字典: 优先按条目含义展示, 这是字典最直接的收益
    if let Some(set) = f.dict_key().and_then(|k| dicts.get(k)) {
        let raw = hex::encode_upper(slice);
        // 设备档案: 已入库显示档案含义, 未入库的新序列号显示可读 ASCII
        if set.key == DEVICE_SN_KEY || f.role == FieldRole::DeviceSn {
            return match set.label_of(&raw) {
                Some(label) => label.to_string(),
                None => sn_display(slice),
            };
        }
        return match set.label_of(&raw) {
            Some(label) => format!("{} ({label})", spaced_hex(slice)),
            None => format!("{} <字典[{}]中未定义>", spaced_hex(slice), set.name),
        };
    }
    // 语义=设备序列号的未绑字典字段也给出可读形式（设备列表子字段场景）
    if f.role == FieldRole::DeviceSn && f.enc == FieldEnc::Hex && !slice.is_empty() {
        return sn_display(slice);
    }
    match f.enc {
        FieldEnc::Hex => spaced_hex(slice),
        FieldEnc::Uint => {
            if slice.len() <= 8 {
                let mut v: u64 = 0;
                for b in slice {
                    v = (v << 8) | *b as u64;
                }
                let mut s = format!("{v} (0x{})", hex::encode_upper(slice));
                // 秒级时间戳启发式: 补充可读时间
                if slice.len() == 4 && (1_000_000_000..4_100_000_000).contains(&v) {
                    if let Some(dt) = chrono::DateTime::from_timestamp(v as i64, 0) {
                        s.push_str(&format!(
                            " {}",
                            dt.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S")
                        ));
                    }
                }
                s
            } else {
                spaced_hex(slice)
            }
        }
        FieldEnc::Ascii => {
            let trimmed: &[u8] = {
                let end = slice.iter().rposition(|b| *b != 0).map_or(0, |p| p + 1);
                &slice[..end]
            };
            String::from_utf8_lossy(trimmed).to_string()
        }
    }
}

fn spaced_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

// ==================== 请求字段值提取（自动应答回填） ====================

/// 从收包字节中提取各字段的原始取值, 供自动应答按同名字段回填请求值
///
/// - hex / 绑定字典字段 → 空格分隔大写 hex（渲染端 `compact_hex` 与字典 `find_value` 均接受）
/// - uint 字段 → 十进制字符串（超过 16 字节无法用十进制表达, 退回 hex, 渲染时会得到明确报错）
/// - ascii 字段 → 去掉右侧 0x00 填充后的文本
///
/// 长度不足的字段（残缺收包）及之后全部跳过, 不产生半截值。
pub fn extract_field_values(
    fields: &[ProtocolField],
    bytes: &[u8],
    dicts: &DictMap,
) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    let mut off = 0usize;
    for (idx, f) in fields.iter().enumerate() {
        if !when_satisfied(fields, f, &out, dicts) {
            continue; // 分支未选中: 不占字节不产值
        }
        // 变长字段: 按条目数×单台推导实际长度(尾部字段不被吞), 无条目数时吃剩余
        let need = if is_variable_len(f) {
            variable_field_need(fields, idx, &out, bytes.len().saturating_sub(off))
        } else {
            f.length as usize
        };
        if need == 0 { out.insert(f.name.clone(), String::new()); continue; }
        let tail = if is_variable_len(f) && preceding_count(fields, idx, &out).ok().flatten().is_some() {
            fixed_tail_length(fields, idx, &out, dicts)
        } else { 0 };
        if need > bytes.len().saturating_sub(off).saturating_sub(tail) {
            break;
        }
        let slice = &bytes[off..off + need];
        let value = raw_field_value(f, slice);
        out.insert(f.name.clone(), value);
        off += need;
    }
    out
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Node, NodeRole, Protocol};
    use crate::render::DictSetLite;
    use std::collections::HashMap;

    fn node() -> Node {
        Node {
            id: 1,
            name: "终端A".into(),
            role: NodeRole::Simulator,
            local_ip: "127.0.0.1".into(),
            local_port: 5000,
            remote_ip: "127.0.0.1".into(),
            remote_port: 6000,
            node_id_hex: Some("0A 01 02 03".into()),
            session_hex: None,
            captures_json: None,
            group_bind_json: None,
            enabled: true,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    fn f(name: &str, len: u32, enc: FieldEnc, value: &str) -> ProtocolField {
        ProtocolField {
            name: name.into(),
            length: len,
            enc,
            value: value.into(),
            dict: None,
            unit_fields: Vec::new(),
            role: Default::default(),
            hdr_ref: None,
            header_key: None, profile_key: None, section: None, section_name: None,
                when: None,
        }
    }

    #[test]
    fn dynamic_array_rejects_count_mismatch_and_supports_empty_override() {
        let n = node();
        let dicts = DictMap::new();
        let fields = vec![
            fu("count", 1, "01", FieldRole::DeviceCount, &[]),
            fu("items", 2, "AABBCCDDEEFF00112233", FieldRole::DeviceList, &[("item", 2)]),
            f("crc", 2, FieldEnc::Hex, "1234"),
        ];
        let error = render_fields(&fields, &RenderCtx::bare(&n, 0)).unwrap_err().to_string();
        assert!(error.contains("items") && error.contains("1 条"), "{error}");
        let params = HashMap::from([("count".into(), "00".into()), ("items".into(), String::new())]);
        assert_eq!(render_fields(&fields, &RenderCtx::new(&n, 0, &params, &dicts)).unwrap(), [0, 0x12, 0x34]);
    }

    #[test]
    fn dynamic_array_defaults_follow_nearest_active_count() {
        let fields: Vec<ProtocolField> = serde_json::from_value(serde_json::json!([
            {"name":"mode","len":1,"enc":"hex","value":"01"},
            {"name":"active_count","len":1,"enc":"hex","value":"02","role":"device_count"},
            {"name":"hidden_count","len":1,"enc":"hex","value":"05","role":"device_count","when":{"field":"mode","value":"02"}},
            {"name":"items","len":2,"enc":"hex","value":"{vlen}","role":"device_list","unit_fields":[
                {"name":"kind","len":1,"enc":"hex","value":"AB"},
                {"name":"value","len":1,"enc":"uint","value":"7"}
            ]},
            {"name":"tail","len":2,"enc":"hex","value":"1234"}
        ])).unwrap();
        assert_eq!(render_fields(&fields, &RenderCtx::bare(&node(), 0)).unwrap(), [1, 2, 0xAB, 7, 0xAB, 7, 0x12, 0x34]);
        let mut zero = fields;
        zero[1].value = "00".into();
        zero[3].unit_fields[0].value = "{node_id}".into();
        let mut n = node();
        n.node_id_hex = None;
        assert_eq!(render_fields(&zero, &RenderCtx::bare(&n, 0)).unwrap(), [1, 0, 0x12, 0x34]);
    }

    #[test]
    fn computed_length_and_crc_follow_actual_array_bytes() {
        let n = node();
        let dicts = DictMap::new();
        let fields = vec![
            f("total", 2, FieldEnc::Uint, "{tlen}"),
            f("body_len", 2, FieldEnc::Uint, "{byte_length:count:crc}"),
            fu("count", 1, "01", FieldRole::DeviceCount, &[]),
            fu("items", 2, "{vlen}", FieldRole::DeviceList, &[("value", 2)]),
            f("crc", 2, FieldEnc::Hex, "{crc16_ccitt_false}"),
        ];
        for count in [0usize, 1, 5] {
            let params = HashMap::from([("count".into(), format!("{count:02X}")), ("items".into(), "ABCD".repeat(count))]);
            let ctx = RenderCtx::new(&n, 0, &params, &dicts);
            let first = render_fields(&fields, &ctx).unwrap();
            let bytes = render_fields(&fields, &ctx.with_tlen(first.len() as u64)).unwrap();
            assert_eq!(bytes.len(), 7 + 2 * count);
            assert_eq!(&bytes[..4], &[0, bytes.len() as u8, 0, (1 + 2 * count) as u8]);
            let mut expected = 0xffffu16;
            for byte in &bytes[..bytes.len() - 2] {
                expected ^= (*byte as u16) << 8;
                for _ in 0..8 { expected = if expected & 0x8000 != 0 { (expected << 1) ^ 0x1021 } else { expected << 1 }; }
            }
            assert_eq!(&bytes[bytes.len() - 2..], &expected.to_be_bytes());
            let parsed = extract_field_values(&fields, &bytes, &dicts);
            assert_eq!(compact_hex(&parsed["items"]), "ABCD".repeat(count));
            assert_eq!(compact_hex(&parsed["crc"]), format!("{expected:04X}"));
        }
    }

    #[test]
    fn computed_crc_matches_ccitt_false_check_vector() {
        let n = node();
        let fields = vec![f("data", 9, FieldEnc::Ascii, "123456789"), f("crc", 2, FieldEnc::Hex, "{crc16_ccitt_false}")];
        let bytes = render_fields(&fields, &RenderCtx::bare(&n, 0)).unwrap();
        assert_eq!(&bytes[9..], &[0x29, 0xB1]);
        let params = HashMap::from([("crc".into(), "{crc16_ccitt_false}".into())]);
        assert_eq!(render_fields(&fields, &RenderCtx::with_params(&n, 0, &params)).unwrap(), bytes);
    }

    #[test]
    fn computed_fields_reject_unsafe_definitions_and_overrides() {
        let n = node();
        let dicts = DictMap::new();
        for token in ["{byte_length:missing:crc}", "{byte_length:crc:data}", "{byte_length:data}"] {
            let fields = vec![f("len", 1, FieldEnc::Hex, token), f("data", 1, FieldEnc::Hex, "01"), f("crc", 2, FieldEnc::Hex, "{crc16_ccitt_false}")];
            assert!(render_fields(&fields, &RenderCtx::bare(&n, 0)).is_err(), "{token}");
        }
        let crc = f("crc", 2, FieldEnc::Hex, "{crc16_ccitt_false}");
        assert!(render_fields(&[crc.clone(), f("tail", 1, FieldEnc::Hex, "01")], &RenderCtx::bare(&n, 0)).is_err());
        let params = HashMap::from([("crc".into(), "0000".into())]);
        let error = render_fields(&[crc], &RenderCtx::new(&n, 0, &params, &dicts)).unwrap_err().to_string();
        assert!(error.contains("覆盖"), "{error}");
        let fields: Vec<ProtocolField> = serde_json::from_value(serde_json::json!([
            {"name":"len","len":1,"enc":"hex","value":"{byte_length:data:crc}"},
            {"name":"data","len":1,"enc":"hex","value":"01","when":{"field":"len","value":"01"}},
            {"name":"crc","len":2,"enc":"hex","value":"{crc16_ccitt_false}"}
        ])).unwrap();
        let error = render_fields(&fields, &RenderCtx::bare(&n, 0)).unwrap_err().to_string();
        assert!(error.contains("条件") || error.contains("循环"), "{error}");

        let overflow = vec![f("len", 1, FieldEnc::Hex, "{byte_length:data:crc}"), f("data", 256, FieldEnc::Hex, ""), f("crc", 2, FieldEnc::Hex, "{crc16_ccitt_false}")];
        assert!(render_fields(&overflow, &RenderCtx::bare(&n, 0)).unwrap_err().to_string().contains("超出"));
        let reversed_empty = vec![f("len", 1, FieldEnc::Hex, "{byte_length:crc:items}"), fu("count", 1, "00", FieldRole::DeviceCount, &[]), fu("items", 1, "{vlen}", FieldRole::DeviceList, &[("value", 1)]), f("crc", 2, FieldEnc::Hex, "{crc16_ccitt_false}")];
        assert!(render_fields(&reversed_empty, &RenderCtx::bare(&n, 0)).unwrap_err().to_string().contains("之前"));
    }

    #[test]
    fn truncated_dynamic_array_is_not_extracted_as_complete() {
        let fields = vec![fu("count", 1, "05", FieldRole::DeviceCount, &[]), fu("items", 2, "{vlen}", FieldRole::DeviceList, &[("value", 2)]), f("crc", 2, FieldEnc::Hex, "ABCD")];
        for count in [2, 5] {
            let bytes = [count, 0, 1, 0xAB, 0xCD];
            let extracted = extract_field_values(&fields, &bytes, &DictMap::new());
            assert!(!extracted.contains_key("items"));
            let parsed = parse_fields(&fields, &bytes, &DictMap::new());
            assert!(parsed[1].display.contains(&format!("缺{}B", (count - 1) * 2)), "{parsed:?}");
            assert_eq!(parsed[2].display, "AB CD");
        }
    }

    /// 带语义与记录布局(变长列表 unit 子字段)的字段
    fn fu(
        name: &str,
        len: u32,
        value: &str,
        role: FieldRole,
        units: &[(&str, u32)],
    ) -> ProtocolField {
        ProtocolField {
            name: name.into(),
            length: len,
            enc: FieldEnc::Hex,
            value: value.into(),
            dict: None,
            unit_fields: units
                .iter()
                .map(|(n, l)| ProtocolField {
                    name: (*n).into(),
                    length: *l,
                    enc: FieldEnc::Hex,
                    value: String::new(),
                    dict: None,
                    unit_fields: Vec::new(),
                    role: Default::default(),
                    hdr_ref: None,
            header_key: None, profile_key: None, section: None, section_name: None,
                when: None,
                })
                .collect(),
            role,
            hdr_ref: None,
            header_key: None, profile_key: None, section: None, section_name: None,
                when: None,
        }
    }

    #[test]
    fn variable_list_with_trailing_fields_parses_tail() {
        // 用户协议形态: 条目数 + 设备信息列表(变长, 单台 13B=编号3+序号4+IP4+端口2)
        //               + crc 校验 2B + 保留 2B —— 列表后的尾随字段必须可解析
        let d = DictMap::new();
        let fields = vec![
            f("配置类型", 1, FieldEnc::Hex, "11"),
            f("子类型", 1, FieldEnc::Hex, "01"),
            fu("条目数", 1, "02", FieldRole::DeviceCount, &[]),
            fu(
                "设备信息",
                13,
                "{vlen}",
                FieldRole::DeviceList,
                &[("设备编号", 3), ("设备序列号", 4), ("设备IP", 4), ("设备端口", 2)],
            ),
            f("crc校验", 2, FieldEnc::Hex, "AB CD"),
            f("保留", 2, FieldEnc::Hex, ""),
        ];
        // 2 台 × 13B 列表 + 尾部 4B
        let mut bytes = vec![0x11u8, 0x01, 0x02];
        for i in 0..2u8 {
            bytes.extend_from_slice(&[0x00, 0x00, i + 1]); // 设备编号 3B
            bytes.extend_from_slice(&[0x53, 0x4E, 0x30, 0x30 + i]); // 序列号 4B
            bytes.extend_from_slice(&[0xC0, 0xA8, 0x00, i + 10]); // IP 4B
            bytes.extend_from_slice(&[0x1F, 0x40]); // 端口 2B
        }
        bytes.extend_from_slice(&[0xAB, 0xCD, 0x00, 0x00]);

        // 提取: 列表恰好 26B, 尾部 crc/保留 各归其位
        let m = extract_field_values(&fields, &bytes, &d);
        assert_eq!(m["设备信息"].split_whitespace().count(), 26, "列表不应吞掉尾部字段");
        assert_eq!(m["crc校验"], "AB CD");
        assert_eq!(m["保留"], "00 00");

        // 摘要: 列表按台展开(2台), 尾部字段照常出现
        let parsed = parse_fields(&fields, &bytes, &d);
        let summary = parse_fields_summary(&fields, &bytes, &d);
        assert!(summary.contains("设备信息(2台)"), "按条目数展开: {summary}");
        assert!(summary.contains("crc校验=AB CD"), "尾部字段在摘要中: {summary}");
        assert!(parsed.iter().any(|p| p.name == "保留"));

        // 残缺容错: 条目数=5 但剩余只够 2 台 + 尾部 → 列表取剩余(不超过)
        let mut short = vec![0x11u8, 0x01, 0x05];
        short.extend_from_slice(&bytes[3..]);
        let m2 = extract_field_values(&fields, &short, &d);
        assert!(!m2.contains_key("设备信息"), "残缺列表不应被提取为完整值");
    }

    /// 分支(when): 条件字段值决定后续字段是否存在 — 解析/提取/渲染三口径一致
    #[test]
    fn when_conditional_fields_branch() {
        let n = node();
        let d = DictMap::new();
        // 布局: 类型(1B) + [分支A: 2B] when 类型=01 / [分支B: 4B] when 类型=02 + 尾部(1B)
        let mk = |name: &str, len: u32, value: &str, when: Option<(String, String)>| ProtocolField {
            name: name.into(),
            length: len,
            enc: FieldEnc::Hex,
            value: value.into(),
            dict: None,
            unit_fields: Vec::new(),
            role: Default::default(),
            hdr_ref: None,
            header_key: None, profile_key: None, section: None, section_name: None,
            when: when.map(|(f, v)| crate::models::MatchCond { field: f, value: v }),
        };
        let fields = vec![
            mk("类型", 1, "01", None),
            mk("分支A", 2, "AA BB", Some(("类型".into(), "01".into()))),
            mk("分支B", 4, "11 22 33 44", Some(("类型".into(), "02".into()))),
            mk("尾部", 1, "FF", None),
        ];
        // 类型=01: 分支A存在(总 4B), 分支B跳过
        let m = extract_field_values(&fields, &[0x01, 0xAA, 0xBB, 0xFF], &d);
        assert_eq!(m.get("分支A").map(String::as_str), Some("AA BB"));
        assert!(m.get("分支B").is_none(), "分支B应跳过");
        assert_eq!(m.get("尾部").map(String::as_str), Some("FF"));
        // 类型=02: 分支B存在(总 6B)
        let m2 = extract_field_values(&fields, &[0x02, 0x11, 0x22, 0x33, 0x44, 0xFF], &d);
        assert!(m2.get("分支A").is_none());
        assert_eq!(m2.get("分支B").map(String::as_str), Some("11 22 33 44"));
        // 渲染同样按分支: 定义值类型=01 → 发出的帧含分支A不含分支B
        let bytes = render_fields(&fields, &RenderCtx::bare(&n, 0)).unwrap();
        assert_eq!(bytes, vec![0x01, 0xAA, 0xBB, 0xFF]);
        // 参数切分支: 类型=02 → 6B 帧
        let mut params = std::collections::HashMap::new();
        params.insert("类型".to_string(), "02".to_string());
        let ctx = RenderCtx::new(&n, 0, &params, &d);
        let bytes2 = render_fields(&fields, &ctx).unwrap();
        assert_eq!(bytes2, vec![0x02, 0x11, 0x22, 0x33, 0x44, 0xFF]);
        // 摘要: 只出现命中的分支字段
        let sum = parse_fields_summary(&fields, &bytes2, &d);
        assert!(sum.contains("分支B") && !sum.contains("分支A"), "{sum}");
    }

    /// 通用形态: 列表长度推导只依赖「条目数(role) × unit 布局」两项协议声明,
    /// 与尾随字段的个数/类型/顺序无关, 与具体协议无关
    #[test]
    fn zero_and_hex_sixteen_counts_preserve_tail() {
        let d=DictMap::new();
        for enc in [FieldEnc::Hex, FieldEnc::Uint] {
            let mut count=f("count",1,enc,"0");count.role=FieldRole::DeviceCount;
            let fields=vec![count,fu("items",1,"{vlen}",FieldRole::DeviceList,&[("v",1)]), f("tail",1,FieldEnc::Hex,"FF")];
            for n in [0usize,10,16] {
                let mut bytes=vec![n as u8];bytes.extend(vec![0xAB;n]);bytes.push(0xFF);
                let values=extract_field_values(&fields,&bytes,&d);
                assert_eq!(values.get("tail").map(String::as_str),Some("FF"));
                assert_eq!(compact_hex(values.get("items").unwrap()).len(),n*2);
                assert!(parse_fields_summary(&fields,&bytes,&d).contains("tail=FF"));
            }
        }
    }

    #[test]
    fn hidden_parent_cannot_activate_nested_branch() {
        let fields:Vec<ProtocolField>=serde_json::from_value(serde_json::json!([
            {"name":"mode","len":1,"enc":"uint","value":"2"},
            {"name":"parent","len":1,"enc":"hex","value":"AA","when":{"field":"mode","value":"1"}},
            {"name":"child","len":1,"enc":"hex","value":"BB","when":{"field":"parent","value":"AA"}}
        ])).unwrap();
        assert_eq!(render_fields(&fields,&RenderCtx::bare(&node(),0)).unwrap(),vec![2]);
    }

    #[test]
    fn variable_list_generic_shapes() {
        let d = dicts();

        // ① 列表后跟任意个任意类型字段: uint 时间戳 + 字典字段 + 第二个变长列表
        //    (两组列表各自带条目数, 且条目数与列表不相邻——中间隔普通字段)
        let fields = vec![
            fu("参数条目数", 2, "02", FieldRole::DeviceCount, &[]),
            f("版本", 1, FieldEnc::Hex, "03"), // 条目数与列表之间隔字段
            fu(
                "参数列表",
                4,
                "{vlen}",
                FieldRole::DeviceList,
                &[("参数类型", 1), ("参数值", 3)],
            ),
            f("时间戳", 4, FieldEnc::Uint, "0"),
            fd("设备类型", 1, FieldEnc::Hex, "01", "device_type"), // 字典尾随字段
            fu("设备条目数", 1, "01", FieldRole::DeviceCount, &[]),
            fu(
                "设备列表",
                5,
                "{vlen}",
                FieldRole::DeviceList,
                &[("编号", 3), ("状态", 1), ("在线", 1)],
            ),
            f("末尾保留", 1, FieldEnc::Hex, ""),
        ];
        // 帧: 0002 | 03 | (01+3B)(02+3B) | 4B时间戳 | 01 | 01 | (3B+01+01) | FF
        let bytes = [
            0x00, 0x02, 0x03, 0x01, 0x00, 0x00, 0x0A, 0x02, 0x00, 0x00, 0x0B, 0x6A, 0x97, 0xE1,
            0x23, 0x01, 0x01, 0x00, 0x00, 0x07, 0x01, 0x01, 0xFF,
        ];
        let m = extract_field_values(&fields, &bytes, &d);
        assert_eq!(m["参数列表"].split_whitespace().count(), 8, "2×4B 参数");
        assert_eq!(m["时间戳"], "1788338467", "uint 尾随字段正常提取");
        assert_eq!(m["设备类型"], "01");
        assert_eq!(m["设备列表"].split_whitespace().count(), 5, "1×5B 设备");
        assert_eq!(m["末尾保留"], "FF");
        // 摘要里两组列表按各自条目数展开
        let s = parse_fields_summary(&fields, &bytes, &d);
        assert!(s.contains("参数列表(2") && s.contains("设备列表(1"), "{s}");

        // ② 未配条目数(无 role)的变长列表: 回退「吃剩余」——老协议行为不变
        let legacy = vec![
            f("命令", 1, FieldEnc::Hex, "01"),
            fu(
                "数据",
                4,
                "{vlen}",
                FieldRole::DeviceList,
                &[("a", 2), ("b", 2)],
            ),
        ];
        let lb = [0x01, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
        let m2 = extract_field_values(&legacy, &lb, &d);
        assert_eq!(m2["数据"].split_whitespace().count(), 6, "无条目数吃剩余");

        // ③ 条目数在列表之后 → 推导不到(只向前找), 同样回退吃剩余, 不误判
        let reversed = vec![
            fu(
                "列表",
                2,
                "{vlen}",
                FieldRole::DeviceList,
                &[("a", 1), ("b", 1)],
            ),
            fu("条目数", 1, "03", FieldRole::DeviceCount, &[]),
        ];
        let m3 = extract_field_values(&reversed, &[0x11, 0x22, 0x33, 0x03], &d);
        assert_eq!(m3["列表"].split_whitespace().count(), 4, "条目数在后推导不到 → 吃剩余全部");
    }

    /// 绑定字典的字段
    fn fd(name: &str, len: u32, enc: FieldEnc, value: &str, dict: &str) -> ProtocolField {
        ProtocolField {
            name: name.into(),
            length: len,
            enc,
            value: value.into(),
            dict: Some(dict.into()),
            unit_fields: Vec::new(),
            role: Default::default(),
            hdr_ref: None,
            header_key: None, profile_key: None, section: None, section_name: None,
                when: None,
        }
    }

    /// device_type(1B) + reg_addr(2B) 两张测试字典
    fn dicts() -> DictMap {
        let mut m = DictMap::new();
        m.insert(DictSetLite {
            key: "device_type".into(),
            name: "设备类型".into(),
            byte_len: 1,
            items: vec![
                ("01".into(), "温度传感器".into()),
                ("02".into(), "湿度传感器".into()),
                ("03".into(), "烟感".into()),
            ],
        });
        m.insert(DictSetLite {
            key: "reg_addr".into(),
            name: "注册地址".into(),
            byte_len: 2,
            items: vec![("0100".into(), "主站".into()), ("0200".into(), "备站".into())],
        });
        m.insert(DictSetLite {
            key: DEVICE_SN_KEY.into(),
            name: "设备档案".into(),
            byte_len: 8,
            items: vec![("534E303030303031".into(), "SN000001".into())],
        });
        m
    }

    #[test]
    fn device_sn_accepts_new_sn_and_existing_label() {
        let n = node();
        let d = dicts();
        let params = HashMap::new();
        let ctx = || RenderCtx::new(&n, 0, &params, &d);
        let sn_field = |value: &str| {
            let mut f = fd("设备序列号", 8, FieldEnc::Hex, value, DEVICE_SN_KEY);
            f.role = FieldRole::DeviceSn; // 语义标记: 设备序列号
            f
        };
        // 新序列号: 不在字典内, 合法 8B hex 直接放行（首次配置新设备）
        let fields = vec![sn_field("53 4E 30 30 30 30 30 39")];
        assert_eq!(render_fields(&fields, &ctx()).unwrap(), b"SN000009");
        // 已有条目: 按含义匹配
        let fields = vec![sn_field("SN000001")];
        assert_eq!(render_fields(&fields, &ctx()).unwrap(), b"SN000001");
        // 未标记 role 的字段维持字典强约束
        let fields = vec![fd("设备序列号", 8, FieldEnc::Hex, "53 4E 30 30 30 30 30 39", DEVICE_SN_KEY)];
        let e = render_fields(&fields, &ctx()).unwrap_err().to_string();
        assert!(e.contains("不属于字典"), "{e}");
        // 长度不符仍然报错
        let fields = vec![sn_field("01 02")];
        let e = render_fields(&fields, &ctx()).unwrap_err().to_string();
        assert!(e.contains("字节"), "{e}");
    }

    #[test]
    fn vlen_device_list_renders_and_extracts_variable_length() {
        let n = node();
        let d = dicts();
        // 设备配置请求: 操作类型1B + 数量2B + 设备列表(变长, 声明长度=单台记录12B)
        let fields = vec![
            f("操作类型", 1, FieldEnc::Hex, "01"),
            f("设备数量", 2, FieldEnc::Hex, "00 02"),
            f("设备列表", 12, FieldEnc::Hex, "{vlen}"),
        ];
        // 2 台 × 12B = 24B 列表, params 整体覆盖
        let mut params = HashMap::new();
        params.insert(
            "设备列表".to_string(),
            "53 4E 30 30 30 30 30 31 00 01 00 0A 53 4E 30 30 30 30 30 32 00 01 00 0A".to_string(),
        );
        let ctx = RenderCtx::new(&n, 0, &params, &d);
        let bytes = render_fields(&fields, &ctx).unwrap();
        assert_eq!(bytes.len(), 1 + 2 + 24, "变长列表按实际值渲染: {bytes:?}");
        assert_eq!(&bytes[..3], &[0x01, 0x00, 0x02]);

        // 定义值原样渲染({vlen}=空): 列表为空
        let empty = HashMap::new();
        let ctx0 = RenderCtx::new(&n, 0, &empty, &d);
        let b0 = render_fields(&fields, &ctx0).unwrap();
        assert_eq!(b0.len(), 3, "{{vlen}} 渲染为空");

        // 提取/解析: 变长字段吃掉剩余全部字节
        let m = extract_field_values(&fields, &bytes, &d);
        assert_eq!(m["设备列表"].split_whitespace().count(), 24);
        let parsed = parse_fields(&fields, &bytes, &d);
        assert_eq!(parsed[2].display.split_whitespace().count(), 24);
    }

    #[test]
    fn unit_fields_parse_shows_per_device_meaning() {
        let d = dicts();
        // 设备列表带记录布局: 序列号(device_sn) + 设备类型(device_type, 1B 测试字典) + 心跳周期
        let mut list = fd("设备列表", 11, FieldEnc::Hex, "{vlen}", "unused");
        list.dict = None;
        let mut sn_unit = fd("序列号", 8, FieldEnc::Hex, "", DEVICE_SN_KEY);
        sn_unit.role = FieldRole::DeviceSn;
        list.unit_fields = vec![
            sn_unit,
            fd("设备类型", 1, FieldEnc::Hex, "", "device_type"),
            f("心跳周期", 2, FieldEnc::Hex, ""),
        ];
        let fields = vec![
            f("设备数量", 2, FieldEnc::Hex, "00 02"),
            list,
        ];
        // 两台: SN000001(档案已有) + SN000009(新序列号), 类型 01=温度传感器
        let data: Vec<u8> = [
            0x00, 0x02,
            b'S', b'N', b'0', b'0', b'0', b'0', b'0', b'1', 0x01, 0x00, 0x0A,
            b'S', b'N', b'0', b'0', b'0', b'0', b'0', b'9', 0x01, 0x00, 0x0A,
        ]
        .to_vec();
        let parsed = parse_fields(&fields, &data, &d);
        assert_eq!(parsed[1].name, "设备列表(2台)");
        let disp = &parsed[1].display;
        // 档案设备显示含义, 新序列号显示可读 ASCII, 类型显示字典含义
        assert!(disp.contains("SN000001"), "{disp}");
        assert!(disp.contains("SN000009"), "{disp}");
        assert!(disp.contains("温度传感器"), "{disp}");
        assert!(disp.contains("1: 序列号"), "{disp}");
    }

    #[test]
    fn render_mixed_fields() {
        let n = node();
        let fields = vec![
            f("帧头", 2, FieldEnc::Hex, "FF A1"),
            f("命令", 1, FieldEnc::Hex, "01"),
            f("终端编号", 4, FieldEnc::Uint, "{node_id}"),
            f("注册地址", 2, FieldEnc::Uint, "0x0100"),
            f("版本", 1, FieldEnc::Uint, "2"),
            f("预留", 3, FieldEnc::Ascii, "AB"),
        ];
        let bytes = render_fields(&fields, &RenderCtx::bare(&n, 7)).unwrap();
        assert_eq!(
            bytes,
            vec![
                0xFF, 0xA1, 0x01, // 帧头+命令
                0x0A, 0x01, 0x02, 0x03, // {node_id}
                0x01, 0x00, // 0x0100
                0x02, // 版本
                b'A', b'B', 0x00, // ascii 右补 0
            ]
        );
    }

    #[test]
    fn render_timestamp_and_seq_vars() {
        let n = node();
        let fields = vec![
            f("时间戳", 4, FieldEnc::Uint, "{timestamp}"),
            f("序号", 4, FieldEnc::Uint, "{seq}"),
        ];
        let bytes = render_fields(&fields, &RenderCtx::bare(&n, 1)).unwrap();
        let ts = u32::from_be_bytes(bytes[0..4].try_into().unwrap()) as i64;
        assert!((chrono::Utc::now().timestamp() - ts).abs() <= 2);
        assert_eq!(&bytes[4..], &[0, 0, 0, 1]);
    }

    #[test]
    fn hex_field_renders_variables() {
        // 旧实现里 hex 字段直接 hex::decode, {node_id} 必然报错
        let n = node();
        let fields = vec![
            f("帧头", 2, FieldEnc::Hex, "FF A1"),
            f("终端编号", 4, FieldEnc::Hex, "{node_id}"),
        ];
        let bytes = render_fields(&fields, &RenderCtx::bare(&n, 0)).unwrap();
        assert_eq!(bytes, vec![0xFF, 0xA1, 0x0A, 0x01, 0x02, 0x03]);
    }

    #[test]
    fn hex_field_variable_aligns_to_declared_width() {
        let n = node();
        // node_id 是 4 字节, 塞进 2 字节字段按大端对齐取低位（宽度由协议声明推导, 不卡渲染）
        let fields = vec![f("x", 2, FieldEnc::Hex, "{node_id}")];
        let bytes = render_fields(&fields, &RenderCtx::bare(&n, 0)).unwrap();
        assert_eq!(bytes, vec![0x02, 0x03]);
    }

    #[test]
    fn empty_value_renders_zeros_of_declared_len() {
        // 值留空且未绑字典: 按声明长度补 0, 不必逐字段手填占位值
        let n = node();
        let fields = vec![
            f("保留", 3, FieldEnc::Hex, ""),
            f("计数", 1, FieldEnc::Hex, "01"),
        ];
        let bytes = render_fields(&fields, &RenderCtx::bare(&n, 0)).unwrap();
        assert_eq!(bytes, vec![0x00, 0x00, 0x00, 0x01]);
        // uint 字段留空同样补 0
        let fields = vec![f("时长", 4, FieldEnc::Uint, "")];
        let bytes = render_fields(&fields, &RenderCtx::bare(&n, 0)).unwrap();
        assert_eq!(bytes, vec![0x00; 4]);
        // 控制台传空参数 = 沿用定义值语义: 定义值也空 → 依然补 0
        let mut params = HashMap::new();
        params.insert("保留".to_string(), "  ".to_string());
        let fields = vec![f("保留", 2, FieldEnc::Hex, "")];
        let bytes = render_fields(&fields, &RenderCtx::new(&n, 0, &params, &DictMap::new())).unwrap();
        assert_eq!(bytes, vec![0x00, 0x00]);
    }

    #[test]
    fn ascii_field_renders_variables() {
        // 旧实现把整个 "{node_id}" 当变量名查 params, 必然报「未知模板变量」
        let n = node();
        let fields = vec![f("标识", 16, FieldEnc::Ascii, "SN-{node_id}")];
        let bytes = render_fields(&fields, &RenderCtx::bare(&n, 0)).unwrap();
        let text = String::from_utf8_lossy(&bytes[..14]).to_string();
        assert_eq!(text, "SN-0A 01 02 03");
        assert_eq!(&bytes[14..], &[0x00, 0x00], "剩余字节右补 0");
    }

    #[test]
    fn dict_bound_field_uses_item_bytes() {
        let n = node();
        let d = dicts();
        let params = HashMap::new();
        let ctx = RenderCtx::new(&n, 0, &params, &d);
        let fields = vec![
            f("帧头", 2, FieldEnc::Hex, "FF A1"),
            // 定义值写条目十六进制值
            fd("设备类型", 1, FieldEnc::Hex, "02", "device_type"),
            // 定义值写条目含义也接受
            fd("注册地址", 2, FieldEnc::Uint, "备站", "reg_addr"),
        ];
        let bytes = render_fields(&fields, &ctx).unwrap();
        assert_eq!(bytes, vec![0xFF, 0xA1, 0x02, 0x02, 0x00]);
    }

    #[test]
    fn dict_bound_field_falls_back_to_default_item() {
        let n = node();
        let d = dicts();
        let params = HashMap::new();
        let fields = vec![fd("设备类型", 1, FieldEnc::Hex, "", "device_type")];
        let bytes = render_fields(&fields, &RenderCtx::new(&n, 0, &params, &d)).unwrap();
        assert_eq!(bytes, vec![0x01]);
    }

    #[test]
    fn dict_bound_field_rejects_value_outside_set() {
        let n = node();
        let d = dicts();
        let params = HashMap::new();
        let fields = vec![fd("设备类型", 1, FieldEnc::Hex, "FF", "device_type")];
        let e = render_fields(&fields, &RenderCtx::new(&n, 0, &params, &d))
            .unwrap_err()
            .to_string();
        assert!(e.contains("不属于字典"), "{e}");
        assert!(e.contains("温度传感器"), "报错应列出可选条目: {e}");
    }

    #[test]
    fn dict_bound_field_rejects_length_mismatch() {
        let n = node();
        let d = dicts();
        let params = HashMap::new();
        // reg_addr 是 2 字节字典, 绑到 1 字节字段上
        let fields = vec![fd("x", 1, FieldEnc::Hex, "", "reg_addr")];
        let e = render_fields(&fields, &RenderCtx::new(&n, 0, &params, &d))
            .unwrap_err()
            .to_string();
        assert!(e.contains("不符"), "{e}");
    }

    #[test]
    fn dict_bound_field_rejects_missing_set() {
        let n = node();
        let params = HashMap::new();
        let d = DictMap::new();
        let fields = vec![fd("x", 1, FieldEnc::Hex, "01", "gone")];
        let e = render_fields(&fields, &RenderCtx::new(&n, 0, &params, &d))
            .unwrap_err()
            .to_string();
        assert!(e.contains("不存在"), "{e}");
    }

    #[test]
    fn console_param_overrides_dict_bound_value() {
        let n = node();
        let d = dicts();
        let mut params = HashMap::new();
        params.insert("设备类型".to_string(), "烟感".to_string());
        let fields = vec![fd("设备类型", 1, FieldEnc::Hex, "01", "device_type")];
        let bytes = render_fields(&fields, &RenderCtx::new(&n, 0, &params, &d)).unwrap();
        assert_eq!(bytes, vec![0x03]);
    }

    #[test]
    fn hex_length_mismatch_is_error() {
        let n = node();
        let fields = vec![f("帧头", 3, FieldEnc::Hex, "FF A1")];
        assert!(render_fields(&fields, &RenderCtx::bare(&n, 0)).is_err());
    }

    #[test]
    fn uint_overflow_is_error() {
        let n = node();
        let fields = vec![f("x", 1, FieldEnc::Uint, "256")];
        assert!(render_fields(&fields, &RenderCtx::bare(&n, 0)).is_err());
    }

    #[test]
    fn parse_round_trip() {
        let n = node();
        let d = DictMap::new();
        let fields = vec![
            f("帧头", 2, FieldEnc::Hex, "FF A1"),
            f("终端编号", 4, FieldEnc::Uint, "{node_id}"),
            f("版本", 1, FieldEnc::Uint, "3"),
        ];
        let bytes = render_fields(&fields, &RenderCtx::bare(&n, 0)).unwrap();
        let parsed = parse_fields(&fields, &bytes, &d);
        assert_eq!(parsed[0].display, "FF A1");
        assert_eq!(parsed[1].display, "167838211 (0x0A010203)");
        assert_eq!(parsed[2].display, "3 (0x03)");
        let summary = parse_fields_summary(&fields, &bytes, &d);
        assert!(summary.contains("终端编号=167838211"));
        assert!(summary.contains("帧头=FF A1"));
    }

    #[test]
    fn parse_shows_dict_label() {
        let n = node();
        let d = dicts();
        let params = HashMap::new();
        let fields = vec![
            fd("设备类型", 1, FieldEnc::Hex, "02", "device_type"),
            fd("注册地址", 2, FieldEnc::Uint, "主站", "reg_addr"),
        ];
        let bytes = render_fields(&fields, &RenderCtx::new(&n, 0, &params, &d)).unwrap();
        let summary = parse_fields_summary(&fields, &bytes, &d);
        assert!(summary.contains("设备类型=02 (湿度传感器)"), "{summary}");
        assert!(summary.contains("注册地址=01 00 (主站)"), "{summary}");
        // 字典外的值要标出来, 而不是当成普通字节糊过去
        let parsed = parse_fields(&fields, &[0xFF, 0x00, 0x00], &d);
        assert!(parsed[0].display.contains("未定义"), "{:?}", parsed[0]);
    }

    #[test]
    fn parse_truncated_and_payload_remainder() {
        let d = DictMap::new();
        let fields = vec![
            f("a", 2, FieldEnc::Hex, "0000"),
            f("b", 4, FieldEnc::Uint, "0"),
        ];
        // 只有 3 字节: a 完整, b 不足
        let parsed = parse_fields(&fields, &[0x00, 0x00, 0x05], &d);
        assert_eq!(parsed[1].display, "05 <缺3B>");
        // 超长: 剩余记 payload
        let parsed = parse_fields(
            &fields,
            &[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xDE, 0xAD],
            &d,
        );
        assert_eq!(parsed[2].name, "payload");
        assert_eq!(parsed[2].display, "DE AD");
    }

    #[test]
    fn timestamp_field_displays_datetime() {
        let n = node();
        let d = DictMap::new();
        let fields = vec![f("时间戳", 4, FieldEnc::Uint, "{timestamp}")];
        let bytes = render_fields(&fields, &RenderCtx::bare(&n, 0)).unwrap();
        let parsed = parse_fields(&fields, &bytes, &d);
        assert!(
            parsed[0].display.contains("20"),
            "应包含可读时间: {}",
            parsed[0].display
        );
    }

    #[test]
    fn ascii_field_parses_text() {
        let n = node();
        let d = DictMap::new();
        let fields = vec![f("标识", 4, FieldEnc::Ascii, "SN01")];
        let bytes = render_fields(&fields, &RenderCtx::bare(&n, 0)).unwrap();
        assert_eq!(parse_fields(&fields, &bytes, &d)[0].display, "SN01");
    }

    #[test]
    fn extract_values_by_enc_for_auto_echo() {
        let d = DictMap::new();
        let fields = vec![
            f("帧头", 2, FieldEnc::Hex, "FF A1"),
            f("终端编号", 4, FieldEnc::Uint, "0"),
            f("标识", 8, FieldEnc::Ascii, ""),
            fd("设备类型", 1, FieldEnc::Hex, "01", "device_type"),
        ];
        let mut data = vec![0xFF, 0xA1, 0x0A, 0x01, 0x02, 0x03];
        data.extend_from_slice(b"SN-1\0\0\0\0");
        data.push(0x02);
        let m = extract_field_values(&fields, &data, &d);
        assert_eq!(m["帧头"], "FF A1");
        assert_eq!(m["终端编号"], "167838211");
        assert_eq!(m["标识"], "SN-1");
        assert_eq!(m["设备类型"], "02");
    }

    #[test]
    fn extract_skips_truncated_tail() {
        let d = DictMap::new();
        let fields = vec![
            f("a", 2, FieldEnc::Hex, ""),
            f("b", 2, FieldEnc::Hex, ""),
        ];
        let m = extract_field_values(&fields, &[0x01, 0x02, 0x03], &d);
        assert_eq!(m.len(), 1, "残缺字段不应产生半截值: {m:?}");
        assert_eq!(m["a"], "01 02");
    }

    #[test]
    fn extracted_request_value_renders_response_fields() {
        // 自动应答全链路（单元级）: 请求字段值提取 → 作为 params 渲染应答协议同名字段
        let n = node();
        let d = dicts();
        let req_fields = vec![
            fd("设备类型", 1, FieldEnc::Hex, "", "device_type"),
            f("终端编号", 4, FieldEnc::Uint, "0"),
        ];
        let req_values = extract_field_values(&req_fields, &[0x02, 0x0A, 0x01, 0x02, 0x03], &d);
        // 应答协议: 同名字段定义值留空 → 由 params 回填; 不同名字段用定义值
        let resp_fields = vec![
            f("命令字", 1, FieldEnc::Hex, "81"),
            fd("设备类型", 1, FieldEnc::Hex, "", "device_type"),
            f("终端编号", 4, FieldEnc::Uint, ""),
        ];
        let bytes = render_fields(&resp_fields, &RenderCtx::new(&n, 0, &req_values, &d)).unwrap();
        assert_eq!(bytes, vec![0x81, 0x02, 0x0A, 0x01, 0x02, 0x03]);
    }

    #[test]
    fn empty_fields_render_empty_body() {
        // 纯公共头报文: 字段表为空 → 消息体 0 字节, 由系统公共头构成整帧
        let n = node();
        assert_eq!(render_fields(&[], &RenderCtx::bare(&n, 0)).unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn req_ref_name_parses_mark() {
        assert_eq!(req_ref_name("{req:源设备ID}"), Some("源设备ID"));
        assert_eq!(req_ref_name("  {req:序号}  "), Some("序号"));
        assert_eq!(req_ref_name("{req:}"), None, "空字段名不算");
        assert_eq!(req_ref_name("{node_id}"), None);
        assert_eq!(req_ref_name("01"), None);
        assert_eq!(req_ref_name("X{req:a}Y"), None, "仅整值匹配, 不支持混排");
    }

    #[test]
    fn merge_header_requires_explicit_hdr_ref() {
        // 头在分组级; 协议字段通过 hdr_ref 显式覆盖头字段, 同名但不标记的独立存在
        let header = vec![
            f("帧头", 2, FieldEnc::Hex, "55 AA"),
            f("消息类型", 1, FieldEnc::Hex, "01"), // 头默认值, 协议可覆盖
            f("版本号", 1, FieldEnc::Hex, "01"),
            f("序号", 4, FieldEnc::Hex, "{seq}"),
        ];
        let mut over = fd("消息类型", 1, FieldEnc::Hex, "02", "device_type");
        over.hdr_ref = Some("消息类型".into()); // 显式覆盖头里的「消息类型」
        let body = vec![
            over,
            f("目标设备ID", 2, FieldEnc::Hex, "10 02"),
            f("温度", 2, FieldEnc::Hex, "01 0C"),
        ];
        let (head, body_only) = merge_header_specs(&header, &body).unwrap();
        // 覆盖生效: 消息类型=02 且字典跟随协议字段
        assert_eq!(head[1].value, "02");
        assert_eq!(head[1].dict_key(), Some("device_type"));
        // 头里无字典的字段不受影响
        assert_eq!(head[2].dict_key(), None);
        // 仅被覆盖的字段从消息体剔除
        assert_eq!(body_only.len(), 2);
        assert_eq!(body_only[0].name, "目标设备ID");
        assert_eq!(body_only[1].name, "温度");
        // 渲染: 头(覆盖后) + 消息体
        let n = node();
        let d = dicts();
        let empty = HashMap::new();
        let ctx = RenderCtx::new(&n, 1, &empty, &d);
        let mut bytes = render_fields(&head, &ctx).unwrap();
        bytes.extend(render_fields(&body_only, &ctx).unwrap());
        assert_eq!(bytes, vec![0x55, 0xAA, 0x02, 0x01, 0x00, 0x00, 0x00, 0x01, 0x10, 0x02, 0x01, 0x0C]);
        // 解析对齐: 合并字段表逐字段还原
        let parsed = parse_fields(&[head.as_slice(), body_only.as_slice()].concat(), &bytes, &d);
        assert_eq!(parsed[0].display, "55 AA");
        assert!(parsed[1].display.contains("湿度传感器"), "{}", parsed[1].display);
        assert_eq!(parsed[3].display, "00 00 00 01");
    }

    #[test]
    fn same_name_without_hdr_ref_stays_independent() {
        let d = DictMap::new();
        // 同名字段但不标记 hdr_ref: 独立留在消息体, 长度/取值互不影响（用户可重名录入）
        let header = vec![f("序号", 4, FieldEnc::Hex, "{seq}")];
        let body = vec![
            f("序号", 2, FieldEnc::Hex, "00 0A"), // 与头同名但未标记 → 独立消息体字段
            f("温度", 2, FieldEnc::Hex, "01 0C"),
        ];
        let (head, body_only) = merge_header_specs(&header, &body).unwrap();
        assert_eq!(head[0].value, "{seq}", "头字段不被同名字段隐式覆盖");
        assert_eq!(body_only.len(), 2, "同名字段全部留在消息体");
        assert_eq!(body_only[0].length, 2, "长度保持自己的定义");
        // hdr_ref 指向不存在的头字段 → 报错
        let mut bad = f("X", 1, FieldEnc::Hex, "01");
        bad.hdr_ref = Some("不存在的头字段".into());
        let e = merge_header_specs(&header, &[bad]).unwrap_err().to_string();
        assert!(e.contains("公共头中没有该字段"), "{e}");
    }

    #[test]
    fn dedupe_names_and_display() {
        let d = DictMap::new();
        // 重名字段: 全帧唯一键从第二个同名者起加 #k; 展示层转回 序号(2)
        let mut fields = vec![
            f("序号", 1, FieldEnc::Hex, "01"),
            f("温度", 1, FieldEnc::Hex, "02"),
            f("序号", 1, FieldEnc::Hex, "03"),
            f("序号", 1, FieldEnc::Hex, "04"),
        ];
        dedupe_field_names(&mut fields);
        assert_eq!(fields[0].name, "序号");
        assert_eq!(fields[1].name, "温度");
        assert_eq!(fields[2].name, "序号#2");
        assert_eq!(fields[3].name, "序号#3");
        // 提取/参数索引: 同名字段互不踩踏
        let m = extract_field_values(&fields, &[0x01, 0x02, 0x03, 0x04], &d);
        assert_eq!(m["序号"], "01");
        assert_eq!(m["序号#2"], "03");
        assert_eq!(m["序号#3"], "04");
        // 展示名
        assert_eq!(display_field_name("序号"), "序号");
        assert_eq!(display_field_name("序号#2"), "序号(2)");
        assert_eq!(display_field_name("设备列表#2"), "设备列表(2)");
        // 含 # 但非数字后缀的不动
        assert_eq!(display_field_name("CRC#A"), "CRC#A");
    }

    #[test]
    fn tlen_variable_renders_total_length() {
        // 两遍渲染（单元级）: 第一遍 tlen=0 占位得总长, 第二遍真实总长; 宽度一致
        let n = node();
        let fields = vec![
            f("帧头", 2, FieldEnc::Hex, "55 AA"),
            f("长度", 2, FieldEnc::Uint, "{tlen}"),
            f("序号", 4, FieldEnc::Uint, "{seq}"),
        ];
        let d = DictMap::new();
        let params = HashMap::new();
        let pass1 = render_fields(&fields, &RenderCtx::new(&n, 7, &params, &d)).unwrap();
        assert_eq!(pass1.len(), 8);
        let pass2 = render_fields(
            &fields,
            &RenderCtx::new(&n, 7, &params, &d).with_tlen(pass1.len() as u64),
        )
        .unwrap();
        assert_eq!(pass2.len(), 8, "两遍长度必须一致");
        assert_eq!(&pass2[2..4], &[0x00, 0x08], "长度字段应为总长 8");
        assert_eq!(&pass2[0..2], &[0x55, 0xAA]);
    }

    #[test]
    fn protocol_field_specs_decodes_json() {
        let p = Protocol {
            header_refs_json: None,
            id: 1,
            protocol_id: "x".into(),
            name: "x".into(),
            format: Default::default(),
            msg_type: Default::default(),
            def_mode: crate::models::DefMode::Fields,
            fields_json: Some(
                r#"[{"name":"a","len":2,"enc":"hex","value":"FF00"},
                    {"name":"b","len":1,"enc":"hex","value":"01","dict":"cmd"}]"#
                    .into(),
            ),
            system_ref: None,
            group_ref: None,
            group_name: None,
            system_name: None,
            payload_template: String::new(),
            match_rule: None,
            match_fields_json: None,
            auto_respond: None,
            auto_periodic: None,
            auto_periodic_interval_ms: 0,
            respond_delay_ms: 0,
            description: None,
            created_at: String::new(),
            updated_at: String::new(),
        };
        let specs = p.field_specs();
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].length, 2);
        // 老数据没有 dict 键 → 反序列化为 None, 保持兼容
        assert_eq!(specs[0].dict, None);
        assert_eq!(specs[1].dict_key(), Some("cmd"));
        // 模板模式不解析
        let mut p2 = p.clone();
        p2.def_mode = crate::models::DefMode::Template;
        assert!(p2.field_specs().is_empty());
    }
}
