use super::expression::Expr;
use super::*;
use serde_json::Value;
use serde_json::{Map, json};
use std::collections::BTreeMap;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecodedField {
    pub path: String,
    pub start_offset0: usize,
    pub byte_start1: usize,
    pub byte_end1: usize,
    pub byte_length: usize,
    pub bit_start: Option<u8>,
    pub bit_width: Option<u8>,
    pub raw_hex: String,
    pub decoded_value: Value,
    pub decimal: Option<String>,
    pub binary: String,
    pub label_zh: String,
    pub description_zh: String,
    pub unit: Option<String>,
    pub meaning_zh: Option<String>,
    pub secret: bool,
    pub diagnostics: Vec<String>,
    pub children: Vec<DecodedField>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DecodedPacket {
    pub protocol_id: String,
    pub message_id: String,
    pub message_code: u8,
    pub raw_hex: String,
    pub values: Value,
    pub fields: Vec<DecodedField>,
    pub diagnostics: Vec<String>,
}
impl DecodedPacket {
    pub fn field(&self, path: &str) -> Option<&DecodedField> {
        fn find<'a>(nodes: &'a [DecodedField], path: &str) -> Option<&'a DecodedField> {
            for n in nodes {
                if n.path == path {
                    return Some(n);
                }
                if let Some(x) = find(&n.children, path) {
                    return Some(x);
                }
            }
            None
        }
        find(&self.fields, path)
    }
    pub fn secret_spans(&self) -> Vec<(usize, usize)> {
        fn walk(nodes: &[DecodedField], out: &mut Vec<(usize, usize)>) {
            for n in nodes {
                if n.secret {
                    out.push((n.start_offset0, n.byte_length));
                } else {
                    walk(&n.children, out)
                }
            }
        }
        let mut spans = Vec::new();
        walk(&self.fields, &mut spans);
        spans
    }
    pub fn redacted_bytes(&self, bytes: &[u8]) -> Vec<u8> {
        let mut bytes = bytes.to_vec();
        for (start, len) in self.secret_spans() {
            let end = start.saturating_add(len).min(bytes.len());
            if start < end {
                bytes[start..end].fill(0)
            }
        }
        bytes
    }
}
impl CompiledProtocol {
    /// Integer inputs are JSON numbers; string inputs are HEX, except ASCII/BCD fields.
    /// Array count references and computed lengths/CRC are always derived from actual data.
    pub fn encode(&self, message_code: u8, values: &Value) -> Result<Vec<u8>, ProtocolError> {
        if !values.is_object() {
            return Err(ProtocolError::new("values", "报文值必须是对象"));
        }
        let message = self
            .messages
            .iter()
            .find(|m| m.code == message_code)
            .ok_or_else(|| {
                ProtocolError::new("unknownMessage", format!("未知信令码 {message_code:02X}"))
            })?;
        let mut state = EncodeState {
            schema: self,
            code: message_code,
            input: values,
            bytes: Vec::new(),
            context: Context::default(),
            overrides: BTreeMap::new(),
            aliases: BTreeMap::new(),
            computed: Vec::new(),
            steps: 0,
        };
        for section in &message.sections {
            state.prepare(
                &section.nodes,
                section.kind.name(),
                section.kind.name(),
                section.kind != SectionKind::Body,
            )?;
        }
        let mut header_len = 0;
        let mut body_len = 0;
        for section in &message.sections {
            if !state
                .context
                .condition(section.when.as_ref(), section.kind.name())?
            {
                continue;
            }
            let start = state.bytes.len();
            state.nodes(
                &section.nodes,
                section.kind.name(),
                section.kind.name(),
                section.kind != SectionKind::Body,
            )?;
            let len = state.bytes.len() - start;
            match section.kind {
                SectionKind::Header | SectionKind::HeaderExtension => header_len += len,
                SectionKind::Body => body_len += len,
                _ => {}
            }
        }
        let total = state.bytes.len();
        for patch in &state.computed {
            let n = match patch.computed.as_str() {
                "headerTotalLength" => header_len,
                "bodyTotalLength" => body_len,
                "packetTotalLength" => total,
                "crc16CcittFalse(packetWithoutTrailer)" => continue,
                _ => continue,
            };
            let encoded = encode_integer(n as i128, patch.length, patch.order, false)?;
            state.bytes[patch.offset..patch.offset + patch.length].copy_from_slice(&encoded);
        }
        for patch in &state.computed {
            if patch.computed == "crc16CcittFalse(packetWithoutTrailer)" {
                if patch.offset + 2 != total {
                    return Err(ProtocolError::new("crcRange", "CRC16必须位于最后2字节"));
                }
                let crc = crc16_ccitt_false(&state.bytes[..patch.offset]);
                let encoded = encode_integer(crc as i128, 2, patch.order, false)?;
                state.bytes[patch.offset..].copy_from_slice(&encoded);
            }
        }
        self.decode(&state.bytes)?;
        Ok(state.bytes)
    }
    pub fn decode(&self, bytes: &[u8]) -> Result<DecodedPacket, ProtocolError> {
        if bytes.len() > self.max_packet_bytes {
            return Err(ProtocolError::new("packetLimit", "报文超过协议最大长度"));
        }
        let code = *bytes.get(self.routing_offset).ok_or_else(|| {
            ProtocolError::new("truncated", "报文不足以读取信令码").at("header", bytes.len())
        })?;
        let message = self
            .messages
            .iter()
            .find(|m| m.code == code)
            .ok_or_else(|| {
                ProtocolError::new("unknownMessage", format!("未知信令码 {code:02X}"))
                    .at("header", self.routing_offset)
            })?;
        let mut state = DecodeState {
            schema: self,
            bytes,
            pos: 0,
            context: Context::default(),
            computed: Vec::new(),
            steps: 0,
        };
        let mut values = Map::new();
        let mut fields = Vec::new();
        let mut header_len = 0;
        let mut body_len = 0;
        for section in &message.sections {
            if !state
                .context
                .condition(section.when.as_ref(), section.kind.name())?
            {
                continue;
            }
            let start = state.pos;
            let (object, children) = state.nodes(
                &section.nodes,
                section.kind.name(),
                section.kind.name(),
                section.kind != SectionKind::Body,
            )?;
            let len = state.pos - start;
            match section.kind {
                SectionKind::Header | SectionKind::HeaderExtension => header_len += len,
                SectionKind::Body => body_len += len,
                _ => {}
            }
            values.insert(section.kind.name().into(), Value::Object(object));
            fields.push(group(
                section.kind.name(),
                section.kind.name(),
                start,
                &bytes[start..state.pos],
                children,
            ));
        }
        if state.pos != bytes.len() {
            return Err(ProtocolError::new(
                "lengthMismatch",
                format!("Schema消费{}字节，实际{}字节", state.pos, bytes.len()),
            )
            .at("packet", state.pos));
        }
        for computed in &state.computed {
            let expected = match computed.computed.as_str() {
                "headerTotalLength" => Some(header_len as i128),
                "bodyTotalLength" => Some(body_len as i128),
                "packetTotalLength" => Some(bytes.len() as i128),
                "crc16CcittFalse(packetWithoutTrailer)" => None,
                _ => Some(
                    state
                        .context
                        .eval_scoped(&Expr::parse(&computed.computed)?, &computed.scope)?,
                ),
            };
            if let Some(expected) = expected
                && computed.actual != expected
            {
                return Err(ProtocolError::new(
                    "lengthMismatch",
                    format!("{}声明{}，实际{expected}", computed.path, computed.actual),
                )
                .at(&computed.path, computed.offset));
            }
        }
        for computed in &state.computed {
            if computed.computed == "crc16CcittFalse(packetWithoutTrailer)" {
                if computed.offset + 2 != bytes.len() {
                    return Err(ProtocolError::new("crcRange", "CRC必须在最后2字节"));
                }
                let expected = crc16_ccitt_false(&bytes[..computed.offset]);
                if computed.actual != expected as i128 {
                    return Err(ProtocolError::new(
                        "crcMismatch",
                        format!("CRC校验失败，期望{expected:04X}"),
                    )
                    .at(&computed.path, computed.offset));
                }
            }
        }
        Ok(DecodedPacket {
            protocol_id: self.id.clone(),
            message_id: message.id.clone(),
            message_code: code,
            raw_hex: format_hex(bytes),
            values: Value::Object(values),
            fields,
            diagnostics: Vec::new(),
        })
    }
}
pub fn crc16_ccitt_false(bytes: &[u8]) -> u16 {
    let mut crc = 0xffffu16;
    for &byte in bytes {
        crc ^= (byte as u16) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}
pub fn set_bits(value: u64, start: u8, width: u8, bits: u64) -> Result<u64, ProtocolError> {
    if width == 0 || u16::from(start) + u16::from(width) > 64 {
        return Err(ProtocolError::new("bitRange", "位范围无效"));
    }
    let mask = if width == 64 {
        u64::MAX
    } else {
        (1u64 << width) - 1
    };
    if bits > mask {
        return Err(ProtocolError::new("bitRange", "位值超出宽度"));
    }
    Ok((value & !(mask << start)) | (bits << start))
}

#[derive(Default)]
struct Context {
    numbers: BTreeMap<String, i128>,
    lengths: BTreeMap<String, usize>,
}
impl Context {
    fn put(&mut self, path: &str, _section: &str, _key: &str, number: Option<i128>, len: usize) {
        {
            let name = path.to_string();
            if let Some(n) = number {
                self.numbers.insert(name.clone(), n);
            }
            self.lengths.insert(name, len);
        }
    }
    fn aliases(&mut self, from: &str, to: &str) {
        for (name, value) in self.numbers.clone() {
            if let Some(key) = name.strip_prefix(&format!("{from}."))
                && !key.contains('.')
                && !key.contains('[')
            {
                self.numbers.insert(format!("{to}.{key}"), value);
            }
        }
        for (name, value) in self.lengths.clone() {
            if let Some(key) = name.strip_prefix(&format!("{from}."))
                && !key.contains('.')
                && !key.contains('[')
            {
                self.lengths.insert(format!("{to}.{key}"), value);
            }
        }
    }
    fn eval_scoped(&self, expr: &Expr, scope: &str) -> Result<i128, ProtocolError> {
        expr.evaluate(&|r| scoped_lookup(&self.numbers, scope, r), &|r| {
            scoped_lookup(&self.lengths, scope, r)
        })
    }
    fn condition(&self, expr: Option<&Expr>, scope: &str) -> Result<bool, ProtocolError> {
        expr.map(|e| self.eval_scoped(e, scope).map(|v| v != 0))
            .unwrap_or(Ok(true))
    }
    fn size(&self, expr: &Expr, scope: &str) -> Result<usize, ProtocolError> {
        let n = self.eval_scoped(expr, scope)?;
        usize::try_from(n).map_err(|_| ProtocolError::new("length", "长度不能为负数或溢出"))
    }
}
fn scoped_lookup<T: Copy>(map: &BTreeMap<String, T>, scope: &str, reference: &str) -> Option<T> {
    let mut current = scope;
    while !current.is_empty() {
        if let Some(value) = map.get(&format!("{current}.{reference}")) {
            return Some(*value);
        }
        current = current
            .rsplit_once('.')
            .map(|(prefix, _)| prefix)
            .unwrap_or("");
    }
    map.get(reference).copied()
}
struct Computed {
    computed: String,
    path: String,
    scope: String,
    offset: usize,
    length: usize,
    order: ByteOrder,
    actual: i128,
}
struct EncodeState<'a> {
    schema: &'a CompiledProtocol,
    code: u8,
    input: &'a Value,
    bytes: Vec<u8>,
    context: Context,
    overrides: BTreeMap<String, Value>,
    aliases: BTreeMap<String, String>,
    computed: Vec<Computed>,
    steps: usize,
}
struct DecodeState<'a> {
    schema: &'a CompiledProtocol,
    bytes: &'a [u8],
    pos: usize,
    context: Context,
    computed: Vec<Computed>,
    steps: usize,
}
fn step(steps: &mut usize) -> Result<(), ProtocolError> {
    *steps += 1;
    if *steps > 100_000 {
        return Err(ProtocolError::new("nodeLimit", "报文展开节点数超限"));
    }
    Ok(())
}
fn scoped(path: &str, key: &str) -> String {
    format!("{path}.{key}")
}
fn lookup<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    for key in path.split('.') {
        let mut parts = key.split('[');
        current = current.get(parts.next()?)?;
        for p in parts {
            let index = p.strip_suffix(']')?.parse::<usize>().ok()?;
            current = current.get(index)?;
        }
    }
    Some(current)
}
fn field_input<'a>(
    input: &'a Value,
    path: &str,
    alias: Option<&str>,
    field: &Field,
) -> Option<&'a Value> {
    lookup(input, path)
        .or_else(|| alias.and_then(|name| lookup(input, name)))
        .or_else(|| {
            field
                .binding
                .as_ref()
                .and_then(|binding| lookup(input, binding))
        })
        .or_else(|| input.get(&field.key))
}
fn unsigned(bytes: &[u8], order: ByteOrder) -> u64 {
    match order {
        ByteOrder::Big => bytes.iter().fold(0u64, |n, b| (n << 8) | u64::from(*b)),
        ByteOrder::Little => bytes
            .iter()
            .rev()
            .fold(0u64, |n, b| (n << 8) | u64::from(*b)),
    }
}
fn numeric(bytes: &[u8], field: &Field) -> Option<i128> {
    if !matches!(
        field.value_kind,
        ValueKind::Uint | ValueKind::Int | ValueKind::Bitset
    ) || bytes.is_empty()
        || bytes.len() > 8
    {
        return None;
    }
    let n = unsigned(bytes, field.endian) as i128;
    let bits = bytes.len() * 8;
    Some(
        if field.value_kind == ValueKind::Int && n & (1i128 << (bits - 1)) != 0 {
            n - (1i128 << bits)
        } else {
            n
        },
    )
}
fn encode_integer(
    n: i128,
    len: usize,
    order: ByteOrder,
    signed: bool,
) -> Result<Vec<u8>, ProtocolError> {
    if len == 0 || len > 8 {
        return Err(ProtocolError::new("length", "数值字段长度必须1..8字节"));
    }
    let bits = len * 8;
    let (min, max) = if signed {
        (-(1i128 << (bits - 1)), (1i128 << (bits - 1)) - 1)
    } else {
        (0, (1i128 << bits) - 1)
    };
    if n < min || n > max {
        return Err(ProtocolError::new(
            "valueRange",
            format!("数值超出{len}字节范围"),
        ));
    }
    let n = if n < 0 { n + (1i128 << bits) } else { n } as u64;
    let bytes = match order {
        ByteOrder::Big => n.to_be_bytes()[8 - len..].to_vec(),
        ByteOrder::Little => n.to_le_bytes()[..len].to_vec(),
    };
    Ok(bytes)
}
fn input_bytes(value: &Value, field: &Field, len: Option<usize>) -> Result<Vec<u8>, ProtocolError> {
    if let Some(s) = value.as_str() {
        return match field.value_kind {
            ValueKind::Ascii => {
                if !s.is_ascii() {
                    Err(ProtocolError::new("ascii", "ASCII字段包含非ASCII字符"))
                } else {
                    Ok(s.as_bytes().to_vec())
                }
            }
            ValueKind::Bcd => {
                if s.len() % 2 != 0 || !s.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(ProtocolError::new("bcd", "BCD必须为偶数位十进制数字串"));
                }
                Ok(s.as_bytes()
                    .chunks(2)
                    .map(|p| (p[0] - b'0') << 4 | (p[1] - b'0'))
                    .collect())
            }
            _ => parse_hex(s),
        };
    }
    if let Some(array) = value.as_array() {
        return array
            .iter()
            .map(|n| {
                n.as_u64()
                    .filter(|v| *v <= 255)
                    .map(|v| v as u8)
                    .ok_or_else(|| ProtocolError::new("valueRange", "字节数组只能包含0..255整数"))
            })
            .collect();
    }
    let n = value
        .as_i64()
        .map(|n| n as i128)
        .or_else(|| value.as_u64().map(|n| n as i128))
        .ok_or_else(|| ProtocolError::new("value", "字段值必须为整数、HEX字符串或字节数组"))?;
    encode_integer(
        n,
        len.ok_or_else(|| ProtocolError::new("length", "动态原始字节字段需要HEX/字节数组"))?,
        field.endian,
        field.value_kind == ValueKind::Int,
    )
}
fn check_value(
    field: &Field,
    bytes: &[u8],
    path: &str,
    offset: usize,
) -> Result<(), ProtocolError> {
    if field
        .fixed
        .as_ref()
        .is_some_and(|fixed| fixed.as_slice() != bytes)
    {
        return Err(
            ProtocolError::new("fixedValue", format!("固定字段{path}不匹配")).at(path, offset),
        );
    }
    if matches!(
        field.value_kind,
        ValueKind::Uint | ValueKind::Int | ValueKind::Bitset
    ) && (bytes.is_empty() || bytes.len() > 8)
    {
        return Err(ProtocolError::new("length", "数值字段长度必须1..8字节").at(path, offset));
    }
    if let Some(n) = numeric(bytes, field)
        && (field.min.is_some_and(|min| n < min) || field.max.is_some_and(|max| n > max))
    {
        return Err(ProtocolError::new("valueRange", "字段值超出Schema范围").at(path, offset));
    }
    if field.value_kind == ValueKind::Ascii && !bytes.is_ascii() {
        return Err(ProtocolError::new("ascii", "ASCII字节无效").at(path, offset));
    }
    if field.value_kind == ValueKind::Bcd && bytes.iter().any(|b| b >> 4 > 9 || b & 0xf > 9) {
        return Err(ProtocolError::new("bcd", "BCD半字节无效").at(path, offset));
    }
    Ok(())
}
fn group(
    path: &str,
    label: &str,
    start: usize,
    bytes: &[u8],
    children: Vec<DecodedField>,
) -> DecodedField {
    DecodedField {
        path: path.into(),
        start_offset0: start,
        byte_start1: start + 1,
        byte_end1: start + bytes.len(),
        byte_length: bytes.len(),
        bit_start: None,
        bit_width: None,
        raw_hex: format_hex(bytes),
        decoded_value: Value::Null,
        decimal: None,
        binary: String::new(),
        label_zh: label.into(),
        description_zh: String::new(),
        unit: None,
        meaning_zh: None,
        secret: false,
        diagnostics: Vec::new(),
        children,
    }
}
fn display_field(field: &Field, path: &str, start: usize, bytes: &[u8]) -> DecodedField {
    let number = numeric(bytes, field);
    let unsigned = if bytes.len() <= 8 {
        Some(unsigned(bytes, field.endian))
    } else {
        None
    };
    let value = match field.value_kind {
        ValueKind::Ascii => json!(String::from_utf8_lossy(bytes).to_string()),
        ValueKind::Bcd => json!(
            bytes
                .iter()
                .map(|b| format!("{:X}{:X}", b >> 4, b & 15))
                .collect::<String>()
        ),
        ValueKind::Bytes => json!(format_hex(bytes)),
        _ => number
            .map(|n| {
                if n < 0 {
                    json!(n as i64)
                } else {
                    json!(n as u64)
                }
            })
            .unwrap_or(Value::Null),
    };
    let meaning = if field.secret || field.enums.is_empty() {
        None
    } else {
        unsigned
            .and_then(|n| field.enums.get(&n))
            .cloned()
            .or_else(|| Some(format!("未定义结果码（{}）", format_hex(bytes))))
    };
    let mut node = group(path, &field.label_zh, start, bytes, Vec::new());
    node.decoded_value = value;
    node.decimal = number.map(|n| n.to_string());
    node.binary = bytes
        .iter()
        .map(|b| format!("{b:08b}"))
        .collect::<Vec<_>>()
        .join(" ");
    node.description_zh = field.description_zh.clone();
    node.unit = field.unit.clone();
    node.meaning_zh = meaning;
    node.secret = field.secret;
    if !field.enums.is_empty() && unsigned.is_some_and(|n| !field.enums.contains_key(&n)) {
        node.diagnostics.push("未定义枚举值，保留原始HEX".into())
    }
    for bit in &field.bits {
        let n = (unsigned.unwrap_or(0) >> bit.start)
            & if bit.width == 64 {
                u64::MAX
            } else {
                (1u64 << bit.width) - 1
            };
        let mut child = group(
            &format!("{path}.{}", bit.key),
            &bit.label_zh,
            start,
            bytes,
            Vec::new(),
        );
        child.bit_start = Some(bit.start);
        child.bit_width = Some(bit.width);
        child.decoded_value = json!(n);
        child.decimal = Some(n.to_string());
        child.binary = format!("{n:0width$b}", width = bit.width as usize);
        child.meaning_zh = bit
            .values
            .get(&n)
            .cloned()
            .or_else(|| Some(format!("{}={n}", bit.label_zh)));
        if field.secret {
            child.meaning_zh = None;
        }
        child.secret = field.secret;
        node.children.push(child)
    }
    node
}

impl EncodeState<'_> {
    fn prepare(
        &mut self,
        nodes: &[Node],
        path: &str,
        section: &str,
        collapse: bool,
    ) -> Result<(), ProtocolError> {
        for node in nodes {
            step(&mut self.steps)?;
            match node {
                Node::Use(u) if u.enabled => {
                    let child_path = if collapse {
                        path.into()
                    } else {
                        scoped(path, &u.reference)
                    };
                    if !collapse {
                        for node in &self.schema.fragments[&u.reference].nodes {
                            if let Node::Field(f) = node {
                                self.aliases
                                    .insert(scoped(&child_path, &f.key), scoped(path, &f.key));
                            }
                        }
                    }
                    self.prepare(
                        &self.schema.fragments[&u.reference].nodes,
                        &child_path,
                        section,
                        collapse,
                    )?;
                }
                Node::Struct(s) => {
                    self.prepare(&s.children, &scoped(path, &s.key), section, false)?
                }
                Node::Array(a) => {
                    let array_path = scoped(path, &a.key);
                    if let Some(items) = lookup(self.input, &array_path)
                        .or_else(|| lookup(self.input, &scoped(section, &a.key)))
                        .and_then(Value::as_array)
                    {
                        if items.len() > a.max_items {
                            return Err(ProtocolError::new("arrayLimit", "数组数量超过maxItems")
                                .at(&array_path, self.bytes.len()));
                        }
                        if let Expr::Ref(r) = &a.count {
                            let key = if r.contains('.') {
                                r.clone()
                            } else {
                                scoped(path, r)
                            };
                            self.overrides.insert(key, json!(items.len()));
                        }
                        for (i, _) in items.iter().enumerate() {
                            let item_path = format!("{array_path}[{i}]");
                            self.prepare(&a.children, &item_path, section, false)?;
                            for v in &a.variants {
                                self.prepare(
                                    &self.schema.fragments[&v.reference].nodes,
                                    &item_path,
                                    section,
                                    false,
                                )?;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
    fn nodes(
        &mut self,
        nodes: &[Node],
        path: &str,
        section: &str,
        collapse: bool,
    ) -> Result<(), ProtocolError> {
        for node in nodes {
            step(&mut self.steps)?;
            match node {
                Node::Field(f) => {
                    if !self.context.condition(f.when.as_ref(), path)? {
                        continue;
                    }
                    let field_path = scoped(path, &f.key);
                    let start = self.bytes.len();
                    let len = if let Some(len) = f.bytes {
                        len
                    } else {
                        self.context.size(f.length_expr.as_ref().unwrap(), path)?
                    };
                    if len > self.schema.max_packet_bytes {
                        return Err(
                            ProtocolError::new("length", "动态字段长度超限").at(&field_path, start)
                        );
                    }
                    let bytes = if let Some(computed) = &f.computed {
                        if computed.starts_with("len(") {
                            let Expr::Len(reference) = Expr::parse(computed)? else {
                                unreachable!()
                            };
                            let target_path = scoped(path, &reference);
                            let target =
                                find_field(nodes, &reference, self.schema).ok_or_else(|| {
                                    ProtocolError::new(
                                        "missingReference",
                                        format!("len引用不存在: {reference}"),
                                    )
                                })?;
                            let value = field_input(
                                self.input,
                                &target_path,
                                self.aliases.get(&target_path).map(String::as_str),
                                target,
                            )
                            .ok_or_else(|| {
                                ProtocolError::new("missingValue", format!("缺少{target_path}"))
                            })?;
                            let bytes = input_bytes(value, target, target.bytes)?;
                            encode_integer(bytes.len() as i128, len, f.endian, false)?
                        } else {
                            self.computed.push(Computed {
                                computed: computed.clone(),
                                path: field_path.clone(),
                                scope: path.into(),
                                offset: start,
                                length: len,
                                order: f.endian,
                                actual: 0,
                            });
                            vec![0; len]
                        }
                    } else if f.binding.as_deref() == Some("message.code") || f.key == "msgCode" {
                        encode_integer(self.code as i128, len, f.endian, false)?
                    } else if let Some(value) = self.overrides.get(&field_path).or_else(|| {
                        self.aliases
                            .get(&field_path)
                            .and_then(|alias| self.overrides.get(alias))
                    }) {
                        input_bytes(value, f, Some(len))?
                    } else if let Some(value) = field_input(
                        self.input,
                        &field_path,
                        self.aliases.get(&field_path).map(String::as_str),
                        f,
                    ) {
                        input_bytes(value, f, Some(len))?
                    } else if let Some(bytes) = f.fixed.as_ref().or(f.default.as_ref()) {
                        bytes.clone()
                    } else {
                        return Err(ProtocolError::new(
                            "missingValue",
                            format!("缺少字段值: {field_path}"),
                        )
                        .at(&field_path, start));
                    };
                    if bytes.len() != len {
                        return Err(ProtocolError::new(
                            "lengthMismatch",
                            format!("{field_path}要求{len}字节，输入{}字节", bytes.len()),
                        )
                        .at(&field_path, start));
                    }
                    if f.computed.is_none() {
                        check_value(f, &bytes, &field_path, start)?;
                    }
                    if self
                        .bytes
                        .len()
                        .checked_add(len)
                        .is_none_or(|n| n > self.schema.max_packet_bytes)
                    {
                        return Err(ProtocolError::new("packetLimit", "编码超过最大报文长度"));
                    }
                    self.context
                        .put(&field_path, section, &f.key, numeric(&bytes, f), len);
                    self.bytes.extend(bytes);
                }
                Node::Use(u) => {
                    if !u.enabled || !self.context.condition(u.when.as_ref(), path)? {
                        continue;
                    }
                    let child_path = if collapse {
                        path.into()
                    } else {
                        scoped(path, &u.reference)
                    };
                    self.nodes(
                        &self.schema.fragments[&u.reference].nodes,
                        &child_path,
                        section,
                        collapse,
                    )?;
                    if !collapse {
                        self.context.aliases(&child_path, path);
                    }
                }
                Node::Struct(s) => {
                    if !self.context.condition(s.when.as_ref(), path)? {
                        continue;
                    }
                    let start = self.bytes.len();
                    let child_path = scoped(path, &s.key);
                    self.nodes(&s.children, &child_path, section, false)?;
                    if s.bytes.is_some_and(|n| n != self.bytes.len() - start) {
                        return Err(ProtocolError::new(
                            "lengthMismatch",
                            "结构体实际长度与声明不符",
                        )
                        .at(&child_path, start));
                    }
                    self.context
                        .put(&child_path, section, &s.key, None, self.bytes.len() - start);
                }
                Node::Array(a) => {
                    if !self.context.condition(a.when.as_ref(), path)? {
                        continue;
                    }
                    let array_path = scoped(path, &a.key);
                    let count = self.context.size(&a.count, path)?;
                    if count > a.max_items {
                        return Err(ProtocolError::new("arrayLimit", "数组数量超过maxItems")
                            .at(&array_path, self.bytes.len()));
                    }
                    let empty = Vec::new();
                    let items = lookup(self.input, &array_path)
                        .or_else(|| lookup(self.input, &scoped(section, &a.key)))
                        .and_then(Value::as_array)
                        .unwrap_or(&empty);
                    if items.len() != count {
                        return Err(ProtocolError::new(
                            "arrayCount",
                            format!("数组{array_path}声明{count}项，输入{}项", items.len()),
                        ));
                    }
                    let expected = a
                        .element_size
                        .as_ref()
                        .map(|e| self.context.size(e, path))
                        .transpose()?;
                    let array_start = self.bytes.len();
                    for (i, _) in items.iter().enumerate() {
                        let start = self.bytes.len();
                        let selected = selected_nodes(a, &self.context, self.schema, path)?;
                        self.nodes(selected, &format!("{array_path}[{i}]"), section, false)?;
                        if expected.is_some_and(|n| self.bytes.len() - start != n) {
                            return Err(ProtocolError::new(
                                "elementSize",
                                "数组条目长度与声明不符",
                            )
                            .at(&array_path, start));
                        }
                    }
                    self.context.put(
                        &array_path,
                        section,
                        &a.key,
                        None,
                        self.bytes.len() - array_start,
                    );
                }
            }
        }
        Ok(())
    }
}
fn find_field<'a>(nodes: &'a [Node], key: &str, schema: &'a CompiledProtocol) -> Option<&'a Field> {
    for node in nodes {
        match node {
            Node::Field(f) if f.key == key => return Some(f),
            Node::Use(u) => {
                if let Some(f) = find_field(&schema.fragments.get(&u.reference)?.nodes, key, schema)
                {
                    return Some(f);
                }
            }
            _ => {}
        }
    }
    None
}
fn selected_nodes<'a>(
    array: &'a Array,
    context: &Context,
    schema: &'a CompiledProtocol,
    scope: &str,
) -> Result<&'a [Node], ProtocolError> {
    if array.variants.is_empty() {
        return Ok(&array.children);
    }
    let mut selected = None;
    for v in &array.variants {
        if context.eval_scoped(&v.selector, scope)? == v.equals {
            if selected.is_some() {
                return Err(ProtocolError::new("variant", "数组同时匹配多个variant"));
            }
            selected = Some(
                schema
                    .fragments
                    .get(&v.reference)
                    .ok_or_else(|| ProtocolError::new("missingReference", "variant片段不存在"))?
                    .nodes
                    .as_slice(),
            );
        }
    }
    selected.ok_or_else(|| ProtocolError::new("variant", "数组没有匹配的variant"))
}
impl DecodeState<'_> {
    fn nodes(
        &mut self,
        nodes: &[Node],
        path: &str,
        section: &str,
        collapse: bool,
    ) -> Result<(Map<String, Value>, Vec<DecodedField>), ProtocolError> {
        let mut values = Map::new();
        let mut fields = Vec::new();
        for node in nodes {
            step(&mut self.steps)?;
            match node {
                Node::Field(f) => {
                    if !self.context.condition(f.when.as_ref(), path)? {
                        continue;
                    }
                    let field_path = scoped(path, &f.key);
                    let start = self.pos;
                    let len = if let Some(len) = f.bytes {
                        len
                    } else {
                        self.context.size(f.length_expr.as_ref().unwrap(), path)?
                    };
                    let end = start
                        .checked_add(len)
                        .filter(|end| *end <= self.bytes.len())
                        .ok_or_else(|| {
                            ProtocolError::new(
                                "truncated",
                                format!("字段{field_path}需要{len}字节"),
                            )
                            .at(&field_path, start)
                        })?;
                    let bytes = &self.bytes[start..end];
                    check_value(f, bytes, &field_path, start)?;
                    let field = display_field(f, &field_path, start, bytes);
                    let number = numeric(bytes, f);
                    self.context.put(&field_path, section, &f.key, number, len);
                    if let Some(computed) = &f.computed {
                        self.computed.push(Computed {
                            computed: computed.clone(),
                            path: field_path.clone(),
                            scope: path.into(),
                            offset: start,
                            length: len,
                            order: f.endian,
                            actual: number.ok_or_else(|| {
                                ProtocolError::new("computed", "计算字段必须是整数")
                            })?,
                        });
                    }
                    values.insert(f.key.clone(), field.decoded_value.clone());
                    fields.push(field);
                    self.pos = end;
                }
                Node::Use(u) => {
                    if !u.enabled || !self.context.condition(u.when.as_ref(), path)? {
                        continue;
                    }
                    let fragment = &self.schema.fragments[&u.reference];
                    let child_path = if collapse {
                        path.into()
                    } else {
                        scoped(path, &u.reference)
                    };
                    let start = self.pos;
                    let (child_values, children) =
                        self.nodes(&fragment.nodes, &child_path, section, collapse)?;
                    if !collapse {
                        self.context.aliases(&child_path, path);
                    }
                    if collapse {
                        values.extend(child_values);
                        fields.extend(children);
                    } else {
                        for (key, value) in &child_values {
                            values.entry(key.clone()).or_insert_with(|| value.clone());
                        }
                        values.insert(u.reference.clone(), Value::Object(child_values));
                        fields.push(group(
                            &child_path,
                            &fragment.name_zh,
                            start,
                            &self.bytes[start..self.pos],
                            children,
                        ));
                    }
                }
                Node::Struct(s) => {
                    if !self.context.condition(s.when.as_ref(), path)? {
                        continue;
                    }
                    let start = self.pos;
                    let child_path = scoped(path, &s.key);
                    let (object, children) =
                        self.nodes(&s.children, &child_path, section, false)?;
                    if s.bytes.is_some_and(|n| n != self.pos - start) {
                        return Err(ProtocolError::new(
                            "lengthMismatch",
                            "结构体实际长度与声明不符",
                        )
                        .at(&child_path, start));
                    }
                    self.context
                        .put(&child_path, section, &s.key, None, self.pos - start);
                    values.insert(s.key.clone(), Value::Object(object));
                    fields.push(group(
                        &child_path,
                        &s.label_zh,
                        start,
                        &self.bytes[start..self.pos],
                        children,
                    ));
                }
                Node::Array(a) => {
                    if !self.context.condition(a.when.as_ref(), path)? {
                        continue;
                    }
                    let array_path = scoped(path, &a.key);
                    let count = self.context.size(&a.count, path)?;
                    let expected = a
                        .element_size
                        .as_ref()
                        .map(|e| self.context.size(e, path))
                        .transpose()?;
                    if count > a.max_items {
                        return Err(ProtocolError::new("arrayLimit", "数组数量超过maxItems")
                            .at(&array_path, self.pos));
                    }
                    if expected.is_some_and(|size| {
                        count
                            .checked_mul(size)
                            .is_none_or(|needed| needed > self.bytes.len() - self.pos)
                    }) {
                        return Err(ProtocolError::new("truncated", "数组声明长度超过剩余报文")
                            .at(&array_path, self.pos));
                    }
                    let start = self.pos;
                    let mut children = Vec::new();
                    let mut items = Vec::new();
                    for index in 0..count {
                        step(&mut self.steps)?;
                        let item_start = self.pos;
                        let item_path = format!("{array_path}[{index}]");
                        let selected = selected_nodes(a, &self.context, self.schema, path)?;
                        let (item_values, item_fields) =
                            self.nodes(selected, &item_path, section, false)?;
                        if expected.is_some_and(|size| self.pos - item_start != size) {
                            return Err(ProtocolError::new(
                                "elementSize",
                                "数组条目长度与elementSize声明不符",
                            )
                            .at(&item_path, item_start));
                        }
                        items.push(Value::Object(item_values));
                        children.push(group(
                            &item_path,
                            &format!("{} [{}]", a.label_zh, index),
                            item_start,
                            &self.bytes[item_start..self.pos],
                            item_fields,
                        ));
                    }
                    self.context
                        .put(&array_path, section, &a.key, None, self.pos - start);
                    values.insert(a.key.clone(), Value::Array(items));
                    fields.push(group(
                        &array_path,
                        &a.label_zh,
                        start,
                        &self.bytes[start..self.pos],
                        children,
                    ));
                }
            }
        }
        Ok((values, fields))
    }
}
