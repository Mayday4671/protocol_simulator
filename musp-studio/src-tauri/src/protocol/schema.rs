use super::{
    ProtocolError,
    expression::{Expr, identifier},
};
use quick_xml::{Reader, events::Event};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_XML_BYTES: usize = 1_048_576;
pub const MAX_PACKET_BYTES: usize = 65_507;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceLocation {
    pub offset: usize,
    pub line: usize,
    pub column: usize,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ByteOrder {
    Big,
    Little,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ValueKind {
    Uint,
    Int,
    Bytes,
    Ascii,
    Bcd,
    Bitset,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BitField {
    pub key: String,
    pub label_zh: String,
    pub start: u8,
    pub width: u8,
    pub values: BTreeMap<u64, String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub key: String,
    pub label_zh: String,
    pub description_zh: String,
    pub bytes: Option<usize>,
    pub length_expr: Option<Expr>,
    pub value_kind: ValueKind,
    pub endian: ByteOrder,
    pub fixed: Option<Vec<u8>>,
    pub default: Option<Vec<u8>>,
    pub binding: Option<String>,
    pub computed: Option<String>,
    pub when: Option<Expr>,
    pub enums: BTreeMap<u64, String>,
    pub bits: Vec<BitField>,
    pub unit: Option<String>,
    pub scale: Option<f64>,
    pub secret: bool,
    pub min: Option<i128>,
    pub max: Option<i128>,
    pub location: SourceLocation,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FragmentUse {
    pub reference: String,
    pub enabled: bool,
    pub when: Option<Expr>,
    pub location: SourceLocation,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Variant {
    pub selector: Expr,
    pub equals: i128,
    pub reference: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Array {
    pub key: String,
    pub label_zh: String,
    pub count: Expr,
    pub element_size: Option<Expr>,
    pub max_items: usize,
    pub variants: Vec<Variant>,
    pub children: Vec<Node>,
    pub when: Option<Expr>,
    pub location: SourceLocation,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Struct {
    pub key: String,
    pub label_zh: String,
    pub bytes: Option<usize>,
    pub when: Option<Expr>,
    pub children: Vec<Node>,
    pub location: SourceLocation,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "definition", rename_all = "camelCase")]
pub enum Node {
    Field(Field),
    Use(FragmentUse),
    Array(Array),
    Struct(Struct),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fragment {
    pub id: String,
    pub name_zh: String,
    pub nodes: Vec<Node>,
    pub location: SourceLocation,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SectionKind {
    Header,
    HeaderExtension,
    Body,
    Trailer,
}
impl SectionKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::Header => "header",
            Self::HeaderExtension => "headerExtension",
            Self::Body => "body",
            Self::Trailer => "trailer",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub kind: SectionKind,
    pub when: Option<Expr>,
    pub nodes: Vec<Node>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    pub code: u8,
    pub name_zh: String,
    pub direction: Option<String>,
    pub response_codes: Vec<u8>,
    pub sections: Vec<Section>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompiledProtocol {
    pub id: String,
    pub name_zh: String,
    pub version: String,
    pub schema_version: u32,
    pub byte_order: ByteOrder,
    pub max_packet_bytes: usize,
    pub fragments: BTreeMap<String, Fragment>,
    pub messages: Vec<Message>,
    pub(crate) routing_offset: usize,
}

#[derive(Debug)]
struct Xml {
    name: String,
    attrs: BTreeMap<String, String>,
    children: Vec<Xml>,
    location: SourceLocation,
}
impl Xml {
    fn required(&self, key: &str) -> Result<&str, ProtocolError> {
        self.attrs
            .get(key)
            .map(String::as_str)
            .ok_or_else(|| self.error("schema", format!("{} 缺少 {key}", self.name)))
    }
    fn opt(&self, key: &str) -> Option<&str> {
        self.attrs.get(key).map(String::as_str)
    }
    fn label(&self) -> String {
        self.opt("labelZh")
            .or_else(|| self.opt("nameZh"))
            .unwrap_or("")
            .into()
    }
    fn error(&self, code: &str, message: impl Into<String>) -> ProtocolError {
        locate(ProtocolError::new(code, message), &self.location)
    }
    fn expr(&self, key: &str) -> Result<Option<Expr>, ProtocolError> {
        self.opt(key)
            .map(Expr::parse)
            .transpose()
            .map_err(|e| locate(e, &self.location))
    }
    fn number(&self, key: &str) -> Result<Option<usize>, ProtocolError> {
        self.opt(key)
            .map(|s| {
                s.parse()
                    .map_err(|_| self.error("schema", format!("{key}必须是非负十进制整数")))
            })
            .transpose()
    }
    fn bool(&self, key: &str, default: bool) -> Result<bool, ProtocolError> {
        match self.opt(key) {
            None => Ok(default),
            Some("true") => Ok(true),
            Some("false") => Ok(false),
            _ => Err(self.error("schema", format!("{key}必须是true或false"))),
        }
    }
    fn children_only(&self, names: &[&str]) -> Result<(), ProtocolError> {
        for child in &self.children {
            if !names.contains(&child.name.as_str()) {
                return Err(child.error("unknownElement", format!("不支持元素 {}", child.name)));
            }
        }
        Ok(())
    }
    fn validate_attributes(&self) -> Result<(), ProtocolError> {
        let allowed: &[&str] = match self.name.as_str() {
            "protocol" => &[
                "id",
                "nameZh",
                "version",
                "schemaVersion",
                "byteOrder",
                "maxPacketBytes",
            ],
            "fragments" | "messages" | "bits" => &[],
            "fragment" => &["id", "nameZh", "section"],
            "message" => &["id", "code", "nameZh", "direction", "responseCodes"],
            "header" | "headerExtension" | "body" | "trailer" => &["ref", "when"],
            "use" => &["ref", "enabled", "when"],
            "field" => &[
                "key",
                "labelZh",
                "descriptionZh",
                "bytes",
                "lengthBytes",
                "lengthExpr",
                "display",
                "valueKind",
                "endian",
                "signed",
                "fixed",
                "default",
                "value",
                "binding",
                "computed",
                "when",
                "unit",
                "scale",
                "secret",
                "min",
                "max",
            ],
            "struct" => &["key", "labelZh", "bytes", "lengthBytes", "when"],
            "array" => &[
                "key",
                "labelZh",
                "countRef",
                "countExpr",
                "elementSizeRef",
                "elementBytes",
                "maxItems",
                "when",
            ],
            "variant" => &["selector", "equals", "ref"],
            "enum" => &["hex", "labelZh"],
            "bit" => &["key", "labelZh", "start", "bitStart", "width"],
            _ => return Err(self.error("unknownElement", format!("不支持元素 {}", self.name))),
        };
        for key in self.attrs.keys() {
            if !allowed.contains(&key.as_str()) {
                return Err(self.error(
                    "unknownAttribute",
                    format!("{}暂不支持属性{key}", self.name),
                ));
            }
        }
        Ok(())
    }
}
fn locate(mut e: ProtocolError, l: &SourceLocation) -> ProtocolError {
    e.offset = Some(l.offset);
    e.line = Some(l.line);
    e.column = Some(l.column);
    e
}
fn location(source: &str, offset: usize) -> SourceLocation {
    let prefix = &source[..source.floor_char_boundary(offset.min(source.len()))];
    SourceLocation {
        offset,
        line: prefix.bytes().filter(|b| *b == b'\n').count() + 1,
        column: prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1,
    }
}

fn parse_tree(source: &str) -> Result<Xml, ProtocolError> {
    if source.len() > MAX_XML_BYTES {
        return Err(ProtocolError::new("xmlLimit", "XML超过1MiB"));
    }
    let mut reader = Reader::from_str(source);
    reader.config_mut().trim_text(true);
    let mut stack = Vec::<Xml>::new();
    let mut root = None;
    let mut count = 0;
    loop {
        let offset = reader.buffer_position() as usize;
        let event = reader.read_event().map_err(|e| {
            locate(
                ProtocolError::new("xmlSyntax", e.to_string()),
                &location(source, offset),
            )
        })?;
        match event {
            Event::Start(ref start) | Event::Empty(ref start) => {
                let offset = offset
                    + source[offset..reader.buffer_position() as usize]
                        .find('<')
                        .unwrap_or(0);
                count += 1;
                if count > 10000 || stack.len() >= 32 {
                    return Err(locate(
                        ProtocolError::new("xmlLimit", "XML节点数或嵌套深度超限"),
                        &location(source, offset),
                    ));
                }
                let name = std::str::from_utf8(start.name().as_ref())
                    .map_err(|_| ProtocolError::new("xmlSyntax", "元素名称不是UTF8"))?
                    .to_string();
                let mut attrs = BTreeMap::new();
                for attr in start.attributes() {
                    let attr = attr.map_err(|e| {
                        locate(
                            ProtocolError::new("xmlSyntax", e.to_string()),
                            &location(source, offset),
                        )
                    })?;
                    let key = std::str::from_utf8(attr.key.as_ref())
                        .map_err(|_| ProtocolError::new("xmlSyntax", "属性不是UTF8"))?
                        .to_string();
                    let value = attr
                        .decode_and_unescape_value(reader.decoder())
                        .map_err(|e| {
                            locate(
                                ProtocolError::new("xmlSecurity", e.to_string()),
                                &location(source, offset),
                            )
                        })?
                        .into_owned();
                    attrs.insert(key, value);
                }
                let node = Xml {
                    name,
                    attrs,
                    children: Vec::new(),
                    location: location(source, offset),
                };
                node.validate_attributes()?;
                if matches!(event, Event::Start(_)) {
                    stack.push(node)
                } else if let Some(parent) = stack.last_mut() {
                    parent.children.push(node)
                } else if root.replace(node).is_some() {
                    return Err(ProtocolError::new("xmlSyntax", "XML必须只有一个根节点"));
                }
            }
            Event::End(_) => {
                let node = stack
                    .pop()
                    .ok_or_else(|| ProtocolError::new("xmlSyntax", "多余的结束标签"))?;
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node)
                } else if root.replace(node).is_some() {
                    return Err(ProtocolError::new("xmlSyntax", "XML必须只有一个根节点"));
                }
            }
            Event::DocType(_) | Event::GeneralRef(_) => {
                return Err(locate(
                    ProtocolError::new("xmlSecurity", "禁止DTD和实体引用"),
                    &location(source, offset),
                ));
            }
            Event::Text(t) if !t.is_empty() => {
                return Err(locate(
                    ProtocolError::new("xmlSyntax", "DSL元素不能包含文本节点；使用属性保存说明"),
                    &location(source, offset),
                ));
            }
            Event::CData(_) | Event::PI(_) => {
                return Err(locate(
                    ProtocolError::new("xmlSecurity", "禁止CDATA和处理指令"),
                    &location(source, offset),
                ));
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !stack.is_empty() {
        return Err(ProtocolError::new("xmlSyntax", "XML元素未闭合"));
    }
    root.ok_or_else(|| ProtocolError::new("xmlSyntax", "XML为空"))
}

pub fn parse_hex(text: &str) -> Result<Vec<u8>, ProtocolError> {
    let text = text
        .trim()
        .strip_prefix("0x")
        .or_else(|| text.trim().strip_prefix("0X"))
        .unwrap_or(text.trim());
    let compact: String = text
        .chars()
        .filter(|c| c.is_ascii_whitespace().not())
        .collect();
    if !compact.len().is_multiple_of(2) || !compact.is_ascii() {
        return Err(ProtocolError::new("hex", "HEX必须是完整字节"));
    }
    compact
        .as_bytes()
        .chunks(2)
        .map(|pair| {
            u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16)
                .map_err(|_| ProtocolError::new("hex", "HEX包含非法字符"))
        })
        .collect()
}
use std::ops::Not;
pub fn format_hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|x| format!("{x:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}
fn hex_number(s: &str) -> Result<u64, ProtocolError> {
    u64::from_str_radix(
        s.trim().trim_start_matches("0x").trim_start_matches("0X"),
        16,
    )
    .map_err(|_| ProtocolError::new("schema", "无效十六进制数"))
}
fn byte_order(s: &str) -> Result<ByteOrder, ProtocolError> {
    match s {
        "big" => Ok(ByteOrder::Big),
        "little" => Ok(ByteOrder::Little),
        _ => Err(ProtocolError::new(
            "schema",
            "byteOrder/endian必须是big或little",
        )),
    }
}
fn machine_key(x: &Xml, key: &str) -> Result<String, ProtocolError> {
    let value = x.required(key)?;
    if !identifier(value) || value.contains('.') {
        return Err(x.error("schema", format!("{key}必须是英文标识符")));
    }
    Ok(value.into())
}
fn parse_nodes(nodes: &[Xml], order: ByteOrder) -> Result<Vec<Node>, ProtocolError> {
    let mut result = Vec::new();
    let mut keys = BTreeSet::new();
    for x in nodes {
        let node = match x.name.as_str() {
            "field" => {
                x.children_only(&["bits", "enum"])?;
                let key = machine_key(x, "key")?;
                let bytes = x.number("bytes")?.or(x.number("lengthBytes")?);
                let length_expr = x.expr("lengthExpr")?;
                if bytes.is_some() == length_expr.is_some() {
                    return Err(x.error("schema", "field必须且只能指定bytes或lengthExpr"));
                }
                if bytes.is_some_and(|n| n == 0 || n > MAX_PACKET_BYTES) {
                    return Err(x.error("length", "字段长度超限"));
                }
                let mut kind = match x.opt("valueKind").unwrap_or("uint") {
                    "uint" => ValueKind::Uint,
                    "int" => ValueKind::Int,
                    "bytes" | "rawBytes" => ValueKind::Bytes,
                    "ascii" => ValueKind::Ascii,
                    "bcd" => ValueKind::Bcd,
                    "bitset" => ValueKind::Bitset,
                    _ => return Err(x.error("schema", "不支持valueKind")),
                };
                if x.bool("signed", false)? {
                    if kind != ValueKind::Uint && kind != ValueKind::Int {
                        return Err(x.error("schema", "signed只支持数值字段"));
                    }
                    kind = ValueKind::Int;
                }
                if matches!(kind, ValueKind::Uint | ValueKind::Int | ValueKind::Bitset)
                    && bytes.is_some_and(|n| n > 8)
                {
                    return Err(x.error("length", "整数字段最多8字节"));
                }
                if x.opt("display").is_some_and(|s| s != "HEX") {
                    return Err(x.error("schema", "display必须为HEX"));
                }
                let fixed = x.opt("fixed").map(parse_hex).transpose()?;
                let default = x
                    .opt("default")
                    .or_else(|| x.opt("value"))
                    .map(parse_hex)
                    .transpose()?;
                for b in [&fixed, &default].into_iter().flatten() {
                    if bytes.is_some_and(|n| n != b.len()) {
                        return Err(x.error("length", "固定值/默认值长度与字段不符"));
                    }
                }
                let mut enums = BTreeMap::new();
                let mut bits = Vec::new();
                let mut bit_mask = 0u64;
                let mut bit_keys = BTreeSet::new();
                for child in &x.children {
                    match child.name.as_str() {
                        "enum" => {
                            child.children_only(&[])?;
                            let value = hex_number(child.required("hex")?)?;
                            if enums.insert(value, child.label()).is_some() {
                                return Err(child.error("duplicateKey", "重复枚举值"));
                            }
                        }
                        "bits" => {
                            child.children_only(&["bit"])?;
                            for bit in &child.children {
                                bit.children_only(&["enum"])?;
                                let key = machine_key(bit, "key")?;
                                if !bit_keys.insert(key.clone()) {
                                    return Err(bit.error("duplicateKey", "重复bit key"));
                                }
                                let start = bit
                                    .number("start")?
                                    .or(bit.number("bitStart")?)
                                    .unwrap_or(0);
                                let width = bit.number("width")?.unwrap_or(1);
                                if width == 0
                                    || start + width > bytes.unwrap_or(0) * 8
                                    || start + width > 64
                                {
                                    return Err(bit.error("bitRange", "位域超出字段长度"));
                                }
                                let mask = if width == 64 {
                                    u64::MAX
                                } else {
                                    ((1u64 << width) - 1) << start
                                };
                                if mask & bit_mask != 0 {
                                    return Err(bit.error("bitOverlap", "位域重叠"));
                                }
                                bit_mask |= mask;
                                let mut values = BTreeMap::new();
                                for v in &bit.children {
                                    values.insert(hex_number(v.required("hex")?)?, v.label());
                                }
                                bits.push(BitField {
                                    key,
                                    label_zh: bit.label(),
                                    start: start as u8,
                                    width: width as u8,
                                    values,
                                });
                            }
                        }
                        _ => unreachable!(),
                    }
                }
                let computed = x.opt("computed").map(str::to_string);
                if let Some(c) = &computed
                    && !matches!(
                        c.as_str(),
                        "headerTotalLength"
                            | "packetTotalLength"
                            | "bodyTotalLength"
                            | "crc16CcittFalse(packetWithoutTrailer)"
                    )
                    && !matches!(Expr::parse(c)?, Expr::Len(_))
                {
                    return Err(x.error("schema", "computed目前只支持长度和CRC计算"));
                }
                if computed.as_deref() == Some("crc16CcittFalse(packetWithoutTrailer)")
                    && bytes != Some(2)
                {
                    return Err(x.error("length", "CRC16字段必须是2字节"));
                }
                Node::Field(Field {
                    key,
                    label_zh: x.label(),
                    description_zh: x.opt("descriptionZh").unwrap_or("").into(),
                    bytes,
                    length_expr,
                    value_kind: kind,
                    endian: byte_order(x.opt("endian").unwrap_or(match order {
                        ByteOrder::Big => "big",
                        ByteOrder::Little => "little",
                    }))?,
                    fixed,
                    default,
                    binding: x.opt("binding").map(str::to_string),
                    computed,
                    when: x.expr("when")?,
                    enums,
                    bits,
                    unit: x.opt("unit").map(str::to_string),
                    scale: x
                        .opt("scale")
                        .map(|s| s.parse::<f64>().map_err(|_| x.error("schema", "scale无效")))
                        .transpose()?,
                    secret: x.bool("secret", false)?,
                    min: x
                        .opt("min")
                        .map(|s| s.parse().map_err(|_| x.error("schema", "min无效")))
                        .transpose()?,
                    max: x
                        .opt("max")
                        .map(|s| s.parse().map_err(|_| x.error("schema", "max无效")))
                        .transpose()?,
                    location: x.location.clone(),
                })
            }
            "use" => {
                x.children_only(&[])?;
                Node::Use(FragmentUse {
                    reference: x.required("ref")?.into(),
                    enabled: x.bool("enabled", true)?,
                    when: x.expr("when")?,
                    location: x.location.clone(),
                })
            }
            "array" => {
                x.children_only(&["field", "use", "array", "struct", "variant"])?;
                let key = machine_key(x, "key")?;
                let count = Expr::parse(
                    x.opt("countRef")
                        .or_else(|| x.opt("countExpr"))
                        .ok_or_else(|| x.error("schema", "array缺少countRef/countExpr"))?,
                )?;
                let element_size = x
                    .opt("elementSizeRef")
                    .or_else(|| x.opt("elementBytes"))
                    .map(Expr::parse)
                    .transpose()?;
                let max_items = x.number("maxItems")?.unwrap_or(1024);
                if max_items > 4096 {
                    return Err(x.error("arrayLimit", "maxItems不得超过4096"));
                }
                let mut variants = Vec::new();
                let mut raw_children = Vec::new();
                let mut variant_codes = BTreeSet::new();
                for child in &x.children {
                    if child.name == "variant" {
                        child.children_only(&[])?;
                        let equals = hex_number(child.required("equals")?)? as i128;
                        if !variant_codes.insert(equals) {
                            return Err(child.error("duplicateKey", "variant选择值重复"));
                        }
                        variants.push(Variant {
                            selector: Expr::parse(child.required("selector")?)?,
                            equals,
                            reference: child.required("ref")?.into(),
                        });
                    } else {
                        raw_children.push(child)
                    }
                }
                if !variants.is_empty() && !raw_children.is_empty() {
                    return Err(x.error("schema", "array不能同时定义variant和内联字段"));
                }
                let children = parse_node_refs(&raw_children, order)?;
                if variants.is_empty() && children.is_empty() {
                    return Err(x.error("schema", "array缺少条目结构"));
                }
                Node::Array(Array {
                    key,
                    label_zh: x.label(),
                    count,
                    element_size,
                    max_items,
                    variants,
                    children,
                    when: x.expr("when")?,
                    location: x.location.clone(),
                })
            }
            "struct" => {
                let bytes = x.number("bytes")?.or(x.number("lengthBytes")?);
                if bytes.is_some_and(|n| n > MAX_PACKET_BYTES) {
                    return Err(x.error("length", "结构体长度超限"));
                }
                Node::Struct(Struct {
                    key: machine_key(x, "key")?,
                    label_zh: x.label(),
                    bytes,
                    when: x.expr("when")?,
                    children: parse_nodes(&x.children, order)?,
                    location: x.location.clone(),
                })
            }
            _ => return Err(x.error("unknownElement", format!("不支持元素 {}", x.name))),
        };
        let key = match &node {
            Node::Field(f) => &f.key,
            Node::Array(a) => &a.key,
            Node::Use(u) => &u.reference,
            Node::Struct(s) => &s.key,
        };
        if !keys.insert(key.clone()) {
            return Err(x.error("duplicateKey", format!("当前作用域key重复: {key}")));
        }
        result.push(node);
    }
    Ok(result)
}
fn parse_node_refs(nodes: &[&Xml], order: ByteOrder) -> Result<Vec<Node>, ProtocolError> {
    let mut result = Vec::new();
    let mut keys = BTreeSet::new();
    for x in nodes {
        let mut parsed = parse_nodes(std::slice::from_ref(*x), order)?;
        let key = match &parsed[0] {
            Node::Field(f) => f.key.clone(),
            Node::Array(a) => a.key.clone(),
            Node::Use(u) => u.reference.clone(),
            Node::Struct(s) => s.key.clone(),
        };
        if !keys.insert(key) {
            return Err(x.error("duplicateKey", "数组子字段key重复"));
        }
        result.append(&mut parsed)
    }
    Ok(result)
}

pub fn compile_xml(source: &str) -> Result<CompiledProtocol, ProtocolError> {
    compile_inner(source).map_err(|e| {
        if e.line.is_none() {
            locate(e, &location(source, 0))
        } else {
            e
        }
    })
}
fn compile_inner(source: &str) -> Result<CompiledProtocol, ProtocolError> {
    let root = parse_tree(source)?;
    if root.name != "protocol" {
        return Err(root.error("schema", "根节点必须为protocol"));
    }
    root.children_only(&["fragments", "messages"])?;
    let schema_version = root.number("schemaVersion")?.unwrap_or(1);
    if schema_version != 1 {
        return Err(root.error("schemaVersion", "仅支持schemaVersion=1"));
    }
    let order = byte_order(root.opt("byteOrder").unwrap_or("big"))?;
    let max_packet_bytes = root.number("maxPacketBytes")?.unwrap_or(MAX_PACKET_BYTES);
    if max_packet_bytes == 0 || max_packet_bytes > MAX_PACKET_BYTES {
        return Err(root.error("packetLimit", "maxPacketBytes必须在1..65507"));
    }
    let mut fragments = BTreeMap::new();
    let mut messages = Vec::new();
    let mut codes = BTreeSet::new();
    let mut ids = BTreeSet::new();
    for group in &root.children {
        match group.name.as_str() {
            "fragments" => {
                group.children_only(&["fragment"])?;
                for f in &group.children {
                    let id = machine_key(f, "id")?;
                    let fragment = Fragment {
                        id: id.clone(),
                        name_zh: f.label(),
                        nodes: parse_nodes(&f.children, order)?,
                        location: f.location.clone(),
                    };
                    if fragments.insert(id.clone(), fragment).is_some() {
                        return Err(f.error("duplicateKey", format!("片段id重复: {id}")));
                    }
                }
            }
            "messages" => {
                group.children_only(&["message"])?;
                for m in &group.children {
                    m.children_only(&["header", "headerExtension", "body", "trailer"])?;
                    let id = machine_key(m, "id")?;
                    let code = u8::try_from(hex_number(m.required("code")?)?)
                        .map_err(|_| m.error("messageCode", "信令码必须是1字节"))?;
                    if !codes.insert(code) || !ids.insert(id.clone()) {
                        return Err(m.error("duplicateKey", "信令码或id重复"));
                    }
                    let response_codes = m
                        .opt("responseCodes")
                        .unwrap_or("")
                        .split([',', ' ', ';'])
                        .filter(|s| !s.is_empty())
                        .map(|s| {
                            u8::try_from(hex_number(s)?)
                                .map_err(|_| m.error("messageCode", "响应码必须1字节"))
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    let mut sections = Vec::new();
                    let mut previous = 0;
                    let mut section_names = BTreeSet::new();
                    for section in &m.children {
                        let (kind, rank) = match section.name.as_str() {
                            "header" => (SectionKind::Header, 0),
                            "headerExtension" => (SectionKind::HeaderExtension, 1),
                            "body" => (SectionKind::Body, 2),
                            "trailer" => (SectionKind::Trailer, 3),
                            _ => unreachable!(),
                        };
                        if rank < previous || !section_names.insert(section.name.clone()) {
                            return Err(section.error("schema", "section重复或顺序无效"));
                        }
                        previous = rank;
                        let nodes = if let Some(reference) = section.opt("ref") {
                            section.children_only(&[])?;
                            vec![Node::Use(FragmentUse {
                                reference: reference.into(),
                                enabled: true,
                                when: None,
                                location: section.location.clone(),
                            })]
                        } else {
                            parse_nodes(&section.children, order)?
                        };
                        sections.push(Section {
                            kind,
                            when: section.expr("when")?,
                            nodes,
                        });
                    }
                    if sections.first().map(|s| s.kind) != Some(SectionKind::Header) {
                        return Err(m.error("schema", "信令必须以header开始"));
                    }
                    messages.push(Message {
                        id,
                        code,
                        name_zh: m.label(),
                        direction: m.opt("direction").map(str::to_string),
                        response_codes,
                        sections,
                    });
                }
            }
            _ => unreachable!(),
        }
    }
    if messages.is_empty() {
        return Err(root.error("schema", "协议至少需要一条信令"));
    }
    let mut protocol = CompiledProtocol {
        id: root.required("id")?.into(),
        name_zh: root.label(),
        version: root.required("version")?.into(),
        schema_version: schema_version as u32,
        byte_order: order,
        max_packet_bytes,
        fragments,
        messages,
        routing_offset: 0,
    };
    protocol.validate()?;
    Ok(protocol)
}

impl CompiledProtocol {
    fn validate(&mut self) -> Result<(), ProtocolError> {
        let mut budget = 100_000usize;
        for fragment in self.fragments.values() {
            self.references(
                &fragment.nodes,
                &mut vec![fragment.id.clone()],
                0,
                &mut budget,
            )?;
        }
        let mut routing = None;
        for message in &self.messages {
            let mut available = BTreeSet::new();
            let mut defaults = BTreeMap::new();
            for section in &message.sections {
                self.references(&section.nodes, &mut Vec::new(), 0, &mut budget)?;
                check_expr(section.when.as_ref(), &available, section.kind.name())?;
                let mut conditional_available = available.clone();
                self.dependencies(
                    &section.nodes,
                    section.kind.name(),
                    &mut conditional_available,
                )?;
                if section.when.is_none() {
                    available = conditional_available;
                }
                self.static_lengths(&section.nodes, section.kind.name(), &mut defaults)?;
            }
            let mut offset = 0;
            let mut found = None;
            self.route(&message.sections[0].nodes, &mut offset, &mut found)?;
            let found = found.ok_or_else(|| {
                ProtocolError::new(
                    "messageCode",
                    "公共头缺少binding=message.code的1B信令码字段",
                )
            })?;
            if routing.is_some_and(|x| x != found) {
                return Err(ProtocolError::new(
                    "messageCode",
                    "各信令公共头的信令码位置必须一致",
                ));
            }
            routing = Some(found);
        }
        self.routing_offset = routing.unwrap();
        Ok(())
    }
    fn references(
        &self,
        nodes: &[Node],
        stack: &mut Vec<String>,
        depth: usize,
        budget: &mut usize,
    ) -> Result<(), ProtocolError> {
        if depth > 16 {
            return Err(ProtocolError::new("referenceLimit", "片段引用超过16层"));
        }
        for node in nodes {
            if *budget == 0 {
                return Err(ProtocolError::new(
                    "referenceLimit",
                    "Schema展开超过100000节点",
                ));
            }
            *budget -= 1;
            match node {
                Node::Use(u) => self.reference(&u.reference, stack, depth, budget)?,
                Node::Array(a) => {
                    self.references(&a.children, stack, depth + 1, budget)?;
                    for v in &a.variants {
                        self.reference(&v.reference, stack, depth, budget)?;
                    }
                }
                Node::Struct(s) => self.references(&s.children, stack, depth + 1, budget)?,
                _ => {}
            }
        }
        Ok(())
    }
    fn reference(
        &self,
        id: &str,
        stack: &mut Vec<String>,
        depth: usize,
        budget: &mut usize,
    ) -> Result<(), ProtocolError> {
        if stack.iter().any(|s| s == id) {
            return Err(ProtocolError::new(
                "cyclicReference",
                format!("片段循环引用: {id}"),
            ));
        }
        let f = self
            .fragments
            .get(id)
            .ok_or_else(|| ProtocolError::new("missingReference", format!("片段不存在: {id}")))?;
        stack.push(id.into());
        self.references(&f.nodes, stack, depth + 1, budget)?;
        stack.pop();
        Ok(())
    }
    fn dependencies(
        &self,
        nodes: &[Node],
        scope: &str,
        available: &mut BTreeSet<String>,
    ) -> Result<(), ProtocolError> {
        for node in nodes {
            match node {
                Node::Field(f) => {
                    check_expr(f.when.as_ref(), available, scope)
                        .map_err(|e| locate(e, &f.location))?;
                    check_expr(f.length_expr.as_ref(), available, scope)
                        .map_err(|e| locate(e, &f.location))?;
                    if let Some(computed) = &f.computed {
                        if !matches!(f.value_kind, ValueKind::Uint | ValueKind::Int) {
                            return Err(locate(
                                ProtocolError::new("computed", "计算字段必须是整数"),
                                &f.location,
                            ));
                        }
                        if computed.starts_with("len(") {
                            let Expr::Len(reference) = Expr::parse(computed)? else {
                                unreachable!()
                            };
                            if !nodes.iter().any(|n|matches!(n,Node::Field(target) if target.key==reference&&matches!(target.value_kind,ValueKind::Bytes|ValueKind::Ascii|ValueKind::Bcd))) {
                                return Err(locate(ProtocolError::new("missingReference",format!("len必须引用同作用域的字节/ASCII/BCD字段: {reference}")),&f.location));
                            }
                        }
                    }
                    if f.when.is_none() {
                        let name = format!("{scope}.{}", f.key);
                        available.insert(format!("len:{name}"));
                        if matches!(
                            f.value_kind,
                            ValueKind::Uint | ValueKind::Int | ValueKind::Bitset
                        ) {
                            available.insert(name);
                        }
                    }
                }
                Node::Use(u) => {
                    check_expr(u.when.as_ref(), available, scope)
                        .map_err(|e| locate(e, &u.location))?;
                    let fragment = self.fragments.get(&u.reference).ok_or_else(|| {
                        locate(
                            ProtocolError::new(
                                "missingReference",
                                format!("片段不存在: {}", u.reference),
                            ),
                            &u.location,
                        )
                    })?;
                    if u.enabled {
                        let child_scope =
                            if matches!(scope, "header" | "headerExtension" | "trailer") {
                                scope.to_string()
                            } else {
                                format!("{scope}.{}", u.reference)
                            };
                        let mut child = available.clone();
                        self.dependencies(&fragment.nodes, &child_scope, &mut child)?;
                        if u.when.is_none() {
                            for name in &child {
                                let (prefix, name) = name
                                    .strip_prefix("len:")
                                    .map(|n| ("len:", n))
                                    .unwrap_or(("", name.as_str()));
                                if let Some(key) = name.strip_prefix(&format!("{child_scope}."))
                                    && !key.contains('.')
                                    && !key.contains('[')
                                {
                                    available.insert(format!("{prefix}{scope}.{key}"));
                                }
                            }
                            available.extend(child);
                        }
                    }
                }
                Node::Struct(s) => {
                    check_expr(s.when.as_ref(), available, scope)
                        .map_err(|e| locate(e, &s.location))?;
                    let child_scope = format!("{scope}.{}", s.key);
                    let mut child = available.clone();
                    self.dependencies(&s.children, &child_scope, &mut child)?;
                    if let (Some(expected), Some(actual)) =
                        (s.bytes, self.fixed_length(&s.children))
                        && expected != actual
                    {
                        return Err(locate(
                            ProtocolError::new("lengthMismatch", "结构体bytes与子字段合计不符"),
                            &s.location,
                        ));
                    }
                    if s.when.is_none() {
                        available.extend(child);
                        available.insert(format!("len:{child_scope}"));
                    }
                }
                Node::Array(a) => {
                    check_expr(a.when.as_ref(), available, scope)
                        .map_err(|e| locate(e, &a.location))?;
                    check_expr(Some(&a.count), available, scope)
                        .map_err(|e| locate(e, &a.location))?;
                    check_expr(a.element_size.as_ref(), available, scope)
                        .map_err(|e| locate(e, &a.location))?;
                    let child_scope = format!("{scope}.{}[]", a.key);
                    for v in &a.variants {
                        check_expr(Some(&v.selector), available, scope)
                            .map_err(|e| locate(e, &a.location))?;
                        let fragment = self.fragments.get(&v.reference).ok_or_else(|| {
                            locate(
                                ProtocolError::new(
                                    "missingReference",
                                    format!("片段不存在: {}", v.reference),
                                ),
                                &a.location,
                            )
                        })?;
                        self.dependencies(&fragment.nodes, &child_scope, &mut available.clone())?;
                        if let Some(Expr::Number(n)) = a.element_size
                            && self
                                .fixed_length(&fragment.nodes)
                                .is_some_and(|s| s as i128 != n)
                        {
                            return Err(locate(
                                ProtocolError::new("elementSize", "条目固定长度与elementBytes不符"),
                                &a.location,
                            ));
                        }
                    }
                    self.dependencies(&a.children, &child_scope, &mut available.clone())?;
                    if let Some(Expr::Number(n)) = a.element_size
                        && !a.children.is_empty()
                        && self
                            .fixed_length(&a.children)
                            .is_some_and(|s| s as i128 != n)
                    {
                        return Err(locate(
                            ProtocolError::new("elementSize", "条目固定长度与elementBytes不符"),
                            &a.location,
                        ));
                    }
                    if a.when.is_none() {
                        available.insert(format!("len:{scope}.{}", a.key));
                    }
                }
            }
        }
        Ok(())
    }
    fn static_lengths(
        &self,
        nodes: &[Node],
        scope: &str,
        known: &mut BTreeMap<String, i128>,
    ) -> Result<(), ProtocolError> {
        for node in nodes {
            match node {
                Node::Field(f) => {
                    if let Some(bytes) = f.fixed.as_ref().or(f.default.as_ref())
                        && bytes.len() <= 8
                        && matches!(
                            f.value_kind,
                            ValueKind::Uint | ValueKind::Int | ValueKind::Bitset
                        )
                    {
                        let n = match f.endian {
                            ByteOrder::Big => {
                                bytes.iter().fold(0i128, |n, b| (n << 8) | i128::from(*b))
                            }
                            ByteOrder::Little => bytes
                                .iter()
                                .rev()
                                .fold(0i128, |n, b| (n << 8) | i128::from(*b)),
                        };
                        known.insert(f.key.clone(), n);
                        known.insert(format!("{scope}.{}", f.key), n);
                    }
                }
                Node::Use(u) if u.enabled => {
                    self.static_lengths(&self.fragments[&u.reference].nodes, scope, known)?
                }
                Node::Struct(s) => self.static_lengths(
                    &s.children,
                    &format!("{scope}.{}", s.key),
                    &mut known.clone(),
                )?,
                Node::Array(a) => {
                    let expected = a
                        .element_size
                        .as_ref()
                        .and_then(|e| e.evaluate(&|r| known.get(r).copied(), &|_| None).ok());
                    let mut choices = Vec::new();
                    if a.variants.is_empty() {
                        choices.push(a.children.as_slice());
                    } else {
                        for v in &a.variants {
                            choices.push(self.fragments[&v.reference].nodes.as_slice());
                        }
                    }
                    for nodes in choices {
                        if let (Some(expected), Some(actual)) = (expected, self.fixed_length(nodes))
                            && expected != actual as i128
                        {
                            return Err(locate(
                                ProtocolError::new(
                                    "elementSize",
                                    format!("条目声明{expected}字节，但结构为{actual}字节"),
                                ),
                                &a.location,
                            ));
                        }
                        self.static_lengths(nodes, scope, &mut known.clone())?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
    pub(crate) fn fixed_length(&self, nodes: &[Node]) -> Option<usize> {
        let mut length = 0usize;
        for node in nodes {
            length = length.checked_add(match node {
                Node::Field(f) if f.when.is_none() => f.bytes?,
                Node::Struct(s) if s.when.is_none() => self.fixed_length(&s.children)?,
                Node::Use(u) if u.when.is_none() => {
                    if u.enabled {
                        self.fixed_length(&self.fragments.get(&u.reference)?.nodes)?
                    } else {
                        0
                    }
                }
                _ => return None,
            })?;
        }
        Some(length)
    }
    fn route(
        &self,
        nodes: &[Node],
        offset: &mut usize,
        found: &mut Option<usize>,
    ) -> Result<(), ProtocolError> {
        for node in nodes {
            match node {
                Node::Field(f) => {
                    if f.when.is_some() {
                        return Err(locate(
                            ProtocolError::new("messageCode", "基础头必须为无条件固定结构"),
                            &f.location,
                        ));
                    }
                    if f.binding.as_deref() == Some("message.code") || f.key == "msgCode" {
                        if f.bytes != Some(1) || f.when.is_some() {
                            return Err(ProtocolError::new("messageCode", "信令码必须固定1字节"));
                        }
                        if found.replace(*offset).is_some() {
                            return Err(ProtocolError::new("messageCode", "只能有一个信令码字段"));
                        }
                    }
                    *offset = offset
                        .checked_add(f.bytes.ok_or_else(|| {
                            ProtocolError::new("messageCode", "基础头不能包含动态字段")
                        })?)
                        .ok_or_else(|| ProtocolError::new("length", "头长度溢出"))?;
                }
                Node::Use(u) if u.enabled && u.when.is_none() => self.route(
                    &self
                        .fragments
                        .get(&u.reference)
                        .ok_or_else(|| ProtocolError::new("missingReference", "头片段不存在"))?
                        .nodes,
                    offset,
                    found,
                )?,
                Node::Struct(s) if s.when.is_none() => self.route(&s.children, offset, found)?,
                _ => return Err(ProtocolError::new("messageCode", "基础头必须是固定结构")),
            }
        }
        Ok(())
    }
}
fn check_expr(
    expr: Option<&Expr>,
    available: &BTreeSet<String>,
    scope: &str,
) -> Result<(), ProtocolError> {
    if let Some(expr) = expr {
        let mut refs = Vec::new();
        expr.refs(&mut refs);
        for r in refs {
            let (prefix, reference) = r
                .strip_prefix("len:")
                .map(|name| ("len:", name))
                .unwrap_or(("", r.as_str()));
            let mut current = scope;
            let mut found = available.contains(&r);
            while !current.is_empty() && !found {
                found = available.contains(&format!("{prefix}{current}.{reference}"));
                current = current.rsplit_once('.').map(|(head, _)| head).unwrap_or("");
            }
            if !found {
                return Err(ProtocolError::new(
                    "forwardReference",
                    format!("引用必须指向已定义字段: {r}"),
                ));
            }
        }
    }
    Ok(())
}
