//! 规则引擎：收包 → 协议命中。
//!
//! 两阶段匹配：
//!   1. 正则阶段：协议带匹配正则时对 hex 文本（空格分隔 / 紧凑两种写法）命中即为候选；
//!      无正则的协议直接成为候选（由字段断言把关）。
//!   2. 字段断言阶段（[`check_match_conds`]）：按协议帧布局（分组公共头 + 消息体字段）
//!      解析收包，逐条比对「字段 = 期望值」断言；期望值可写 hex 字面量、十进制、
//!      文本或字典条目含义（绑字典字段）。

use dashmap::DashMap;
use regex::Regex;

use crate::fields::{dedupe_field_names, extract_field_values, is_variable_len};
use crate::models::{FieldEnc, FieldRole, MatchCond, Protocol, ProtocolField};
use crate::render::DictMap;

/// 规范 hex 串：大写、单空格分隔，如 "FF 00 01 0A"
pub fn normalize_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ")
}

/// 紧凑 hex 串：无分隔，如 "FF00010A"
pub fn compact_hex(bytes: &[u8]) -> String {
    hex::encode_upper(bytes)
}

/// 正则缓存（规则串 → 编译结果; 编译失败的规则会被跳过）
pub struct RegexCache {
    inner: DashMap<String, Option<Regex>>,
}

impl RegexCache {
    pub fn new() -> Self {
        Self {
            inner: DashMap::new(),
        }
    }

    pub fn get(&self, rule: &str) -> Option<regex::Regex> {
        if let Some(hit) = self.inner.get(rule) {
            return hit.clone();
        }
        let compiled = Regex::new(rule).ok();
        self.inner.insert(rule.to_string(), compiled.clone());
        compiled
    }
}

impl Default for RegexCache {
    fn default() -> Self {
        Self::new()
    }
}

/// 正则阶段的候选列表（保持传入顺序 = 协议 id 顺序）。
///
/// - 带正则的协议：正则命中（对空格分隔或紧凑 hex 文本任一）才成为候选；
/// - 无正则的协议：直接成为候选（既无正则又无字段断言的协议跳过, 不参与匹配）。
///
/// 候选还需经 [`check_match_conds`] 字段断言验证后才算最终命中。
pub fn match_candidates<'p>(
    cache: &RegexCache,
    bytes: &[u8],
    protocols: &'p [Protocol],
) -> Vec<&'p Protocol> {
    let spaced = normalize_hex(bytes);
    let compact = compact_hex(bytes);
    let mut out = Vec::new();
    for p in protocols {
        let rule = p.match_rule.as_deref().map(str::trim).filter(|s| !s.is_empty());
        match rule {
            Some(rule) => {
                let Some(re) = cache.get(rule) else {
                    continue; // 非法正则, 跳过
                };
                if re.is_match(&spaced) || re.is_match(&compact) {
                    out.push(p);
                }
            }
            None => {
                // 纯字段断言协议, 或带信令类型码(role=type_code)的协议 ——
                // 后者的识别条件由 effective_conds 从字段声明自动生成
                let has_type_code = p
                    .field_specs()
                    .iter()
                    .any(|f| f.role == FieldRole::TypeCode);
                if !p.match_conds().is_empty() || has_type_code {
                    out.push(p);
                }
            }
        }
    }
    out
}

/// 字段断言阶段：全部条件满足返回 true。
///
/// `frame_fields` 为协议整帧字段表（分组公共头 + 消息体, 未去重）;
/// 重名字段自动按 `#2/#3` 唯一键区分, 条件的 field 可写 `序号` / `序号#2`。
pub fn check_match_conds(
    frame_fields: &[ProtocolField],
    dicts: &DictMap,
    conds: &[MatchCond],
    bytes: &[u8],
) -> bool {
    if conds.is_empty() {
        return true;
    }
    let mut fields = frame_fields.to_vec();
    dedupe_field_names(&mut fields);
    let values = extract_field_values(&fields, bytes, dicts);
    for c in conds {
        let Some(actual) = values.get(c.field.trim()) else {
            return false; // 收包里定位不到该字段（布局外或残缺）
        };
        let f = fields.iter().find(|f| f.name == c.field.trim());
        if !cond_value_matches(&c.value, actual, f, dicts) {
            return false;
        }
    }
    true
}

/// 帧长与布局是否一致: 定长布局的总字节数等于报文长度才算一致;
/// 含变长字段({rlen}/{vlen})的布局长度随报文而定, 恒视为一致。
/// 多个候选同时满足断言时, 优先取帧长一致的, 避免短布局截断误吞长帧。
/// 信令类型码(role=type_code)自动生成的匹配条件: 该字段实际值 == 定义值。
/// 协议身份码进数据: 每个业务指令的类型码字段标上「信令类型」语义即可,
/// 引擎自动加「字段=定义值」断言(显式手写的同名字段断言优先, 不重复)。
pub fn effective_conds(
    frame_fields: &[ProtocolField],
    explicit: &[MatchCond],
) -> Vec<MatchCond> {
    let mut out = explicit.to_vec();
    for f in frame_fields.iter().filter(|f| f.role == FieldRole::TypeCode) {
        let v = f.value.trim();
        if v.is_empty() {
            continue; // 未填定义值: 无从断言(按其余条件识别)
        }
        if explicit.iter().any(|c| c.field.trim() == f.name) {
            continue; // 手写断言优先
        }
        out.push(MatchCond {
            field: f.name.clone(),
            value: v.to_string(),
        });
    }
    out
}

pub fn frame_len_matches(frame_fields: &[ProtocolField], bytes: &[u8]) -> bool {
    if frame_fields.iter().any(is_variable_len) || frame_fields.iter().any(|f| f.when.is_some()) {
        return true; // 变长/分支布局: 长度随报文而定, 识别交给字段断言
    }
    frame_fields.iter().map(|f| f.length as usize).sum::<usize>() == bytes.len()
}

/// 单条断言的期望值与实际提取值比对。
///
/// - 期望 `*`（任意）: 字段已解析存在即满足; 绑字典时还必须是字典内条目
///   （用于区分同长度但取值域不同的帧, 如 呼叫的业务类型 vs 挂断的时长尾字节）;
/// - 绑字典字段：期望可为条目 hex 或条目含义, 归一到紧凑 hex 比对;
/// - uint 字段：十进制 / 0x 前缀按数值比对;
/// - 其余（hex/ascii）：hex 归一比对, 失败再按原文（ascii 文本）比对。
pub(crate) fn cond_value_matches(
    expect: &str,
    actual: &str,
    f: Option<&ProtocolField>,
    dicts: &DictMap,
) -> bool {
    let e = expect.trim();
    if e.is_empty() {
        return false;
    }
    let actual_compact = actual.replace(' ', "").to_uppercase();
    if e == "*" {
        return match f.and_then(|f| f.dict_key()).and_then(|k| dicts.get(k)) {
            Some(set) => set.label_of(&actual_compact).is_some(),
            None => true,
        };
    }
    if let (Some(_f), Some(key)) = (f, f.and_then(|f| f.dict_key())) {
        if let Some(set) = dicts.get(key) {
            return match set.find_value(e) {
                Some(item_hex) => item_hex.replace(' ', "").to_uppercase() == actual_compact,
                None => e.replace(' ', "").to_uppercase() == actual_compact,
            };
        }
    }
    match f.map(|f| f.enc) {
        Some(FieldEnc::Uint) => {
            let ev = if let Some(h) = e.strip_prefix("0x").or_else(|| e.strip_prefix("0X")) {
                u128::from_str_radix(h, 16).ok()
            } else {
                e.parse::<u128>().ok()
            };
            match (ev, actual.parse::<u128>().ok()) {
                (Some(e), Some(a)) => e == a,
                _ => false,
            }
        }
        _ => {
            e.replace(' ', "").to_uppercase() == actual_compact || e == actual
        }
    }
}

/// 校验规则串可编译为正则（导入/保存协议时调用）
pub fn validate_rule(rule: &str) -> Result<(), String> {
    Regex::new(rule).map(|_| ()).map_err(|e| format!("Match 规则不是合法正则: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{MsgType, PayloadFormat};

    fn proto(pid: &str, rule: &str) -> Protocol {
        Protocol {
            header_refs_json: None,
            id: 1,
            protocol_id: pid.into(),
            name: pid.into(),
            format: PayloadFormat::Hex,
            msg_type: MsgType::Request,
            def_mode: crate::models::DefMode::Template,
            fields_json: None,
            match_fields_json: None,
            system_ref: None,
            group_ref: None,
            group_name: None,
            system_name: None,
            payload_template: "FF".into(),
            match_rule: Some(rule.into()),
            auto_respond: None,
            auto_periodic: None,
            auto_periodic_interval_ms: 0,
            respond_delay_ms: 0,
            description: None,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn matches_spaced_rule() {
        let cache = RegexCache::new();
        let ps = vec![proto("a", "^FF 00 01"), proto("b", "^EE")];
        let hits = match_candidates(&cache, &[0xFF, 0x00, 0x01, 0xAB], &ps);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].protocol_id, "a");
    }

    #[test]
    fn matches_compact_rule() {
        let cache = RegexCache::new();
        let ps = vec![proto("a", "^FF0001")];
        let hits = match_candidates(&cache, &[0xFF, 0x00, 0x01], &ps);
        assert_eq!(hits[0].protocol_id, "a");
    }

    #[test]
    fn order_preserved_and_regexless_needs_conds() {
        let cache = RegexCache::new();
        // 无正则无断言 → 不参与; 有断言 → 候选
        let mut plain = proto("plain", "");
        plain.match_rule = None;
        let mut cond_only = proto("cond_only", "");
        cond_only.match_rule = None;
        cond_only.match_fields_json = Some(r#"[{"field":"操作类型","value":"11"}]"#.into());
        let ps = vec![plain, cond_only];
        let hits = match_candidates(&cache, &[0xFF], &ps);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].protocol_id, "cond_only");
    }

    #[test]
    fn invalid_regex_is_skipped() {
        let cache = RegexCache::new();
        let ps = vec![proto("bad", "^FF ("), proto("good", "^FF")];
        let hits = match_candidates(&cache, &[0xFF], &ps);
        assert_eq!(hits[0].protocol_id, "good");
    }

    #[test]
    fn conds_match_by_dict_label_and_hex() {
        // 帧布局: 操作类型(1B, 字典) + 配置类型(1B)
        let mut d = DictMap::new();
        d.insert(crate::render::DictSetLite {
            key: "mgmt_op".into(),
            name: "操作类型".into(),
            byte_len: 1,
            items: vec![("11".into(), "设备配置".into()), ("22".into(), "网关配置".into())],
        });
        let fields = vec![
            ProtocolField {
                name: "操作类型".into(),
                length: 1,
                enc: FieldEnc::Hex,
                value: "11".into(),
                dict: Some("mgmt_op".into()),
                unit_fields: Vec::new(),
                role: Default::default(),
                hdr_ref: None,
            header_key: None, profile_key: None, section: None, section_name: None,
                when: None,
            },
            ProtocolField {
                name: "配置类型".into(),
                length: 1,
                enc: FieldEnc::Hex,
                value: "01".into(),
                dict: None,
                unit_fields: Vec::new(),
                role: Default::default(),
                hdr_ref: None,
            header_key: None, profile_key: None, section: None, section_name: None,
                when: None,
            },
        ];
        let data = [0x11u8, 0x01];
        // 字典含义写法
        let conds = vec![MatchCond { field: "操作类型".into(), value: "设备配置".into() }];
        assert!(check_match_conds(&fields, &d, &conds, &data));
        // 字典 hex 写法 + 不匹配的
        let conds = vec![MatchCond { field: "操作类型".into(), value: "22".into() }];
        assert!(!check_match_conds(&fields, &d, &conds, &data));
        // 多条件 AND: hex 字段 + uint 字段
        let mut fields2 = fields.clone();
        fields2.push(ProtocolField {
            name: "数量".into(),
            length: 2,
            enc: FieldEnc::Uint,
            value: "0".into(),
            dict: None,
            unit_fields: Vec::new(),
            role: Default::default(),
            hdr_ref: None,
            header_key: None, profile_key: None, section: None, section_name: None,
                when: None,
        });
        let data2 = [0x11u8, 0x01, 0x00, 0x03];
        let conds = vec![
            MatchCond { field: "操作类型".into(), value: "11".into() },
            MatchCond { field: "数量".into(), value: "3".into() },
        ];
        assert!(check_match_conds(&fields2, &d, &conds, &data2));
        let conds_bad = vec![MatchCond { field: "数量".into(), value: "5".into() }];
        assert!(!check_match_conds(&fields2, &d, &conds_bad, &data2));
        // 字段在布局外 / 残缺收包 → 不命中
        let conds = vec![MatchCond { field: "不存在的字段".into(), value: "01".into() }];
        assert!(!check_match_conds(&fields2, &d, &conds, &data2));
    }

    #[test]
    fn any_entry_cond_matches_dict_member_only() {
        // 期望 * : 绑字典字段必须是字典内条目, 其余字段存在即满足
        let mut d = DictMap::new();
        d.insert(crate::render::DictSetLite {
            key: "biz".into(),
            name: "业务类型".into(),
            byte_len: 1,
            items: vec![
                ("01".into(), "视频通信".into()),
                ("02".into(), "语音通信".into()),
                ("03".into(), "会议邀请".into()),
            ],
        });
        let fields = vec![ProtocolField {
            name: "业务类型".into(),
            length: 1,
            enc: FieldEnc::Hex,
            value: "01".into(),
            dict: Some("biz".into()),
            unit_fields: Vec::new(),
            role: Default::default(),
            hdr_ref: None,
            header_key: None, profile_key: None, section: None, section_name: None,
                when: None,
        }];
        let conds = vec![MatchCond { field: "业务类型".into(), value: "*".into() }];
        // 字典内条目 → 命中
        assert!(check_match_conds(&fields, &d, &conds, &[0x02]));
        // 字典外的值（如挂断帧时长尾字节 1E）→ 不命中, 同长帧得以区分
        assert!(!check_match_conds(&fields, &d, &conds, &[0x1E]));
        // 非字典字段: 存在即满足
        let plain = vec![ProtocolField {
            name: "记录条数".into(),
            length: 2,
            enc: FieldEnc::Uint,
            value: "0".into(),
            dict: None,
            unit_fields: Vec::new(),
            role: Default::default(),
            hdr_ref: None,
            header_key: None, profile_key: None, section: None, section_name: None,
                when: None,
        }];
        let conds2 = vec![MatchCond { field: "记录条数".into(), value: "*".into() }];
        assert!(check_match_conds(&plain, &d, &conds2, &[0x00, 0x03]));
    }

    /// 信令类型码: 标了 role=type_code 的字段自动成为匹配条件, 收包按码识别
    #[test]
    fn type_code_field_becomes_implicit_cond() {
        let mk = |name: &str, len: u32, value: &str, role: FieldRole| ProtocolField {
            name: name.into(),
            length: len,
            enc: FieldEnc::Hex,
            value: value.into(),
            dict: None,
            unit_fields: Vec::new(),
            role,
            hdr_ref: None,
            header_key: None, profile_key: None, section: None, section_name: None,
                when: None,
        };
        // 帧布局: 信令类型(1B, 码=05) + 数据(1B); 无手写断言
        let frame = vec![
            mk("信令类型", 1, "05", FieldRole::TypeCode),
            mk("数据", 1, "00", FieldRole::None),
        ];
        let conds = effective_conds(&frame, &[]);
        assert_eq!(conds.len(), 1, "自动生成 信令类型=05 断言");
        assert_eq!(conds[0].field, "信令类型");
        assert_eq!(conds[0].value, "05");
        // 码相符 → 命中
        assert!(check_match_conds(&frame, &DictMap::new(), &conds, &[0x05, 0xAA]));
        // 码不符(别的指令) → 不命中
        assert!(!check_match_conds(&frame, &DictMap::new(), &conds, &[0x06, 0xAA]));
        // 手写同名字段断言优先, 不重复生成
        let manual = vec![MatchCond { field: "信令类型".into(), value: "07".into() }];
        let conds2 = effective_conds(&frame, &manual);
        assert_eq!(conds2.len(), 1);
        assert_eq!(conds2[0].value, "07");
        // 未填定义值 → 不生成(无从断言)
        let frame2 = vec![mk("信令类型", 1, "", FieldRole::TypeCode)];
        assert!(effective_conds(&frame2, &[]).is_empty());
    }

    #[test]
    fn frame_len_matches_prefers_exact_layout() {
        let mk = |name: &str, len: u32, value: &str| ProtocolField {
            name: name.into(),
            length: len,
            enc: FieldEnc::Hex,
            value: value.into(),
            dict: None,
            unit_fields: Vec::new(),
            role: Default::default(),
            hdr_ref: None,
            header_key: None, profile_key: None, section: None, section_name: None,
                when: None,
        };
        // 8B 布局: 与 8B 报文一致, 与 10B 报文（如媒体协商帧）不一致
        let ring = vec![mk("对端", 3, "0"), mk("时间", 4, "0"), mk("业务", 1, "0")];
        assert!(frame_len_matches(&ring, &[0u8; 8]));
        assert!(!frame_len_matches(&ring, &[0u8; 10]));
        // 含 {vlen} 变长字段的布局: 任意长度都视为一致
        let var = vec![mk("条数", 2, "0"), mk("记录", 14, "{vlen}")];
        assert!(frame_len_matches(&var, &[0u8; 5]));
        assert!(frame_len_matches(&var, &[0u8; 57]));
    }

    /// 视频呼叫链消歧: 同组 8B 呼叫/挂断、10B 媒体协商帧与服务端真实帧序对齐,
    /// 验证「断言全过 + 帧长一致优先」选出正确协议（帧布局与分组头取自 VIDEO 模板）
    #[test]
    fn video_call_chain_frames_disambiguate() {
        let mk = |name: &str, len: u32, value: &str, dict: Option<&str>| ProtocolField {
            name: name.into(),
            length: len,
            enc: FieldEnc::Hex,
            value: value.into(),
            dict: dict.map(str::to_string),
            unit_fields: Vec::new(),
            role: Default::default(),
            hdr_ref: None,
            header_key: None, profile_key: None, section: None, section_name: None,
                when: None,
        };
        let set = |key: &str, items: &[(&str, &str)]| crate::render::DictSetLite {
            key: key.into(),
            name: key.into(),
            byte_len: 1,
            items: items
                .iter()
                .map(|(v, l)| ((*v).to_string(), (*l).to_string()))
                .collect(),
        };
        let mut d = DictMap::new();
        d.insert(set("biz_type", &[("01", "视频通信"), ("02", "语音通信"), ("03", "会议邀请")]));
        d.insert(set("bye_reason", &[("00", "正常挂断"), ("01", "超时挂断"), ("02", "故障挂断")]));
        d.insert(set("video_codec", &[("01", "H.264"), ("02", "H.265")]));
        d.insert(set("resp_result", &[("AA", "成功"), ("01", "失败")]));

        // 设备面 12B 公共头布局(IP4+端口2+掩码1+设备编号3+token2) + 帧字节构造
        let hdr_layout = vec![
            mk("IP地址", 4, "0", None),
            mk("端口", 2, "0", None),
            mk("掩码", 1, "0", None),
            mk("设备编号", 3, "0", None),
            mk("token", 2, "0", None),
        ];
        let hdr = |d1: u8, d2: u8, d3: u8, t1: u8, t2: u8| {
            vec![0xC0, 0xA8, 0x00, 0x01, 0x13, 0x88, 0x18, d1, d2, d3, t1, t2]
        };
        // 视频通信组收包方向整帧布局 = 公共头 + 体字段（与引擎 frame_specs 拼接一致）
        let frame_of = |body: &[ProtocolField]| -> Vec<ProtocolField> {
            let mut f = hdr_layout.clone();
            f.extend_from_slice(body);
            f
        };
        // 视频通信组收包方向布局（对端3 + ...）与断言, 与 XML 一致
        let ring_layout = frame_of(&[
            mk("对端设备编号", 3, "0", None),
            mk("呼叫时间", 4, "0", None),
            mk("业务类型", 1, "01", Some("biz_type")),
        ]);
        let bye_layout = frame_of(&[
            mk("对端设备编号", 3, "0", None),
            mk("挂断原因", 1, "00", Some("bye_reason")),
            mk("通话时长", 4, "0", None),
        ]);
        let media_layout = frame_of(&[
            mk("对端设备编号", 3, "0", None),
            mk("视频编码", 1, "01", Some("video_codec")),
            mk("媒体端口", 2, "0", None),
            mk("流标识", 4, "0", None),
        ]);
        let conds = |f: &str, v: &str| vec![MatchCond { field: f.into(), value: v.into() }];
        // 候选判定复刻 process_incoming: 断言全过 → 帧长一致优先, 否则记兜底
        let pick = |frame: &[ProtocolField], conds: &[MatchCond], data: &[u8]| -> Option<bool> {
            (check_match_conds(frame, &d, conds, data))
                .then(|| frame_len_matches(frame, data))
        };

        // ① 呼叫帧(12+8=20B): 业务=02(语音) — 呼叫命中; 挂断因 呼叫时间首字节∉挂断原因字典 落选
        let ring = hdr(0x00, 0x10, 0x01, 0x01, 0x02);
        let mut data = ring.clone();
        data.extend([0x00, 0x00, 0x02, 0x69, 0x0A, 0x00, 0x00, 0x02]);
        assert_eq!(pick(&ring_layout, &conds("业务类型", "*"), &data), Some(true));
        assert_eq!(pick(&bye_layout, &conds("挂断原因", "*"), &data), None);

        // ② 挂断帧(20B): 原因=00 时长30s — 挂断命中; 呼叫因 时长尾字节 1E∉业务字典 落选
        let mut data = ring;
        data.extend([0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x1E]);
        assert_eq!(pick(&bye_layout, &conds("挂断原因", "*"), &data), Some(true));
        assert_eq!(pick(&ring_layout, &conds("业务类型", "*"), &data), None);

        // ③ 媒体协商帧(12+10=22B): 编码=01 端口+流标识 — 命中且帧长一致;
        //    挂断布局 20B≠22B 断言即使过了也只是兜底, 不会抢走
        let mut data = hdr(0x00, 0x10, 0x01, 0x01, 0x02);
        data.extend([0x00, 0x00, 0x02, 0x01, 0x1F, 0x90, 0x5A, 0x5A, 0x5A, 0x5A]);
        assert_eq!(pick(&media_layout, &conds("视频编码", "*"), &data), Some(true));
        assert_eq!(pick(&bye_layout, &conds("挂断原因", "*"), &data), Some(false));

        // ④ 注册响应帧(12+5=17B): AA + 时间 — 响应结果=成功 命中 (token 经 hdr_ref 并入公共头)
        let reg = frame_of(&[
            mk("响应结果", 1, "AA", Some("resp_result")),
            mk("注册成功时间", 4, "0", None),
        ]);
        let mut data = hdr(0x00, 0x10, 0x01, 0x01, 0x02);
        data.extend([0xAA, 0x69, 0x0A, 0x00, 0x00]);
        assert_eq!(pick(&reg, &conds("响应结果", "成功"), &data), Some(true));
    }
}
