//! XML 协议模板解析与导出
//!
//! 模板结构（元素嵌套即层级, 属性即字段定义）:
//!
//! ```xml
//! <simterm-import>
//!   <system name="视频管理协议" description="...">
//!     <group name="设备配置">
//!       <!-- 公共头锚定在分组: 同系统不同分组可各自定义不同的头 -->
//!       <header enabled="true">
//!         <field name="帧头" len="2" enc="hex" value="55 AA"/>
//!       </header>
//!       <!-- 字典也可挂在分组内(仅该分组可见); 根级 dict = 全局共享 -->
//!       <dict key="msg_type" name="消息类型" byte-len="1">
//!         <item value="01" label="注册请求"/>
//!       </dict>
//!       <protocol id="reg_req" name="注册请求" type="Request" auto-respond="reg_resp" delay="100">
//!         <match>
//!           <!-- 字段断言: 全部满足才命中(唯一匹配方式); 值支持 hex/十进制/字典含义 -->
//!           <cond field="操作类型" value="设备配置"/>
//!         </match>
//!         <field name="消息类型" len="1" enc="hex" value="02" header-ref="消息类型"/>
//!         <field name="设备列表" len="12" enc="hex" value="{vlen}" role="device_list">
//!           <unit name="序列号" len="8" enc="hex" dict="device_sn" role="device_sn"/>
//!         </field>
//!         <!-- 或模板模式: <payload format="hex">FF 00 {node_id}</payload> -->
//!       </protocol>
//!     </group>
//!   </system>
//! </simterm-import>
//! ```
//!
//! 字段通用属性: name / len / enc(hex|uint|ascii) / value / dict / role / header-ref;
//! section(header|header_extension|payload|trailer) / section-name 仅描述显示分区。
//! 变长字段({vlen})用 <unit> 子元素声明单条记录布局。
use crate::error::{AppError, AppResult};
use crate::models::{
    DefMode, DictSetMd, FieldEnc, FieldRole, FieldSection, MatchCond, MsgType, PayloadFormat,
    Protocol, ProtocolField, ProtocolMd,
};

/// 分组公共头定义（公共头锚定在分组; 导入时按 系统+分组 名解析并写入）
#[derive(Debug, Clone)]
pub struct GroupHeaderMd {
    pub system: String,
    pub group: String,
    /// 空字符串仅表示 group header-profiles="[]" 的内部空库标记。
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub fields: Vec<ProtocolField>,
}

/// 业务流程步骤(XML 形态; params 用键值对列表, 落库时转对象)
#[derive(Debug, Clone)]
pub struct FlowStepMd {
    pub id: String,
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub protocol_id: String,
    pub terminal_mode: String,
    pub terminal_id: Option<i64>,
    pub params: Vec<(String, String)>,
    pub expect_protocol: Option<String>,
    pub timeout_ms: i64,
    pub delay_ms: i64,
    pub next: Option<String>,
    pub on_success: Option<String>,
    pub on_failure: Option<String>,
    pub on_timeout: Option<String>,
}

/// 业务流程(XML 形态)
#[derive(Debug, Clone)]
pub struct FlowMd {
    pub system: String,
    pub name: String,
    pub description: Option<String>,
    pub steps: Vec<FlowStepMd>,
    pub trigger_protocols: Vec<String>,
    pub schedule_interval_ms: i64,
}

/// XML 解析结果
#[derive(Debug, Clone, Default)]
pub struct ParsedImport {
    pub protocols: Vec<ProtocolMd>,
    pub dicts: Vec<DictSetMd>,
    pub group_headers: Vec<GroupHeaderMd>,
    pub flows: Vec<FlowMd>,
    pub failed: Vec<String>,
}

// ---- 属性解析 ----

fn enc_of(s: &str) -> FieldEnc {
    match s {
        "uint" => FieldEnc::Uint,
        "ascii" => FieldEnc::Ascii,
        _ => FieldEnc::Hex,
    }
}

fn enc_str(e: FieldEnc) -> &'static str {
    match e {
        FieldEnc::Hex => "hex",
        FieldEnc::Uint => "uint",
        FieldEnc::Ascii => "ascii",
    }
}

fn role_of(s: &str) -> FieldRole {
    match s {
        "session" => FieldRole::Session,
        "device_sn" => FieldRole::DeviceSn,
        "device_list" => FieldRole::DeviceList,
        "device_count" => FieldRole::DeviceCount,
        "op_type" => FieldRole::OpType,
        "result" => FieldRole::Result,
        "src_id" => FieldRole::SrcId,
        "dst_id" => FieldRole::DstId,
        "capture" => FieldRole::Capture,
        "type_code" => FieldRole::TypeCode,
        "correlation" => FieldRole::Correlation,
        _ => FieldRole::None,
    }
}

fn role_str(r: FieldRole) -> &'static str {
    match r {
        FieldRole::Session => "session",
        FieldRole::DeviceSn => "device_sn",
        FieldRole::DeviceList => "device_list",
        FieldRole::DeviceCount => "device_count",
        FieldRole::OpType => "op_type",
        FieldRole::Result => "result",
        FieldRole::SrcId => "src_id",
        FieldRole::DstId => "dst_id",
        FieldRole::Capture => "capture",
        FieldRole::TypeCode => "type_code",
        FieldRole::Correlation => "correlation",
        FieldRole::None => "",
    }
}

fn msg_type_of(s: &str) -> MsgType {
    match s {
        "Response" => MsgType::Response,
        "Heartbeat" => MsgType::Heartbeat,
        "Event" => MsgType::Event,
        _ => MsgType::Request,
    }
}

fn msg_type_str(t: MsgType) -> &'static str {
    match t {
        MsgType::Request => "Request",
        MsgType::Response => "Response",
        MsgType::Heartbeat => "Heartbeat",
        MsgType::Event => "Event",
    }
}

// ---- 解析 ----

fn parse_field_el(el: roxmltree::Node) -> Result<ProtocolField, String> {
    let name = el.attribute("name").unwrap_or_default().trim().to_string();
    if name.is_empty() {
        return Err("字段缺少 name 属性".into());
    }
    let len: u32 = el
        .attribute("len")
        .and_then(|s| s.trim().parse().ok())
        .ok_or_else(|| format!("字段 [{name}] 缺少/非法 len"))?;
    let enc = enc_of(el.attribute("enc").unwrap_or("hex"));
    let section = match el.attribute("section").map(str::trim) {
        None | Some("") => None,
        Some("header") => Some(FieldSection::Header),
        Some("header_extension") => Some(FieldSection::HeaderExtension),
        Some("payload") => Some(FieldSection::Payload),
        Some("trailer") => Some(FieldSection::Trailer),
        Some(value) => return Err(format!(
            "字段 [{name}] 的 section [{value}] 非法，需为 header/header_extension/payload/trailer"
        )),
    };
    let section_name = el
        .attribute("section-name")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let value = el.attribute("value").unwrap_or_default().trim().to_string();
    let dict = el
        .attribute("dict")
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "-")
        .map(str::to_string);
    // 显式覆盖公共头字段（未标记的同名字段独立存在于消息体）
    let hdr_ref = el
        .attribute("header-ref")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let role = role_of(el.attribute("role").unwrap_or_default());
    let mut unit_fields = Vec::new();
    for u in el
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "unit")
    {
        match parse_field_el(u) {
            Ok(mut uf) => {
                uf.role = role_of(u.attribute("role").unwrap_or_default());
                unit_fields.push(uf);
            }
            Err(e) => return Err(format!("字段 [{name}] 的子字段: {e}")),
        }
    }
    // 出现条件(分支): 条件字段的实际值等于期望值时本字段才存在
    let when = match (el.attribute("when-field"), el.attribute("when-value")) {
        (Some(f), Some(v)) if !f.trim().is_empty() && !v.trim().is_empty() => {
            Some(crate::models::MatchCond {
                field: f.trim().to_string(),
                value: v.trim().to_string(),
            })
        }
        _ => None,
    };
    Ok(ProtocolField {
        header_key: el
            .attribute("header-key")
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string),
        section,
        section_name,
        profile_key: el
            .attribute("profile-key")
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(str::to_string),
        when,
        name,
        length: len,
        enc,
        value,
        dict,
        unit_fields,
        role,
        hdr_ref,
    })
}

fn parse_protocol_el(
    el: roxmltree::Node,
    system: Option<&str>,
    group: Option<&str>,
) -> Result<ProtocolMd, String> {
    let pid = el.attribute("id").unwrap_or_default().trim().to_string();
    if pid.is_empty() {
        return Err("协议缺少 id 属性".into());
    }
    let name = el.attribute("name").unwrap_or(&pid).trim().to_string();
    let msg_type = msg_type_of(el.attribute("type").unwrap_or("Request"));
    // 匹配已统一为字段条件(<match><cond>), 正则属性不再支持
    let auto_respond = el
        .attribute("auto-respond")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let respond_delay_ms = el
        .attribute("delay")
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    // 触发定时发送: 收到本协议后自动周期发送指定协议(如 注册响应→心跳保活)
    let auto_periodic = el
        .attribute("auto-periodic")
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let auto_periodic_interval_ms = el
        .attribute("auto-periodic-interval")
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    let description = el.attribute("description").map(str::to_string);

    let mut fields = Vec::new();
    let mut payload_template = String::new();
    let mut def_mode = DefMode::Fields;
    let mut format = PayloadFormat::Hex;
    let mut match_fields = Vec::new();
    for c in el.children().filter(|n| n.is_element()) {
        match c.tag_name().name() {
            "field" => fields.push(parse_field_el(c).map_err(|e| format!("[{pid}] {e}"))?),
            "payload" => {
                def_mode = DefMode::Template;
                format = match c.attribute("format").unwrap_or("hex") {
                    "text" => PayloadFormat::Text,
                    _ => PayloadFormat::Hex,
                };
                payload_template = c.text().unwrap_or_default().trim().to_string();
            }
            "match" => {
                // 字段断言条件: <match><cond field="操作类型" value="设备配置"/></match>
                for cond in c
                    .children()
                    .filter(|n| n.is_element() && n.tag_name().name() == "cond")
                {
                    let field = cond
                        .attribute("field")
                        .unwrap_or_default()
                        .trim()
                        .to_string();
                    let value = cond
                        .attribute("value")
                        .unwrap_or_default()
                        .trim()
                        .to_string();
                    if field.is_empty() || value.is_empty() {
                        return Err(format!("[{pid}] <cond> 缺少 field/value 属性"));
                    }
                    match_fields.push(MatchCond { field, value });
                }
            }
            _ => {}
        }
    }
    if def_mode == DefMode::Template && payload_template.is_empty() {
        return Err(format!("[{pid}] 模板模式缺少 <payload> 内容"));
    }
    Ok(ProtocolMd {
        header_refs: parse_header_refs(el.attribute("header-refs"))?,
        name,
        protocol_id: pid,
        format,
        msg_type,
        def_mode,
        fields,
        payload_template,
        match_rule: None,
        match_fields,
        auto_respond,
        respond_delay_ms,
        auto_periodic,
        auto_periodic_interval_ms,
        description,
        system: system.map(str::to_string),
        group: group.map(str::to_string),
    })
}

fn parse_dict_el(
    el: roxmltree::Node,
    system: Option<&str>,
    group: Option<&str>,
) -> Result<DictSetMd, String> {
    let key = el.attribute("key").unwrap_or_default().trim().to_string();
    if key.is_empty() {
        return Err("字典缺少 key 属性".into());
    }
    let name = el.attribute("name").unwrap_or(&key).trim().to_string();
    let mut items = Vec::new();
    for it in el
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "item")
    {
        let v = it.attribute("value").unwrap_or_default().trim().to_string();
        let label = it.attribute("label").unwrap_or_default().trim().to_string();
        if v.is_empty() || label.is_empty() {
            return Err(format!("字典 [{key}] 存在缺少 value/label 的条目"));
        }
        items.push((crate::render::compact_hex(&v).to_uppercase(), label));
    }
    if items.is_empty() {
        return Err(format!("字典 [{key}] 没有任何条目"));
    }
    Ok(DictSetMd {
        key,
        name,
        system: system.map(str::to_string),
        group: group.map(str::to_string),
        items,
    })
}

/// 解析一份 XML 协议模板
fn opt_attr(el: roxmltree::Node, name: &str) -> Option<String> {
    el.attribute(name)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn opt_target(el: roxmltree::Node, name: &str) -> Option<String> {
    opt_attr(el, name)
}

fn valid_header_id(id: &str) -> bool {
    !id.is_empty() && id.trim() == id && !id.contains(',') && !id.contains("::")
}

fn parse_header_refs(value: Option<&str>) -> Result<Option<Vec<String>>, String> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_empty() {
        return Ok(Some(Vec::new()));
    }
    let mut refs = Vec::new();
    for id in value.split(',') {
        if !valid_header_id(id) {
            return Err(format!("header-refs 中公共头 id [{id}] 非法"));
        }
        if refs.iter().any(|previous| previous == id) {
            return Err(format!("header-refs 中公共头引用 [{id}] 重复"));
        }
        refs.push(id.to_string());
    }
    Ok(Some(refs))
}

fn parse_header_el(el: roxmltree::Node, system: &str, group: &str) -> AppResult<GroupHeaderMd> {
    let id = el
        .attribute("id")
        .unwrap_or(crate::headers::LEGACY_HEADER_ID);
    if !valid_header_id(id) {
        return Err(AppError::msg(format!(
            "分组 [{system}/{group}] 公共头 id [{id}] 非法"
        )));
    }
    let name = el.attribute("name").map(str::to_string).unwrap_or_else(|| {
        if id == crate::headers::LEGACY_HEADER_ID {
            format!("{group} 公共头")
        } else {
            id.to_string()
        }
    });
    let enabled = match el.attribute("enabled") {
        None | Some("true") => true,
        Some("false") => false,
        Some(value) => {
            return Err(AppError::msg(format!(
                "公共头 [{id}] enabled [{value}] 非法"
            )))
        }
    };
    let mut fields = Vec::new();
    for field in el
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "field")
    {
        fields
            .push(parse_field_el(field).map_err(|e| AppError::msg(format!("公共头 [{id}]: {e}")))?);
    }
    // Empty legacy headers carried no wire bytes; retain that as an explicit empty library.
    if fields.is_empty() && el.attribute("id").is_none() {
        return Ok(GroupHeaderMd { system: system.into(), group: group.into(), id: String::new(), name: String::new(), enabled: false, fields });
    }
    crate::headers::validate_definitions(&[crate::models::HeaderDefinition {
        id: id.to_string(),
        name: name.clone(),
        fields: fields.clone(),
    }])?;
    Ok(GroupHeaderMd {
        system: system.to_string(),
        group: group.to_string(),
        id: id.to_string(),
        name,
        enabled,
        fields,
    })
}

fn validate_import_headers(out: &ParsedImport) -> AppResult<()> {
    let mut seen = std::collections::HashSet::new();
    let mut empty_groups = std::collections::HashSet::new();
    for header in &out.group_headers {
        if !seen.insert((&header.system, &header.group, &header.id)) {
            return Err(AppError::msg(format!(
                "分组 [{}/{}] 公共头 id [{}] 重复",
                header.system, header.group, header.id
            )));
        }
        if header.id.is_empty() {
            empty_groups.insert((&header.system, &header.group));
        }
    }
    for header in &out.group_headers {
        if !header.id.is_empty() && empty_groups.contains(&(&header.system, &header.group)) {
            return Err(AppError::msg(format!(
                "分组 [{}/{}] header-profiles 空库不能同时定义公共头",
                header.system, header.group
            )));
        }
    }
    for protocol in &out.protocols {
        let Some(refs) = &protocol.header_refs else {
            continue;
        };
        let mut selected = Vec::new();
        for id in refs {
            let header = out
                .group_headers
                .iter()
                .find(|h| {
                    protocol.system.as_deref() == Some(h.system.as_str())
                        && protocol.group.as_deref() == Some(h.group.as_str())
                        && &h.id == id
                })
                .ok_or_else(|| {
                    AppError::msg(format!(
                        "协议 [{}] 公共头引用 [{id}] 不在当前分组中",
                        protocol.protocol_id
                    ))
                })?;
            selected.extend(crate::headers::resolve_definition_fields(&crate::models::HeaderDefinition {
                id: header.id.clone(), name: header.name.clone(), fields: header.fields.clone(),
            }));
        }
        crate::fields::merge_header_specs(&selected, &protocol.fields)?;
    }
    Ok(())
}

/// <flow> = 业务流程(步骤图): 步骤属性 + <param name value> 子元素
fn parse_flow_el(el: roxmltree::Node, sys: &str) -> AppResult<FlowMd> {
    let name = opt_attr(el, "name").ok_or_else(|| AppError::msg("<flow> 缺少 name 属性"))?;
    let mut steps = Vec::new();
    for st in el
        .children()
        .filter(|n| n.is_element() && n.tag_name().name() == "step")
    {
        let id = opt_attr(st, "id")
            .ok_or_else(|| AppError::msg(format!("流程 [{name}] 步骤缺少 id")))?;
        let mut params = Vec::new();
        for pm in st
            .children()
            .filter(|n| n.is_element() && n.tag_name().name() == "param")
        {
            if let (Some(k), Some(v)) = (opt_attr(pm, "name"), opt_attr(pm, "value")) {
                params.push((k, v));
            }
        }
        steps.push(FlowStepMd {
            id,
            name: opt_attr(st, "name").unwrap_or_default(),
            x: st
                .attribute("x")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.0),
            y: st
                .attribute("y")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.0),
            protocol_id: opt_attr(st, "protocol").unwrap_or_default(),
            terminal_mode: st
                .attribute("terminal-mode")
                .unwrap_or("single")
                .to_string(),
            terminal_id: st.attribute("terminal-id").and_then(|v| v.parse().ok()),
            params,
            expect_protocol: opt_attr(st, "expect"),
            timeout_ms: st
                .attribute("timeout-ms")
                .and_then(|v| v.parse().ok())
                .unwrap_or(3000),
            delay_ms: st
                .attribute("delay-ms")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0),
            next: opt_target(st, "next"),
            on_success: opt_target(st, "on-success"),
            on_failure: opt_target(st, "on-failure"),
            on_timeout: opt_target(st, "on-timeout"),
        });
    }
    Ok(FlowMd {
        system: sys.to_string(),
        name,
        description: opt_attr(el, "description"),
        steps,
        trigger_protocols: el
            .attribute("trigger-protocols")
            .map(|v| {
                v.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        schedule_interval_ms: el
            .attribute("schedule-interval-ms")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
    })
}

pub fn parse_protocols_xml(xml: &str) -> AppResult<ParsedImport> {
    let doc =
        roxmltree::Document::parse(xml).map_err(|e| AppError::msg(format!("XML 解析失败: {e}")))?;
    let root = doc.root_element();
    if root.tag_name().name() != "simterm-import" {
        return Err(AppError::msg("根元素须为 <simterm-import>"));
    }
    let mut out = ParsedImport::default();

    let handle_protocol =
        |out: &mut ParsedImport, el: roxmltree::Node, sys: Option<&str>, grp: Option<&str>| {
            match parse_protocol_el(el, sys, grp) {
                Ok(p) => out.protocols.push(p),
                Err(e) => out.failed.push(e),
            }
        };
    for node in root.children().filter(|n| n.is_element()) {
        match node.tag_name().name() {
            "system" => {
                let sys_name = node
                    .attribute("name")
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                if sys_name.is_empty() {
                    out.failed.push("<system> 缺少 name 属性".into());
                    continue;
                }
                // 旧模板把 header 写在 system 级: 兼容为该系统下「没有自有头的分组」的默认头
                let legacy_headers: Vec<_> = node
                    .children()
                    .filter(|n| n.is_element() && n.tag_name().name() == "header")
                    .collect();
                if legacy_headers.len() > 1 {
                    return Err(AppError::msg(format!(
                        "系统 [{sys_name}] 旧公共头重复，请将多公共头定义放入分组"
                    )));
                }
                let legacy_header = legacy_headers.first().copied();
                for c in node.children().filter(|n| n.is_element()) {
                    match c.tag_name().name() {
                        "header" => {}
                        "dict" => match parse_dict_el(c, Some(&sys_name), None) {
                            Ok(d) => out.dicts.push(d),
                            Err(e) => out.failed.push(e),
                        },
                        "group" => {
                            let gname = c.attribute("name").unwrap_or_default().trim().to_string();
                            let gref = if gname.is_empty() {
                                None
                            } else {
                                Some(gname.as_str())
                            };
                            let explicit_empty = match c.attribute("header-profiles") {
                                None => false,
                                Some("[]") => true,
                                Some(value) => return Err(AppError::msg(format!("分组 [{sys_name}/{gname}] header-profiles [{value}] 非法，仅支持 [] 空库标记"))),
                            };
                            let mut own_header = explicit_empty;
                            if explicit_empty {
                                if gref.is_none() {
                                    return Err(AppError::msg("header-profiles 需要具名分组"));
                                }
                                out.group_headers.push(GroupHeaderMd {
                                    system: sys_name.clone(),
                                    group: gname.clone(),
                                    id: String::new(),
                                    name: String::new(),
                                    enabled: false,
                                    fields: Vec::new(),
                                });
                            }
                            for g in c.children().filter(|n| n.is_element()) {
                                match g.tag_name().name() {
                                    "header" => {
                                        own_header = true;
                                        if let Some(gn) = gref {
                                            out.group_headers
                                                .push(parse_header_el(g, &sys_name, gn)?);
                                        } else {
                                            return Err(AppError::msg("公共头定义需要具名分组"));
                                        }
                                    }
                                    "dict" => match parse_dict_el(g, Some(&sys_name), gref) {
                                        Ok(d) => out.dicts.push(d),
                                        Err(e) => out.failed.push(e),
                                    },
                                    "protocol" => {
                                        handle_protocol(&mut out, g, Some(&sys_name), gref)
                                    }
                                    _ => {}
                                }
                            }
                            // 旧模板: 分组无自有头时继承 system 级 header
                            if !own_header && gref.is_some() {
                                if let Some(header) = legacy_header {
                                    out.group_headers
                                        .push(parse_header_el(header, &sys_name, &gname)?);
                                }
                            }
                        }
                        "protocol" => handle_protocol(&mut out, c, Some(&sys_name), None),
                        "flow" => match parse_flow_el(c, &sys_name) {
                            Ok(f) => out.flows.push(f),
                            Err(e) => out.failed.push(e.to_string()),
                        },
                        _ => {}
                    }
                }
            }
            "dict" => match parse_dict_el(node, None, None) {
                Ok(d) => out.dicts.push(d),
                Err(e) => out.failed.push(e),
            },
            "protocol" => handle_protocol(&mut out, node, None, None),
            _ => {}
        }
    }
    validate_import_headers(&out)?;
    if out.protocols.is_empty()
        && out.dicts.is_empty()
        && out.group_headers.is_empty()
        && out.flows.is_empty()
    {
        if !out.failed.is_empty() {
            return Err(AppError::msg(out.failed.join("\n")));
        }
        return Err(AppError::msg(
            "模板中没有协议 / 字典 / 公共头 / 业务流程定义",
        ));
    }
    Ok(out)
}

// ---- 导出 ----

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn field_xml(f: &ProtocolField, indent: &str, tag: &str) -> String {
    let mut line = format!(
        "{indent}<{tag} name=\"{}\" len=\"{}\" enc=\"{}\" value=\"{}\"",
        esc(&f.name),
        f.length,
        enc_str(f.enc),
        esc(&f.value)
    );
    if let Some(d) = f.dict_key() {
        line.push_str(&format!(" dict=\"{}\"", esc(d)));
    }
    if let Some(section) = f.section {
        line.push_str(&format!(" section=\"{}\"", section.as_str()));
    }
    if let Some(name) = &f.section_name {
        line.push_str(&format!(" section-name=\"{}\"", esc(name)));
    }
    if let Some(key) = &f.profile_key {
        if !key.trim().is_empty() {
            line.push_str(&format!(" profile-key=\"{}\"", esc(key.trim())));
        }
    }
    if let Some(key) = &f.header_key {
        if !key.trim().is_empty() {
            line.push_str(&format!(" header-key=\"{}\"", esc(key.trim())));
        }
    }
    if let Some(h) = &f.hdr_ref {
        if !h.trim().is_empty() {
            line.push_str(&format!(" header-ref=\"{}\"", esc(h.trim())));
        }
    }
    if f.role != FieldRole::None {
        line.push_str(&format!(" role=\"{}\"", role_str(f.role)));
    }
    if let Some(w) = &f.when {
        line.push_str(&format!(
            " when-field=\"{}\" when-value=\"{}\"",
            esc(&w.field),
            esc(&w.value)
        ));
    }
    if f.unit_fields.is_empty() {
        line.push_str("/>\n");
        return line;
    }
    line.push_str(">\n");
    for u in &f.unit_fields {
        line.push_str(&field_xml(u, &format!("{indent}  "), "unit"));
    }
    line.push_str(&format!("{indent}</{tag}>\n"));
    line
}

fn dict_xml(d: &DictSetMd, indent: &str) -> String {
    let mut out = format!(
        "{indent}<dict key=\"{}\" name=\"{}\">\n",
        esc(&d.key),
        esc(&d.name)
    );
    for (v, label) in &d.items {
        out.push_str(&format!(
            "{indent}  <item value=\"{}\" label=\"{}\"/>\n",
            esc(v),
            esc(label)
        ));
    }
    out.push_str(&format!("{indent}</dict>\n"));
    out
}

/// 将协议（含其引用的字典与所属分组公共头）导出为 XML 文本。
/// 公共头与分组专属字典都挂在 <group> 内（与解析结构对称）。
pub fn export_protocols_xml(
    protocols: &[Protocol],
    dicts: &[DictSetMd],
    headers: &[GroupHeaderMd],
    flows: &[FlowMd],
) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<simterm-import>\n");

    // 系统段: 分组（公共头 + 分组专属字典 + 协议）
    let mut sys_names: Vec<String> = Vec::new();
    for p in protocols {
        if let Some(s) = &p.system_name {
            if !sys_names.contains(s) {
                sys_names.push(s.clone());
            }
        }
    }
    for header in headers {
        if !sys_names.contains(&header.system) {
            sys_names.push(header.system.clone());
        }
    }
    for s in &sys_names {
        out.push_str(&format!("  <system name=\"{}\">\n", esc(s)));
        let mut groups: Vec<String> = Vec::new();
        for p in protocols
            .iter()
            .filter(|p| p.system_name.as_deref() == Some(s.as_str()))
        {
            let Some(g) = p.group_name.clone() else { continue; };
            if !groups.contains(&g) {
                groups.push(g);
            }
        }
        for header in headers.iter().filter(|header| &header.system == s) {
            if !groups.contains(&header.group) {
                groups.push(header.group.clone());
            }
        }
        for g in &groups {
            let group_headers: Vec<_> = headers
                .iter()
                .filter(|h| &h.system == s && &h.group == g)
                .collect();
            let empty_library = group_headers.iter().any(|h| h.id.is_empty());
            out.push_str(&format!(
                "    <group name=\"{}\"{}>\n",
                esc(g),
                if empty_library {
                    " header-profiles=\"[]\""
                } else {
                    ""
                }
            ));
            for h in group_headers.iter().filter(|h| !h.id.is_empty()) {
                out.push_str(&format!(
                    "      <header id=\"{}\" name=\"{}\" enabled=\"{}\">\n",
                    esc(&h.id),
                    esc(&h.name),
                    h.enabled
                ));
                for f in &h.fields {
                    out.push_str(&field_xml(f, "        ", "field"));
                }
                out.push_str("      </header>\n");
            }
            // 分组专属字典
            for d in dicts.iter().filter(|d| {
                d.system.as_deref() == Some(s.as_str()) && d.group.as_deref() == Some(g.as_str())
            }) {
                out.push_str(&dict_xml(d, "      "));
            }
            for p in protocols.iter().filter(|p| {
                p.system_name.as_deref() == Some(s.as_str())
                    && p.group_name.as_deref() == Some(g.as_str())
            }) {
                out.push_str(&protocol_xml(p, "      "));
            }
            out.push_str("    </group>\n");
        }
        for p in protocols.iter().filter(|p| {
            p.system_name.as_deref() == Some(s.as_str()) && p.group_name.is_none()
        }) {
            out.push_str(&protocol_xml(p, "    "));
        }
        // 系统级(不属于任何分组)的字典 → 挂到各分组? 系统级字典已废除, 仅全局档
        out.push_str("  </system>\n");
    }

    // 全局共享字典
    for d in dicts.iter().filter(|d| d.system.is_none()) {
        out.push_str(&dict_xml(d, "  "));
    }

    // 未归属系统的协议
    for p in protocols.iter().filter(|p| p.system_name.is_none()) {
        out.push_str(&protocol_xml(p, "  "));
    }

    out.push_str("</simterm-import>\n");
    out
}

fn flow_xml(f: &FlowMd, indent: &str) -> String {
    let mut head = format!("{indent}<flow name=\"{}\"", esc(&f.name));
    if let Some(d) = &f.description {
        head.push_str(&format!(" description=\"{}\"", esc(d)));
    }
    if !f.trigger_protocols.is_empty() {
        head.push_str(&format!(
            " trigger-protocols=\"{}\"",
            esc(&f.trigger_protocols.join(","))
        ));
    }
    if f.schedule_interval_ms > 0 {
        head.push_str(&format!(
            " schedule-interval-ms=\"{}\"",
            f.schedule_interval_ms
        ));
    }
    let mut out = format!(
        "{head}>
"
    );
    for st in &f.steps {
        let mut line = format!(
            "{indent}  <step id=\"{}\" name=\"{}\" protocol=\"{}\"",
            esc(&st.id),
            esc(&st.name),
            esc(&st.protocol_id)
        );
        if st.terminal_mode != "single" {
            line.push_str(&format!(" terminal-mode=\"{}\"", esc(&st.terminal_mode)));
        }
        if let Some(t) = st.terminal_id {
            line.push_str(&format!(" terminal-id=\"{t}\""));
        }
        if let Some(e) = &st.expect_protocol {
            line.push_str(&format!(" expect=\"{}\"", esc(e)));
        }
        if st.timeout_ms != 3000 {
            line.push_str(&format!(" timeout-ms=\"{}\"", st.timeout_ms));
        }
        if st.delay_ms > 0 {
            line.push_str(&format!(" delay-ms=\"{}\"", st.delay_ms));
        }
        for (k, t) in [
            ("next", &st.next),
            ("on-success", &st.on_success),
            ("on-failure", &st.on_failure),
            ("on-timeout", &st.on_timeout),
        ] {
            if let Some(v) = t {
                line.push_str(&format!(" {k}=\"{}\"", esc(v)));
            }
        }
        if st.params.is_empty() {
            out.push_str(&format!(
                "{line}/>
"
            ));
        } else {
            out.push_str(&format!(
                "{line}>
"
            ));
            for (k, v) in &st.params {
                out.push_str(&format!(
                    "{indent}    <param name=\"{}\" value=\"{}\"/>
",
                    esc(k),
                    esc(v)
                ));
            }
            out.push_str(&format!(
                "{indent}  </step>
"
            ));
        }
    }
    out.push_str(&format!(
        "{indent}</flow>
"
    ));
    out
}

fn protocol_xml(p: &Protocol, indent: &str) -> String {
    let mut line = format!(
        "{indent}<protocol id=\"{}\" name=\"{}\" type=\"{}\"",
        esc(&p.protocol_id),
        esc(&p.name),
        msg_type_str(p.msg_type)
    );
    if let Some(refs) = p
        .header_refs_json
        .as_deref()
        .and_then(|json| serde_json::from_str::<Option<Vec<String>>>(json).ok())
        .flatten()
    {
        line.push_str(&format!(" header-refs=\"{}\"", esc(&refs.join(","))));
    }
    if let Some(a) = &p.auto_respond {
        if !a.is_empty() {
            line.push_str(&format!(" auto-respond=\"{}\"", esc(a)));
        }
    }
    if let Some(t) = &p.auto_periodic {
        if !t.is_empty() {
            line.push_str(&format!(
                " auto-periodic=\"{}\" auto-periodic-interval=\"{}\"",
                esc(t),
                if p.auto_periodic_interval_ms > 0 {
                    p.auto_periodic_interval_ms
                } else {
                    30000
                }
            ));
        }
    }
    if p.respond_delay_ms > 0 {
        line.push_str(&format!(" delay=\"{}\"", p.respond_delay_ms));
    }
    if let Some(d) = &p.description {
        if !d.is_empty() {
            line.push_str(&format!(" description=\"{}\"", esc(d)));
        }
    }
    line.push_str(">\n");
    let conds = p.match_conds();
    if !conds.is_empty() {
        line.push_str(&format!("{indent}  <match>\n"));
        for c in &conds {
            line.push_str(&format!(
                "{indent}    <cond field=\"{}\" value=\"{}\"/>\n",
                esc(&c.field),
                esc(&c.value)
            ));
        }
        line.push_str(&format!("{indent}  </match>\n"));
    }
    if p.def_mode == DefMode::Fields {
        for f in p.field_specs() {
            line.push_str(&field_xml(&f, &format!("{indent}  "), "field"));
        }
    } else {
        let fmt = match p.format {
            PayloadFormat::Text => "text",
            PayloadFormat::Json => "json",
            PayloadFormat::Hex => "hex",
        };
        line.push_str(&format!(
            "{indent}  <payload format=\"{}\">{}</payload>\n",
            fmt,
            esc(&p.payload_template)
        ));
    }
    line.push_str(&format!("{indent}</protocol>\n"));
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    const TPL: &str = include_str!("../../../protocols/UDCP_1.0.xml");

    #[test]
    fn udcp_xml_template_parses() {
        let r = parse_protocols_xml(TPL).expect("UDCP XML 模板解析失败");
        assert!(r.failed.is_empty(), "失败段: {:?}", r.failed);
        assert_eq!(r.protocols.len(), 13, "13 个业务报文");
        // 旧模板 header 在 system 级: 兼容展开到该系统全部 4 个分组
        assert_eq!(r.group_headers.len(), 4, "4 个分组各一份公共头");
        let h = &r.group_headers[0];
        assert_eq!(h.system, "UDCP");
        assert_eq!(h.fields.len(), 10, "18B 公共头共 10 个字段");
        assert_eq!(h.fields.iter().map(|f| f.length).sum::<u32>(), 18);
        assert!(h.fields.iter().all(|f| f.enc == FieldEnc::Hex));
        assert!(r.dicts.len() >= 8, "字典, 实际 {}", r.dicts.len());

        // 语义标记: 公共头 源/目标/Session
        let role_of =
            |fs: &[ProtocolField], name: &str| fs.iter().find(|f| f.name == name).unwrap().role;
        assert_eq!(role_of(&h.fields, "源设备ID"), FieldRole::SrcId);
        assert_eq!(role_of(&h.fields, "目标设备ID"), FieldRole::DstId);
        assert_eq!(role_of(&h.fields, "Session"), FieldRole::Session);
        assert_eq!(role_of(&h.fields, "帧头"), FieldRole::None);

        // 应答链
        let chain = |id: &str| {
            r.protocols
                .iter()
                .find(|p| p.protocol_id == id)
                .unwrap()
                .auto_respond
                .clone()
        };
        assert_eq!(chain("udcp_reg_req").as_deref(), Some("udcp_reg_resp"));
        assert_eq!(
            chain("udcp_devinfo_req").as_deref(),
            Some("udcp_devinfo_resp")
        );

        // 设备配置请求: 语义完整 + 变长子字段布局
        let dev = r
            .protocols
            .iter()
            .find(|p| p.protocol_id == "udcp_devinfo_req")
            .unwrap();
        assert_eq!(role_of(&dev.fields, "操作类型"), FieldRole::OpType);
        assert_eq!(role_of(&dev.fields, "设备数量"), FieldRole::DeviceCount);
        let list = dev.fields.iter().find(|f| f.name == "设备列表").unwrap();
        assert_eq!(list.role, FieldRole::DeviceList);
        assert_eq!(list.value, "{vlen}");
        assert_eq!(list.unit_fields.len(), 3);
        assert_eq!(list.unit_fields[0].role, FieldRole::DeviceSn);
        assert_eq!(list.unit_fields[0].length, 8);
        // 应答: 结果/数量/操作类型语义
        let resp = r
            .protocols
            .iter()
            .find(|p| p.protocol_id == "udcp_devinfo_resp")
            .unwrap();
        assert_eq!(role_of(&resp.fields, "应答结果"), FieldRole::Result);
        assert_eq!(role_of(&resp.fields, "配置数量"), FieldRole::DeviceCount);
        assert_eq!(role_of(&resp.fields, "操作类型"), FieldRole::OpType);

        // 消息体长度: 公共头 18 + 消息体 = 协议定义总长
        let header_names: Vec<&str> = h.fields.iter().map(|f| f.name.as_str()).collect();
        let body_len = |id: &str| -> u32 {
            r.protocols
                .iter()
                .find(|p| p.protocol_id == id)
                .unwrap()
                .fields
                .iter()
                .filter(|f| !header_names.contains(&f.name.as_str()))
                .map(|f| f.length)
                .sum()
        };
        assert_eq!(18 + body_len("udcp_reg_req"), 32);
        assert_eq!(18 + body_len("udcp_reg_resp"), 24);
        assert_eq!(18 + body_len("udcp_devinfo_req"), 33, "单台: 18+3+12");
        assert_eq!(18 + body_len("udcp_devinfo_resp"), 23);
    }

    #[test]
    fn export_round_trip() {
        let r = parse_protocols_xml(TPL).unwrap();
        let dicts: Vec<DictSetMd> = r.dicts.clone();
        let headers = r.group_headers.clone();
        // 用解析结果构造导出输入(借用 Protocol 的字段), 再解析回来比对关键信息
        let xml = export_protocols_xml(&fake_rows(&r.protocols), &dicts, &headers, &r.flows);
        let back = parse_protocols_xml(&xml).unwrap();
        assert!(back.failed.is_empty(), "{:?}", back.failed);
        assert_eq!(back.protocols.len(), r.protocols.len());
        for (a, b) in r.protocols.iter().zip(back.protocols.iter()) {
            assert_eq!(a.protocol_id, b.protocol_id);
            assert_eq!(a.fields.len(), b.fields.len());
            assert_eq!(a.auto_respond, b.auto_respond);
        }
        // 语义标记 round-trip 保留
        let dev = back
            .protocols
            .iter()
            .find(|p| p.protocol_id == "udcp_devinfo_req")
            .unwrap();
        let list = dev.fields.iter().find(|f| f.name == "设备列表").unwrap();
        assert_eq!(list.role, FieldRole::DeviceList);
        assert_eq!(list.unit_fields[0].role, FieldRole::DeviceSn);
        let h = &back.group_headers[0];
        assert_eq!(
            h.fields.iter().find(|f| f.name == "Session").unwrap().role,
            FieldRole::Session
        );
    }

    #[test]
    fn group_level_header_dict_and_conds_parse() {
        // 新结构: header/dict 挂 group 内; 协议字段 header-ref 显式覆盖; <match> 字段断言
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<simterm-import>
  <system name="测试系统">
    <group name="配置">
      <header enabled="true">
        <field name="帧头" len="2" enc="hex" value="55 AA"/>
        <field name="消息类型" len="1" enc="hex" value="01" dict="mgmt_op"/>
      </header>
      <dict key="mgmt_op" name="操作类型">
        <item value="11" label="设备配置"/>
      </dict>
      <protocol id="cfg_req" name="配置请求" type="Request" auto-respond="cfg_resp">
        <match><cond field="消息类型" value="设备配置"/></match>
        <field name="类型" len="1" enc="hex" value="11" header-ref="消息类型"/>
        <field name="消息类型" len="2" enc="hex" value="AB CD"/>
      </protocol>
    </group>
  </system>
</simterm-import>"#;
        let r = parse_protocols_xml(xml).unwrap();
        assert!(r.failed.is_empty(), "{:?}", r.failed);
        assert_eq!(r.group_headers.len(), 1);
        assert_eq!(r.group_headers[0].group, "配置");
        // dict 挂 group 内
        let d = &r.dicts[0];
        assert_eq!(d.system.as_deref(), Some("测试系统"));
        assert_eq!(d.group.as_deref(), Some("配置"));
        // 协议: 断言条件 + header-ref + 同名独立字段
        let p = &r.protocols[0];
        assert_eq!(p.match_fields.len(), 1);
        assert_eq!(p.match_fields[0].field, "消息类型");
        assert_eq!(p.match_fields[0].value, "设备配置");
        let over = p.fields.iter().find(|f| f.name == "类型").unwrap();
        assert_eq!(over.hdr_ref.as_deref(), Some("消息类型"));
        let indep = p.fields.iter().find(|f| f.name == "消息类型").unwrap();
        assert!(indep.hdr_ref.is_none(), "同名字段未标记则独立");
        assert_eq!(indep.length, 2);
    }

    /// 把解析出的 ProtocolMd 映射为 db 行视图供导出函数使用
    fn fake_rows(md: &[ProtocolMd]) -> Vec<Protocol> {
        md.iter()
            .map(|p| Protocol {
                header_refs_json: p
                    .header_refs
                    .as_ref()
                    .map(|refs| serde_json::to_string(refs).unwrap()),
                id: 0,
                protocol_id: p.protocol_id.clone(),
                name: p.name.clone(),
                format: p.format,
                msg_type: p.msg_type,
                def_mode: p.def_mode,
                fields_json: Some(serde_json::to_string(&p.fields).unwrap()),
                payload_template: p.payload_template.clone(),
                match_rule: p.match_rule.clone(),
                match_fields_json: if p.match_fields.is_empty() {
                    None
                } else {
                    Some(serde_json::to_string(&p.match_fields).unwrap())
                },
                auto_respond: p.auto_respond.clone(),
                respond_delay_ms: p.respond_delay_ms,
                auto_periodic: p.auto_periodic.clone(),
                auto_periodic_interval_ms: p.auto_periodic_interval_ms,
                description: p.description.clone(),
                system_ref: None,
                group_ref: None,
                system_name: Some("UDCP".into()),
                group_name: p.group.clone(),
                created_at: String::new(),
                updated_at: String::new(),
            })
            .collect()
    }
}
