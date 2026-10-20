//! 报文模板变量渲染
//!
//! Hex 模板：变量渲染为十六进制字节（大端），例如
//!   `FF 00 01 {node_id} {timestamp}` + node_id_hex="0A 00 00 01"
//!   → `FF 00 01 0A 00 00 01 66 CDE ...`
//!
//! 三类变量分属独立命名空间, 互不覆盖：
//!   - 内置变量
//!     - `{node_id}`      节点配置的 node_id_hex（原样填入）
//!     - `{timestamp}`    Unix 秒, 默认 4 字节大端, `{timestamp:8}` 指定字节数
//!     - `{timestamp_ms}` Unix 毫秒
//!     - `{seq}`          节点内自增序号, 4 字节大端
//!     - `{tlen}`         渲染后报文总长字节数; 发送侧两遍渲染自动填充
//!     - `{random:N}`     N 个随机字节（默认 4）
//!     - `{datetime}`     仅 Text/Json 模板, 本地可读时间
//!   - 发送参数  `{自定义名}`          来自 [`RenderCtx::params`], 值按 hex 解析
//!   - 字典  `{dict:key}`          取该字典默认条目（排序最前一条）
//!               `{dict:key=含义}`     取指定条目, 选择器可以是含义或条目十六进制值
//!
//! 字典 Key 限定为 ASCII 标识符, 中文写在字典的「名称」与条目「含义」上,
//! 这样中文既能在 UI 里显示, 又不会因正则匹配不到而渲染失败。
//!
//! Text/Json 模板：变量按纯文本替换（timestamp 为十进制字符串等）。

use std::sync::OnceLock;

use chrono::Local;
use rand::RngCore;
use regex::Regex;

use crate::{
    error::{AppError, AppResult},
    models::{PayloadFormat, Protocol},
    render::RenderCtx,
};

static RE_VAR: OnceLock<Regex> = OnceLock::new();

/// 变量正则: 先匹配 `{dict:key[=选择器]}`（组 1/2）, 再匹配内置/参数变量（组 3/4）
///
/// 选择器允许中文({last:安全信息})与空白以外的字符; Hex 模板按空白切分 token,
/// 数字后缀({timestamp:8}/{rlen:8-64})解析失败自然回退默认宽度。
fn var_re() -> &'static Regex {
    RE_VAR.get_or_init(|| {
        Regex::new(
            r"\{dict:([A-Za-z_][A-Za-z0-9_]*)(?:=([^}\s]+))?\}|\{([A-Za-z_][A-Za-z0-9_]*)(?::([^}\s]+))?\}",
        )
        .unwrap()
    })
}

/// 渲染协议模板为实际字节
pub fn render_payload(protocol: &Protocol, ctx: &RenderCtx) -> AppResult<Vec<u8>> {
    match protocol.format {
        PayloadFormat::Hex => render_hex(&protocol.payload_template, ctx),
        PayloadFormat::Text | PayloadFormat::Json => {
            let text = render_text_template(&protocol.payload_template, ctx)?;
            Ok(text.into_bytes())
        }
    }
}

// ==================== Hex 渲染 ====================

fn render_hex(template: &str, ctx: &RenderCtx) -> AppResult<Vec<u8>> {
    let mut digits = String::new();
    for token in template.split_whitespace() {
        digits.push_str(&render_hex_template(token, 0, ctx)?);
    }
    if digits.is_empty() {
        return Err(AppError::msg("模板渲染结果为空"));
    }
    hex::decode(&digits)
        .map_err(|e| AppError::msg(format!("渲染后不是合法的十六进制串 ({digits}): {e}")))
}

/// 渲染一段含变量的十六进制串, 输出紧凑大写 hex 数字串
///
/// 字面 hex 片段与变量可混排, 如 `FF{timestamp:4}`。
///
/// * `default_width` — 变量未显式写 `:N` 时的字节宽度; 传 0 表示用变量自身默认宽度。
///   字段模式下传字段长度, 于是 4 字节字段里的 `{timestamp}` 自然渲染成 4 字节。
pub(crate) fn render_hex_template(
    s: &str,
    default_width: usize,
    ctx: &RenderCtx,
) -> AppResult<String> {
    let mut err: Option<String> = None;
    let out = var_re().replace_all(s, |caps: &regex::Captures| {
        let resolved = match caps.get(1) {
            // ---- {dict:key[=选择器]} ----
            Some(key) => ctx
                .dicts
                .resolve_hex(key.as_str(), caps.get(2).map(|m| m.as_str())),
            // ---- 内置变量 / 发送参数 ----
            None => {
                let name = caps.get(3).map(|m| m.as_str()).unwrap_or_default();
                let raw_w = caps.get(4).map(|m| m.as_str()).unwrap_or_default();
                // {rlen:最小-最大} 随机长度字节串: 区间内随机取长, 每字节随机
                if name == "rlen" {
                    return match gen_rlen(raw_w) {
                        Ok(digits) => digits,
                        Err(e) => {
                            if err.is_none() {
                                err = Some(e.to_string());
                            }
                            String::new()
                        }
                    };
                }
                // {vlen} 变长占位: 渲染为空, 仅标记该字段长度随实际值而定
                // (如「设备列表」= N × 单台记录, 由控制台批量生成 hex 后整体覆盖)
                if name == "vlen" {
                    return String::new();
                }
                let width = raw_w.parse::<usize>().unwrap_or(default_width);
                resolve_var_hex(name, raw_w, width, ctx)
            }
        };
        match resolved {
            Ok(digits) => digits,
            Err(e) => {
                if err.is_none() {
                    err = Some(e.to_string());
                }
                String::new()
            }
        }
    });
    if let Some(e) = err {
        return Err(AppError::msg(e));
    }
    // 变量已替换完, 剩余字面量必须是合法 hex
    let compact = crate::render::compact_hex(&out);
    if !compact.is_empty() && hex::decode(&compact).is_err() {
        return Err(AppError::msg(format!("`{s}` 含非法十六进制字符")));
    }
    Ok(compact)
}

/// 将单个内置变量或发送参数解析为 hex 数字串（不含空格）
pub(crate) fn resolve_var_hex(
    name: &str,
    selector: &str,
    width: usize,
    ctx: &RenderCtx,
) -> AppResult<String> {
    match name {
        "timestamp" => {
            let n = if width == 0 { 4 } else { width };
            Ok(be_hex(chrono::Utc::now().timestamp() as u128, n))
        }
        "timestamp_ms" => {
            let n = if width == 0 { 8 } else { width };
            Ok(be_hex(chrono::Utc::now().timestamp_millis() as u128, n))
        }
        // 节点编号: 节点配置的原样值。字段声明宽度与配置不一致时按大端对齐
        // (不足补零/超宽取低位), 与 {session} 同策略 —— 宽度由协议声明推导, 不卡渲染
        "node_id" => {
            let raw = ctx
                .node
                .node_id_hex
                .as_deref()
                .ok_or_else(|| AppError::msg("节点未配置 node_id_hex, 无法渲染 {node_id}"))?;
            let compact = crate::render::compact_hex(raw);
            if compact.is_empty() {
                return Err(AppError::msg("节点 node_id_hex 为空"));
            }
            let bytes = hex::decode(&compact)
                .map_err(|_| AppError::msg(format!("节点 node_id_hex 不是合法 hex: {raw}")))?;
            if width == 0 || bytes.len() > 16 {
                return Ok(compact); // 模板模式(无声明宽度)/超长值: 原样填入
            }
            let v: u128 = bytes.iter().fold(0u128, |a, b| (a << 8) | *b as u128);
            Ok(be_hex(v, width))
        }
        // 认证会话/安全信息: 注册应答返回后由引擎存入节点; 未认证时按字段宽度填 0。
        // 多终端单通道: 优先取当前终端桶里捕获的会话, 没有再回退节点级存量值。
        // 宽度与声明不一致时按大端对齐(超宽取低位/不足补零), 不因旧数据卡渲染
        "session" => {
            let per_device = ctx
                .device_bucket()
                .and_then(|b| b.get("__session").cloned());
            let compact = match per_device.as_deref() {
                Some(v) => crate::render::compact_hex(v),
                None if ctx.device_key.is_none() => ctx
                    .node
                    .session_hex
                    .as_deref()
                    .map(crate::render::compact_hex)
                    .unwrap_or_default(),
                None => String::new(),
            };
            let n = if width == 0 { 2 } else { width };
            if compact.is_empty() || compact.chars().all(|c| c == '0') {
                return Ok("0".repeat(n * 2));
            }
            let bytes = hex::decode(&compact)
                .map_err(|_| AppError::msg(format!("节点 session_hex 不是合法 hex: {compact}")))?;
            if bytes.len() > 16 {
                return Ok(compact);
            }
            let v: u128 = bytes.iter().fold(0u128, |a, b| (a << 8) | *b as u128);
            Ok(be_hex(v, n))
        }
        // 响应捕获值: {last:字段名} 取当前终端桶里该字段最近一次收到的值。
        // 值为捕获时的原始形式(hex 字段=空格 hex, uint=十进制), 统一按 hex 归一;
        // 尚未收到过 → 报错提示先触发一次响应(如先注册), 而不是静默填 0
        "last" => {
            let name = if selector.is_empty() {
                return Err(AppError::msg("{last:} 需写明字段名, 如 {last:安全信息}"));
            } else {
                selector
            };
            let v = ctx.device_bucket().and_then(|b| b.get(name).cloned()).ok_or_else(|| {
                AppError::msg(format!(
                    "尚未捕获到字段「{name}」的值(当前终端{}); 先触发一次携带它的响应(如注册/登录)",
                    ctx.device_key.as_deref().map(|k| format!(" {k}")).unwrap_or_default()
                ))
            })?;
            let t = v.trim();
            if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
                return Ok(h.to_uppercase());
            }
            if !t.is_empty() && t.chars().all(|c| c.is_ascii_hexdigit() || c.is_whitespace()) {
                return Ok(crate::render::compact_hex(t));
            }
            // 十进制(uint 字段捕获): 按数值转大端 hex, 宽度取声明值(无声明按值宽)
            match t.parse::<u128>() {
                Ok(n) => {
                    let bytes_len = if width > 0 { width } else { (128 - n.leading_zeros() as usize + 7) / 8 }.max(1);
                    Ok(be_hex(n, bytes_len.min(16)))
                }
                Err(_) => Err(AppError::msg(format!(
                    "字段「{name}」捕获的值 \"{v}\" 不是可用的 hex/十进制"
                ))),
            }
        }
        "seq" => {
            let n = if width == 0 { 4 } else { width };
            Ok(be_hex(ctx.seq as u128, n))
        }
        // 报文总长: 两遍渲染, 第一遍 ctx.tlen=0 占位(宽度一致), 第二遍为真实总长
        "tlen" => {
            let n = if width == 0 { 2 } else { width };
            Ok(be_hex(ctx.tlen as u128, n))
        }
        "random" => {
            let n = if width == 0 { 4 } else { width };
            let mut buf = vec![0u8; n.min(256)];
            rand::thread_rng().fill_bytes(&mut buf);
            Ok(hex::encode_upper(&buf))
        }
        other => {
            if let Some(v) = ctx.params.get(other) {
                let compact = crate::render::compact_hex(v);
                hex::decode(&compact)
                    .map_err(|_| AppError::msg(format!("参数 {other} 的值不是合法 hex: {v}")))?;
                return Ok(compact);
            }
            // 常见笔误: 想引用字典却漏了 dict: 前缀
            if ctx.dicts.get(other).is_some() {
                return Err(AppError::msg(format!(
                    "{{{other}}} 未定义; 若想引用字典请写成 {{dict:{other}}}"
                )));
            }
            Err(AppError::msg(format!("未知模板变量: {{{other}}}")))
        }
    }
}

/// 数值 → n 字节大端 hex 数字串
fn be_hex(value: u128, n: usize) -> String {
    let bytes = value.to_be_bytes(); // 16 字节
    let start = 16usize.saturating_sub(n);
    hex::encode_upper(&bytes[start..])
}

// ==================== Text/Json 渲染 ====================

/// 渲染一段含变量的文本（Text/Json 模板与 ascii 字段共用）
pub(crate) fn render_text_template(template: &str, ctx: &RenderCtx) -> AppResult<String> {
    let mut err: Option<String> = None;
    let out = var_re().replace_all(template, |caps: &regex::Captures| {
        let resolved = match caps.get(1) {
            Some(key) => ctx
                .dicts
                .resolve_hex(key.as_str(), caps.get(2).map(|m| m.as_str())),
            None => {
                let name = caps.get(3).map(|m| m.as_str()).unwrap_or_default();
                if name == "rlen" {
                    return match gen_rlen(caps.get(4).map(|m| m.as_str()).unwrap_or_default()) {
                        Ok(s) => s,
                        Err(e) => {
                            if err.is_none() {
                                err = Some(e.to_string());
                            }
                            String::new()
                        }
                    };
                }
                resolve_var_text(name, ctx)
            }
        };
        match resolved {
            Ok(s) => s,
            Err(e) => {
                if err.is_none() {
                    err = Some(e.to_string());
                }
                String::new()
            }
        }
    });
    if let Some(e) = err {
        return Err(AppError::msg(e));
    }
    Ok(out.into_owned())
}

    pub(crate) fn resolve_var_text(name: &str, ctx: &RenderCtx) -> AppResult<String> {
        match name {
            "timestamp" => Ok(chrono::Utc::now().timestamp().to_string()),
            "timestamp_ms" => Ok(chrono::Utc::now().timestamp_millis().to_string()),
            "node_id" => ctx
                .node
                .node_id_hex
                .clone()
                .ok_or_else(|| AppError::msg("节点未配置 node_id_hex, 无法渲染 {node_id}")),
            "seq" => Ok(ctx.seq.to_string()),
            "tlen" => Ok(ctx.tlen.to_string()),
            "random" => {
            let mut buf = [0u8; 4];
            rand::thread_rng().fill_bytes(&mut buf);
            Ok(hex::encode(buf))
        }
        "datetime" => Ok(Local::now().format("%Y-%m-%d %H:%M:%S").to_string()),
        other => {
            if let Some(v) = ctx.params.get(other) {
                return Ok(v.clone());
            }
            if ctx.dicts.get(other).is_some() {
                return Err(AppError::msg(format!(
                    "{{{other}}} 未定义; 若想引用字典请写成 {{dict:{other}}}"
                )));
            }
            Err(AppError::msg(format!("未知模板变量: {{{other}}}")))
        }
    }
}

// ==================== 单元测试 ====================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{MsgType, Node, NodeRole};
    use crate::render::{DictMap, DictSetLite};
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
            node_id_hex: Some("0A 00 00 01".into()),
            session_hex: None,
            captures_json: None,
            group_bind_json: None,
            enabled: true,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    fn protocol(template: &str) -> Protocol {
        Protocol {
            header_refs_json: None,
            id: 1,
            protocol_id: "p1".into(),
            name: "p".into(),
            format: PayloadFormat::Hex,
            msg_type: MsgType::Request,
            def_mode: crate::models::DefMode::Template,
            fields_json: None,
            system_ref: None,
            group_ref: None,
            group_name: None,
            system_name: None,
            payload_template: template.into(),
            match_rule: None,
            match_fields_json: None,
            auto_respond: None,
            auto_periodic: None,
            auto_periodic_interval_ms: 0,
            respond_delay_ms: 0,
            description: None,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    /// 含两张字典的测试字典: cmd(1B) / device_type(1B)
    fn dicts() -> DictMap {
        let mut m = DictMap::new();
        m.insert(DictSetLite {
            key: "cmd".into(),
            name: "命令字".into(),
            byte_len: 1,
            items: vec![
                ("01".into(), "注册".into()),
                ("02".into(), "心跳".into()),
                ("03".into(), "告警".into()),
            ],
        });
        m.insert(DictSetLite {
            key: "reg_addr".into(),
            name: "注册地址".into(),
            byte_len: 2,
            items: vec![("0100".into(), "主站".into()), ("0200".into(), "备站".into())],
        });
        m
    }

    #[test]
    fn renders_node_id_and_timestamp() {
        let n = node();
        let p = protocol("FF 00 01 {node_id} {timestamp}");
        let bytes = render_payload(&p, &RenderCtx::bare(&n, 7)).unwrap();
        assert_eq!(bytes.len(), 3 + 4 + 4);
        assert_eq!(&bytes[..3], &[0xFF, 0x00, 0x01]);
        assert_eq!(&bytes[3..7], &[0x0A, 0x00, 0x00, 0x01]);
        let ts = u32::from_be_bytes(bytes[7..11].try_into().unwrap()) as i64;
        let now = chrono::Utc::now().timestamp();
        assert!((now - ts).abs() <= 2, "timestamp 不正确: {ts} vs {now}");
    }

    #[test]
    fn renders_width_suffix_and_mixed_token() {
        let n = node();
        let p = protocol("AB{timestamp:2} {seq}");
        let bytes = render_payload(&p, &RenderCtx::bare(&n, 1)).unwrap();
        assert_eq!(bytes.len(), 1 + 2 + 4);
        assert_eq!(bytes[0], 0xAB);
        // 布局: [AB][timestamp:2] 共 3 字节, {seq} 从下标 3 开始的 4 字节大端
        assert_eq!(&bytes[3..], &[0, 0, 0, 1]);
    }

    #[test]
    fn renders_custom_params() {
        let n = node();
        let mut params = HashMap::new();
        params.insert("status".to_string(), "0102".to_string());
        let p = protocol("FF 81 01 {status}");
        let bytes = render_payload(&p, &RenderCtx::with_params(&n, 0, &params)).unwrap();
        assert_eq!(bytes, vec![0xFF, 0x81, 0x01, 0x01, 0x02]);
    }

    #[test]
    fn renders_dict_default_and_selected_item() {
        let n = node();
        let d = dicts();
        let params = HashMap::new();
        let ctx = RenderCtx::new(&n, 0, &params, &d);
        // 默认条目 = 排序最前一条
        let p = protocol("FF A1 {dict:cmd}");
        assert_eq!(render_payload(&p, &ctx).unwrap(), vec![0xFF, 0xA1, 0x01]);
        // 按含义选择
        let p = protocol("FF A1 {dict:cmd=告警} {dict:reg_addr=备站}");
        assert_eq!(
            render_payload(&p, &ctx).unwrap(),
            vec![0xFF, 0xA1, 0x03, 0x02, 0x00]
        );
        // 按条目值选择
        let p = protocol("{dict:cmd=02}");
        assert_eq!(render_payload(&p, &ctx).unwrap(), vec![0x02]);
    }

    #[test]
    fn dict_and_builtin_share_no_namespace() {
        // 字典叫 seq 也不会被内置变量吃掉: 两者写法不同
        let n = node();
        let mut d = DictMap::new();
        d.insert(DictSetLite {
            key: "seq".into(),
            name: "序号字典".into(),
            byte_len: 1,
            items: vec![("FE".into(), "特殊".into())],
        });
        let params = HashMap::new();
        let ctx = RenderCtx::new(&n, 9, &params, &d);
        let p = protocol("{seq:1} {dict:seq}");
        assert_eq!(render_payload(&p, &ctx).unwrap(), vec![0x09, 0xFE]);
    }

    #[test]
    fn unknown_dict_key_and_item_are_errors() {
        let n = node();
        let d = dicts();
        let params = HashMap::new();
        let ctx = RenderCtx::new(&n, 0, &params, &d);
        assert!(render_payload(&protocol("{dict:nope}"), &ctx).is_err());
        assert!(render_payload(&protocol("{dict:cmd=不存在}"), &ctx).is_err());
    }

    #[test]
    fn missing_dict_prefix_hints_correct_syntax() {
        let n = node();
        let d = dicts();
        let params = HashMap::new();
        let ctx = RenderCtx::new(&n, 0, &params, &d);
        let e = render_payload(&protocol("{cmd}"), &ctx).unwrap_err().to_string();
        assert!(e.contains("{dict:cmd}"), "应提示正确写法: {e}");
    }

    #[test]
    fn random_renders_n_bytes() {
        let n = node();
        let p = protocol("{random:8}");
        let bytes = render_payload(&p, &RenderCtx::bare(&n, 0)).unwrap();
        assert_eq!(bytes.len(), 8);
    }

    #[test]
    fn unknown_var_is_error() {
        let n = node();
        let p = protocol("FF {nope}");
        assert!(render_payload(&p, &RenderCtx::bare(&n, 0)).is_err());
    }

    #[test]
    fn session_var_renders_node_session_or_zero() {
        let d = crate::render::DictMap::new();
        let empty = std::collections::HashMap::new();
        // 已认证: 渲染节点保存的 session
        let mut n = node();
        n.session_hex = Some("A1 B2".into());
        let c = RenderCtx::new(&n, 0, &empty, &d);
        assert_eq!(render_hex_template("{session:2}", 0, &c).unwrap(), "A1B2");
        // 未认证: 按宽度填 0
        let mut n0 = node();
        n0.session_hex = None;
        let c0 = RenderCtx::new(&n0, 0, &empty, &d);
        assert_eq!(render_hex_template("{session:2}", 0, &c0).unwrap(), "0000");
        // 存了全零也视为未认证
        let mut nz = node();
        nz.session_hex = Some("0000".into());
        let cz = RenderCtx::new(&nz, 0, &empty, &d);
        assert_eq!(render_hex_template("{session:2}", 0, &cz).unwrap(), "0000");
    }

    #[test]
    fn missing_node_id_is_error() {
        let mut n = node();
        n.node_id_hex = None;
        let p = protocol("{node_id}");
        assert!(render_payload(&p, &RenderCtx::bare(&n, 0)).is_err());
    }

    #[test]
    fn node_id_aligns_to_declared_width() {
        let d = crate::render::DictMap::new();
        let empty = std::collections::HashMap::new();
        let mut n = node();
        n.node_id_hex = Some("10 01".into());
        // 声明 3 字节: 不足补零
        let c = RenderCtx::new(&n, 0, &empty, &d);
        assert_eq!(render_hex_template("{node_id}", 3, &c).unwrap(), "001001");
        // 声明 1 字节: 超宽取低位
        assert_eq!(render_hex_template("{node_id}", 1, &c).unwrap(), "01");
        // 模板模式(无声明宽度): 原样填入
        assert_eq!(render_hex_template("{node_id}", 0, &c).unwrap(), "1001");
        // 显式宽度后缀与声明宽度等价
        assert_eq!(render_hex_template("{node_id:3}", 0, &c).unwrap(), "001001");
    }

    /// 多终端单通道: {last:字段}/{session} 按终端分桶取捕获值
    #[test]
    fn last_var_and_session_per_device_bucket() {
        let d = crate::render::DictMap::new();
        let empty = std::collections::HashMap::new();
        // 节点带两个终端的捕获桶: 设备 00000001 → 安全信息 A1B2; 设备 00000002 → 安全信息 C3D4
        let mut n = node();
        n.captures_json = Some(
            r#"{"00000001":{"__session":"A1B2","服务器地址":"C0A80001"},
                "00000002":{"__session":"C3D4","服务器地址":"C0A80002"}}"#
                .into(),
        );
        n.session_hex = Some("FFFF".into()); // 节点级存量(单终端兼容)

        // 终端 1: 引用自己桶里的值; {session} 优先桶内 __session 而非节点级
        let c1 = RenderCtx::new(&n, 0, &empty, &d).with_device_key(Some("00000001".into()));
        assert_eq!(render_hex_template("{last:服务器地址}", 4, &c1).unwrap(), "C0A80001");
        assert_eq!(render_hex_template("{session}", 2, &c1).unwrap(), "A1B2");

        // 终端 2: 同一协议同一字段, 取到的是自己的值
        let c2 = RenderCtx::new(&n, 0, &empty, &d).with_device_key(Some("00000002".into()));
        assert_eq!(render_hex_template("{session}", 2, &c2).unwrap(), "C3D4");
        let unregistered = RenderCtx::new(&n, 0, &empty, &d).with_device_key(Some("00000003".into()));
        assert_eq!(render_hex_template("{session}", 2, &unregistered).unwrap(), "0000");
        let other_system = RenderCtx::new(&n, 0, &empty, &d).with_device_key(Some("s2:00000001".into()));
        assert_eq!(render_hex_template("{session}", 2, &other_system).unwrap(), "0000");

        // 未识别终端(单终端): {session} 回退节点级存量
        let c0 = RenderCtx::new(&n, 0, &empty, &d);
        assert_eq!(render_hex_template("{session}", 2, &c0).unwrap(), "FFFF");

        // 该终端没捕获过此字段 → 明确报错(提示先触发响应), 不静默填 0
        let e = render_hex_template("{last:服务器地址}", 4, &c0).unwrap_err().to_string();
        assert!(e.contains("服务器地址"), "{e}");
    }

    #[test]
    fn odd_hex_is_error() {
        let n = node();
        let p = protocol("FF0");
        assert!(render_payload(&p, &RenderCtx::bare(&n, 0)).is_err());
    }

    #[test]
    fn text_format_uses_plain_values() {
        let n = node();
        let mut p = protocol("LOGIN {node_id} seq={seq} t={timestamp}");
        p.format = PayloadFormat::Text;
        let bytes = render_payload(&p, &RenderCtx::bare(&n, 42)).unwrap();
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.starts_with("LOGIN 0A 00 00 01 seq=42 t="));
    }

    #[test]
    fn text_format_renders_dict_item() {
        let n = node();
        let d = dicts();
        let params = HashMap::new();
        let mut p = protocol("CMD={dict:cmd=心跳}");
        p.format = PayloadFormat::Text;
        let bytes = render_payload(&p, &RenderCtx::new(&n, 0, &params, &d)).unwrap();
        assert_eq!(String::from_utf8(bytes).unwrap(), "CMD=02");
    }
}

/// `{rlen:最小-最大}` 随机长度字节串: 每次渲染在区间内随机取长, 每字节随机。
/// 区间钳制: 0 ≤ 最小 ≤ 最大 ≤ 1400; 仅写 `{rlen:N}` 等价 0-N。
fn gen_rlen(raw: &str) -> AppResult<String> {
    use rand::Rng;
    let (min, max) = match raw.split_once('-') {
        Some((a, b)) => (
            a.trim().parse::<usize>().unwrap_or(0),
            b.trim().parse::<usize>().unwrap_or(0),
        ),
        None => {
            let n = raw.trim().parse::<usize>().unwrap_or(0);
            (0, n)
        }
    };
    let (min, max) = (min.min(1400), max.min(1400));
    let (lo, hi) = (min.min(max), min.max(max));
    let len = rand::thread_rng().gen_range(lo..=hi);
    let mut buf = vec![0u8; len];
    rand::thread_rng().fill_bytes(&mut buf);
    Ok(hex::encode_upper(&buf))
}

#[cfg(test)]
mod rlen_tests {
    use super::*;
    use crate::models::NodeRole;
    use std::collections::HashMap;

    fn node() -> crate::models::Node {
        crate::models::Node {
            id: 1,
            name: "t".into(),
            role: NodeRole::Simulator,
            local_ip: "127.0.0.1".into(),
            local_port: 1,
            remote_ip: "127.0.0.1".into(),
            remote_port: 2,
            node_id_hex: Some("01".into()),
            session_hex: None,
            captures_json: None,
            group_bind_json: None,
            enabled: true,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn rlen_within_range() {
        let n = node();
        let params = HashMap::new();
        let dicts = crate::render::DictMap::default();
        let c = RenderCtx::new(&n, 0, &params, &dicts);
        for _ in 0..20 {
            let s = render_hex_template("{rlen:16-128}", 0, &c).unwrap();
            let len = s.len() / 2;
            assert!((16..=128).contains(&len), "len {len} out of range");
        }
    }

    #[test]
    fn rlen_single_value_is_zero_to_n() {
        let n = node();
        let params = HashMap::new();
        let dicts = crate::render::DictMap::default();
        let c = RenderCtx::new(&n, 0, &params, &dicts);
        let s = render_hex_template("{rlen:8}", 0, &c).unwrap();
        assert!(s.len() / 2 <= 8);
    }

    #[test]
    fn rlen_clamped_to_1400() {
        let n = node();
        let params = HashMap::new();
        let dicts = crate::render::DictMap::default();
        let c = RenderCtx::new(&n, 0, &params, &dicts);
        let s = render_hex_template("{rlen:1-99999}", 0, &c).unwrap();
        assert!(s.len() / 2 <= 1400);
    }

    #[test]
    fn rlen_varies_between_calls() {
        let n = node();
        let params = HashMap::new();
        let dicts = crate::render::DictMap::default();
        let c = RenderCtx::new(&n, 0, &params, &dicts);
        let lens: std::collections::HashSet<usize> = (0..30)
            .map(|_| render_hex_template("{rlen:10-400}", 0, &c).unwrap().len())
            .collect();
        assert!(lens.len() > 1, "长度应随渲染变化");
    }

    #[test]
    fn mixed_template_with_rlen_parses() {
        let n = node();
        let params = HashMap::new();
        let dicts = crate::render::DictMap::default();
        let c = RenderCtx::new(&n, 0, &params, &dicts);
        let s = render_hex_template("FF B1 01 {node_id} {seq} {rlen:16-64}", 0, &c).unwrap();
        // 3 + 1 + 4 = 8 字节定长头, 后接 16~64 字节随机体
        assert!(s.len() / 2 >= 8 + 16 && s.len() / 2 <= 8 + 64);
    }
}
