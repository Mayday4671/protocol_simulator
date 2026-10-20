//! 渲染上下文与字典视图
//!
//! 字典（字典）为两层结构: 字典(DictSet) → 条目(DictItem)。
//! 渲染时按「协议所属系统」构建 [`DictMap`]: 系统专属字典覆盖同 Key 的全局字典,
//! 因此不同系统可以各自拥有同名字典而互不干扰。
//!
//! 字典不再混入 `params`, 与内置变量、字段级显式参数分属三个独立命名空间:
//!   - `{node_id}` / `{timestamp}` / `{seq}` / `{random}` → 内置变量
//!   - `{自定义参数名}`                                   → 发送参数
//!   - `{dict:字典Key}` / `{dict:字典Key=含义}`           → 字典条目

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::error::{AppError, AppResult};
use crate::models::Node;

/// 单个字典的渲染视图（剥离数据库元信息, 只保留渲染与校验所需部分）
#[derive(Debug, Clone)]
pub struct DictSetLite {
    /// ASCII 标识, 模板中 `{dict:key}` 引用的就是它
    pub key: String,
    /// 中文显示名, 仅用于报错与展示
    pub name: String,
    /// 每个条目的字节长度, 绑定该字典的字段长度必须与之相等
    pub byte_len: usize,
    /// 条目列表 (十六进制值, 含义), 按 sort_order → id 升序
    pub items: Vec<(String, String)>,
}

impl DictSetLite {
    /// 默认条目（排序最前的一条）的十六进制值; 空字典返回 None
    pub fn default_value(&self) -> Option<&str> {
        self.items.first().map(|(v, _)| v.as_str())
    }

    /// 按「含义」或「十六进制值」定位条目, 返回其十六进制值
    ///
    /// 两种写法都接受, 便于人写 md 时用含义、UI 下拉时用值。
    pub fn find_value(&self, selector: &str) -> Option<&str> {
        let compact = compact_hex(selector);
        self.items
            .iter()
            .find(|(v, label)| label == selector || *v == compact)
            .map(|(v, _)| v.as_str())
    }

    /// 按十六进制值反查含义（收包解析时展示 `01 (温度传感器)`）
    pub fn label_of(&self, value_hex: &str) -> Option<&str> {
        let compact = compact_hex(value_hex);
        self.items
            .iter()
            .find(|(v, _)| *v == compact)
            .map(|(_, label)| label.as_str())
    }

    /// 可选条目摘要（报错提示用, 最多列前 6 条）
    pub fn options_hint(&self) -> String {
        let mut s = self
            .items
            .iter()
            .take(6)
            .map(|(v, label)| format!("{v}={label}"))
            .collect::<Vec<_>>()
            .join(", ");
        if self.items.len() > 6 {
            s.push_str(&format!(" 等 {} 项", self.items.len()));
        }
        if s.is_empty() {
            s.push_str("（该字典暂无条目）");
        }
        s
    }
}

/// Key → 字典的映射（已按系统作用域解析完毕）
#[derive(Debug, Clone, Default)]
pub struct DictMap {
    sets: HashMap<String, DictSetLite>,
}

impl DictMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// 插入字典; 同 Key 后插入者覆盖前者（用于系统专属覆盖全局）
    pub fn insert(&mut self, set: DictSetLite) {
        self.sets.insert(set.key.clone(), set);
    }

    pub fn get(&self, key: &str) -> Option<&DictSetLite> {
        self.sets.get(key)
    }

    pub fn is_empty(&self) -> bool {
        self.sets.is_empty()
    }

    pub fn len(&self) -> usize {
        self.sets.len()
    }

    /// 解析 `{dict:key}` / `{dict:key=选择器}` 为紧凑大写十六进制串
    ///
    /// - `selector` 为 None 时取默认条目（排序最前一条）
    /// - `selector` 可以是条目含义, 也可以是条目十六进制值
    pub fn resolve_hex(&self, key: &str, selector: Option<&str>) -> AppResult<String> {
        let set = self.get(key).ok_or_else(|| {
            AppError::msg(format!(
                "字典 [{key}] 不存在（请在「字典管理」中创建, 或检查协议所属系统是否正确）"
            ))
        })?;
        match selector {
            Some(sel) => set.find_value(sel).map(str::to_string).ok_or_else(|| {
                AppError::msg(format!(
                    "字典 [{}] 中没有条目 \"{sel}\", 可选: {}",
                    set.name,
                    set.options_hint()
                ))
            }),
            None => set
                .default_value()
                .map(str::to_string)
                .ok_or_else(|| AppError::msg(format!("字典 [{}] 还没有任何条目", set.name))),
        }
    }
}

/// 报文渲染上下文
///
/// 把渲染一帧报文所需的全部输入收拢到一处: 节点、序号、发送参数、字典。
/// 之前字典是被 `extend` 进 `params` 的, 会与字段名/内置变量抢同一个命名空间,
/// 拆开后三者互不影响。
#[derive(Debug, Clone)]
pub struct RenderCtx<'a> {
    /// 发送方节点（提供 `{node_id}`）
    pub node: &'a Node,
    /// 节点内自增序号（提供 `{seq}`）
    pub seq: u64,
    /// 显式发送参数: 字段模式下按「字段名」覆盖定义值; 模板模式下按「变量名」取值
    pub params: &'a HashMap<String, String>,
    /// 当前协议所属系统作用域内的字典
    pub dicts: &'a DictMap,
    /// 渲染后报文总长（`{tlen}` 变量取值）。两遍渲染机制: 第一遍以 0 占位求总长,
    /// 第二遍带真实总长重渲染; 宽度按字段长度补零, 两遍字节数一致
    pub tlen: u64,
    /// 本次发送的终端标识(源设备/设备序列号字段的值):
    /// `{session}`/`{last:字段}` 从该终端的捕获桶取值, 多终端共用一个通道互不串扰;
    /// None = 未识别终端(单终端场景), 取 "_" 桶
    pub device_key: Option<String>,
}

static EMPTY_PARAMS: OnceLock<HashMap<String, String>> = OnceLock::new();
static EMPTY_DICTS: OnceLock<DictMap> = OnceLock::new();

impl<'a> RenderCtx<'a> {
    pub fn new(
        node: &'a Node,
        seq: u64,
        params: &'a HashMap<String, String>,
        dicts: &'a DictMap,
    ) -> Self {
        Self {
            node,
            seq,
            params,
            dicts,
            tlen: 0,
            device_key: None,
        }
    }

    /// 仅含节点与序号的上下文（无发送参数、无字典）, 用于测试与内部应答渲染
    pub fn bare(node: &'a Node, seq: u64) -> Self {
        Self {
            node,
            seq,
            params: EMPTY_PARAMS.get_or_init(HashMap::new),
            dicts: EMPTY_DICTS.get_or_init(DictMap::default),
            tlen: 0,
            device_key: None,
        }
    }

    /// 含发送参数、无字典的上下文
    pub fn with_params(node: &'a Node, seq: u64, params: &'a HashMap<String, String>) -> Self {
        Self {
            node,
            seq,
            params,
            dicts: EMPTY_DICTS.get_or_init(DictMap::default),
            tlen: 0,
            device_key: None,
        }
    }

    /// 设置 `{tlen}` 变量取值（两遍渲染的第二遍）
    pub fn with_tlen(mut self, tlen: u64) -> Self {
        self.tlen = tlen;
        self
    }

    /// 设置终端标识（多终端单通道: 各终端独立的安全信息/会话/捕获值）
    pub fn with_device_key(mut self, key: Option<String>) -> Self {
        self.device_key = key;
        self
    }

    /// 当前终端的捕获桶(克隆返回: 借用临时 map 会悬垂)
    pub fn device_bucket(&self) -> Option<HashMap<String, String>> {
        self.node
            .capture_buckets()
            .get(self.device_key.as_deref().unwrap_or("_"))
            .cloned()
    }
}

/// 去掉空白并转大写, 得到紧凑十六进制串
pub fn compact_hex(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(|c| c.to_uppercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(key: &str, byte_len: usize, items: &[(&str, &str)]) -> DictSetLite {
        DictSetLite {
            key: key.into(),
            name: format!("字典{key}"),
            byte_len,
            items: items
                .iter()
                .map(|(v, l)| ((*v).to_string(), (*l).to_string()))
                .collect(),
        }
    }

    #[test]
    fn resolves_default_and_selected_item() {
        let mut m = DictMap::new();
        m.insert(set(
            "device_type",
            1,
            &[("01", "温度传感器"), ("02", "湿度传感器")],
        ));
        // 无选择器 → 默认条目
        assert_eq!(m.resolve_hex("device_type", None).unwrap(), "01");
        // 按含义选择
        assert_eq!(
            m.resolve_hex("device_type", Some("湿度传感器")).unwrap(),
            "02"
        );
        // 按值选择（含空格/小写也能命中）
        assert_eq!(m.resolve_hex("device_type", Some("02")).unwrap(), "02");
    }

    #[test]
    fn unknown_key_and_item_are_errors() {
        let mut m = DictMap::new();
        m.insert(set("cmd", 1, &[("01", "注册")]));
        assert!(m.resolve_hex("nope", None).is_err());
        let e = m.resolve_hex("cmd", Some("不存在")).unwrap_err().to_string();
        assert!(e.contains("01=注册"), "报错应列出可选条目: {e}");
    }

    #[test]
    fn empty_set_has_no_default() {
        let mut m = DictMap::new();
        m.insert(set("empty", 1, &[]));
        assert!(m.resolve_hex("empty", None).is_err());
    }

    #[test]
    fn later_insert_overrides_same_key() {
        let mut m = DictMap::new();
        m.insert(set("cmd", 1, &[("01", "全局")]));
        m.insert(set("cmd", 1, &[("A1", "系统专属")]));
        assert_eq!(m.resolve_hex("cmd", None).unwrap(), "A1");
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn label_lookup_for_parse_display() {
        let s = set("r", 1, &[("00", "成功"), ("01", "失败")]);
        assert_eq!(s.label_of("01"), Some("失败"));
        assert_eq!(s.label_of("0 1"), Some("失败"));
        assert_eq!(s.label_of("FF"), None);
    }
}
