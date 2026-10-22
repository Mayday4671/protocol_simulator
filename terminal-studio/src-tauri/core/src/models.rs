use serde::{Deserialize, Serialize};

// ==================== 枚举 ====================

/// 节点角色：模拟端 / 目标端
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum NodeRole {
    Simulator,
    Target,
}

/// 报文格式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum PayloadFormat {
    Hex,
    Text,
    Json,
}

/// 协议消息类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(rename_all = "PascalCase")]
#[serde(rename_all = "PascalCase")]
pub enum MsgType {
    Request,
    Response,
    Heartbeat,
    Event,
}

impl Default for MsgType {
    fn default() -> Self {
        MsgType::Request
    }
}

impl Default for PayloadFormat {
    fn default() -> Self {
        PayloadFormat::Hex
    }
}

/// 模板解析中间产物（一个模板可含多个协议段）
#[derive(Debug, Default, Clone)]
pub struct ProtocolMd {
    pub header_refs: Option<Vec<String>>,
    pub name: String,
    pub protocol_id: String,
    pub format: PayloadFormat,
    pub msg_type: MsgType,
    pub def_mode: DefMode,
    pub fields: Vec<ProtocolField>,
    pub payload_template: String,
    pub match_rule: Option<String>,
    /// 字段断言匹配条件（与正则可并用, 需全部满足）
    pub match_fields: Vec<MatchCond>,
    pub auto_respond: Option<String>,
    pub respond_delay_ms: i64,
    /// 收到本协议后自动开始周期发送的协议 id(如 注册响应→心跳保活)
    pub auto_periodic: Option<String>,
    /// 周期发送间隔(毫秒)
    pub auto_periodic_interval_ms: i64,
    pub description: Option<String>,
    /// 所属系统名（如 物联网协议; 导入时按名解析/创建）
    pub system: Option<String>,
    /// 所属分组（如 设备/网关）
    pub group: Option<String>,
}

/// 收发方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    #[sqlx(rename = "in")]
    In,
    #[sqlx(rename = "out")]
    Out,
}

// ==================== 节点 ====================

/// 节点（数据库行）
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Node {
    pub id: i64,
    pub name: String,
    pub role: NodeRole,
    pub local_ip: String,
    pub local_port: u16,
    pub remote_ip: String,
    pub remote_port: u16,
    pub node_id_hex: Option<String>,
    /// 认证会话（注册应答返回的 Session 十六进制）; 发送时经 {session} 变量自动携带
    #[serde(default)]
    pub session_hex: Option<String>,
    /// 绑定的协议分组(JSON 数组): 该通道对接的外部系统分组, 控制台按分组自动选通道
    #[serde(default)]
    pub group_bind_json: Option<String>,
    /// 响应捕获(按终端分桶, JSON: { 终端标识: { 字段名: 原始值 } });
    /// 一个通道模拟多个终端: 各终端的安全信息/会话/服务端分配值互不干扰,
    /// 发送时按帧内终端标识(源设备/设备序列号字段的值)取自己的一桶
    #[serde(default)]
    pub captures_json: Option<String>,
    pub enabled: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl Node {
    /// 绑定的分组 id 列表
    pub fn group_binds(&self) -> Vec<i64> {
        self.group_bind_json
            .as_deref()
            .and_then(|j| serde_json::from_str(j).ok())
            .unwrap_or_default()
    }
    /// 解析捕获桶: 终端标识 → (字段名 → 原始值)
    pub fn capture_buckets(&self) -> std::collections::HashMap<String, std::collections::HashMap<String, String>> {
        self.captures_json
            .as_deref()
            .and_then(|j| serde_json::from_str(j).ok())
            .unwrap_or_default()
    }
}

/// 节点新建/修改入参
#[derive(Debug, Clone, Deserialize)]
pub struct NodePayload {
    pub name: String,
    pub role: NodeRole,
    pub local_ip: String,
    pub local_port: u16,
    pub remote_ip: String,
    pub remote_port: u16,
    #[serde(default)]
    pub node_id_hex: Option<String>,
    /// 绑定的协议分组 id 列表
    #[serde(default)]
    pub group_binds: Option<Vec<i64>>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

// ==================== 协议 ====================

/// 协议定义模式：Hex 模板字符串 / 结构化字段表
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum DefMode {
    Template,
    Fields,
}

impl Default for DefMode {
    fn default() -> Self {
        DefMode::Template
    }
}

/// 字段编码方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum FieldEnc {
    /// 定长十六进制字节, 如 "FF A1"
    Hex,
    /// 无符号整数, 大端 len 字节; 值可填十进制或 {变量}
    Uint,
    /// 文本, 右补 0x00 到 len 字节
    Ascii,
}

/// 字段语义标记: 引擎与界面按标记工作, 不做字段名匹配。
/// 换协议时只要在字段上选择对应语义, 全部能力(会话/档案/建议/联动)自动生效。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FieldRole {
    #[default]
    None,
    /// 会话: 收到该字段的值自动存入节点, 发送侧用 {session} 变量携带
    Session,
    /// 设备序列号(单字段): 可下拉选档案设备, 绑定字典时放行任意合法 hex
    DeviceSn,
    /// 设备列表(变长 {vlen} + ·子字段布局): 控制台生成逐台填写表单
    DeviceList,
    /// 设备数量: 与设备条目表单联动
    DeviceCount,
    /// 操作类型: 绑定字典条目含义含「删除」时触发档案移除, 否则入库
    OpType,
    /// 配置结果: 绑定字典的第一条约定为「成功」, 应答成功触发档案同步
    Result,
    /// 源设备: 下拉建议=节点(当前通道优先); 收包捕获按其值分终端桶
    SrcId,
    /// 目标设备: 下拉建议=对端节点
    DstId,
    /// 响应捕获: 收到该字段值即存入当前终端的桶, 发送侧用 {last:字段名} 引用
    /// (安全信息/服务端分配地址等任意字段, 不限于会话)
    Capture,
    /// 信令类型: 本协议的身份码。每个业务指令的类型码字段标上它,
    /// 引擎自动生成「实际值==定义值」匹配条件(免手写断言), 收包按码识别协议
    TypeCode,
    /// 请求/应答关联流水号，同一设备的配置确认必须回显此字段。
    Correlation,
}

/// 仅用于解析展示的报文分区，不改变字段顺序、编码或业务语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldSection {
    Header,
    HeaderExtension,
    Payload,
    Trailer,
}

impl FieldSection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Header => "header",
            Self::HeaderExtension => "header_extension",
            Self::Payload => "payload",
            Self::Trailer => "trailer",
        }
    }

    pub fn is_header(self) -> bool {
        matches!(self, Self::Header | Self::HeaderExtension)
    }
}

/// 协议字段定义
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProtocolField {
    /// 组装后的公共头稳定键：头定义 id::原字段名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header_key: Option<String>,
    /// 稳定的设备属性 Key；只有显式映射字段参与档案保存和指令预填。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_key: Option<String>,
    /// 显式显示分区；未设置时，分组公共头来自 header，其余来自 payload。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section: Option<FieldSection>,
    /// 可选显示分区名称，例如同一帧的公共头 A / 公共头 B。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub section_name: Option<String>,
    /// 字段名（解析展示用）
    pub name: String,
    /// 字节长度
    #[serde(rename = "len")]
    pub length: u32,
    pub enc: FieldEnc,
    /// 固定值(hex/十进制/文本) 或 {node_id}/{timestamp}/{seq}/{random} 等变量;
    /// 绑定字典时存该字典某个条目的十六进制值
    #[serde(default)]
    pub value: String,
    /// 绑定的字典 Key（[`DictSet::key`]）
    ///
    /// 存 Key 而非 id: 随 fields_json 一起导出, 换机导入后仍能对上字典。
    /// 绑定后该字段取值被限定为字典条目, 且长度必须等于字典的 byte_len。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dict: Option<String>,
    /// 变长字段({vlen})的单条记录子字段布局, 仅解析展示用, 不参与渲染
    /// （渲染时列表值由控制台按此布局拼装后整体覆盖）
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unit_fields: Vec<ProtocolField>,
    /// 字段语义标记(显式配置, 不做名称匹配)
    #[serde(default)]
    pub role: FieldRole,
    /// 显式声明"覆盖公共头的某个字段": 发送时该字段的值/字典写入公共头同名字段
    /// 并从消息体剔除（字节布局以公共头定义为准）。
    /// 未标记的字段即使与公共头同名也独立存在于消息体, 互不干扰。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hdr_ref: Option<String>,
    /// 出现条件(分支): 条件字段的实际值等于期望值时本字段才存在(解析/发送/表单均跳过不满足的分支);
    /// 条件字段须位于本字段之前。None = 无条件, 恒存在
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<MatchCond>,
}

impl ProtocolField {
    /// 该字段绑定的字典 Key（空串视为未绑定）
    pub fn dict_key(&self) -> Option<&str> {
        self.dict.as_deref().map(str::trim).filter(|s| !s.is_empty())
    }
}

/// 协议（数据库行）
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Protocol {
    pub header_refs_json: Option<String>,
    pub id: i64,
    pub protocol_id: String,
    pub name: String,
    pub format: PayloadFormat,
    pub msg_type: MsgType,
    pub def_mode: DefMode,
    /// 字段表 JSON（fields 模式）
    pub fields_json: Option<String>,
    pub payload_template: String,
    pub match_rule: Option<String>,
    /// 字段断言匹配条件 JSON（MatchCond 数组; 与 match_rule 可同时存在, 需全部满足）
    #[serde(default)]
    pub match_fields_json: Option<String>,
    pub auto_respond: Option<String>,
    pub respond_delay_ms: i64,
    /// 触发定时发送: 收到本协议命中的帧后, 自动开始周期发送该协议(如注册响应→心跳保活)
    #[serde(default)]
    pub auto_periodic: Option<String>,
    /// 触发定时发送的间隔(毫秒)
    #[serde(default)]
    pub auto_periodic_interval_ms: i64,
    pub description: Option<String>,
    /// 所属系统 id (三级分层第一层)
    pub system_ref: Option<i64>,
    /// 所属分组 id (三级分层第二层)
    pub group_ref: Option<i64>,
    /// 系统名（查询时 LEFT JOIN 得到, 便于展示与导出）
    pub system_name: Option<String>,
    /// 分组名（查询时 LEFT JOIN 得到）
    pub group_name: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 字段断言匹配条件: 收包按协议帧布局解析后, 指定字段的值必须等于期望值
/// （值支持 十六进制字面量 / 字典条目含义中文 / {node_id} 等变量）。
/// 全部断言满足（且正则若存在也命中）时协议才算命中。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MatchCond {
    /// 字段名（公共头字段或协议字段, 按帧布局定位）
    pub field: String,
    /// 期望值
    pub value: String,
}

impl Protocol {
    pub fn header_refs(&self) -> crate::error::AppResult<Option<Vec<String>>> {
        self.header_refs_json.as_deref().map(|json| serde_json::from_str(json)
            .map_err(|e| crate::error::AppError::msg(format!("协议 [{}] 公共头引用无效: {e}", self.name)))).transpose()
    }
    /// 解析字段表; 非 fields 模式或 JSON 非法时返回空表
    pub fn field_specs(&self) -> Vec<ProtocolField> {
        if self.def_mode != DefMode::Fields {
            return Vec::new();
        }
        self.fields_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }

    /// 解析字段断言匹配条件; JSON 非法时返回空表
    pub fn match_conds(&self) -> Vec<MatchCond> {
        self.match_fields_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }
}

/// 协议新建/修改入参（按 protocol_id upsert）
#[derive(Debug, Clone, Deserialize)]
pub struct ProtocolPayload {
    /// None 兼容启用的旧公共头；Some([]) 明确无头；其余按引用顺序拼接。
    #[serde(default)]
    pub header_refs: Option<Vec<String>>,
    pub protocol_id: String,
    pub name: String,
    pub format: PayloadFormat,
    pub msg_type: MsgType,
    #[serde(default)]
    pub def_mode: DefMode,
    #[serde(default)]
    pub fields: Vec<ProtocolField>,
    #[serde(default)]
    pub payload_template: String,
    #[serde(default)]
    pub match_rule: Option<String>,
    /// 字段断言匹配条件（与正则可并用, 需全部满足）
    #[serde(default)]
    pub match_fields: Vec<MatchCond>,
    #[serde(default)]
    pub auto_respond: Option<String>,
    #[serde(default)]
    pub respond_delay_ms: i64,
    #[serde(default)]
    pub auto_periodic: Option<String>,
    #[serde(default)]
    pub auto_periodic_interval_ms: i64,
    #[serde(default)]
    pub description: Option<String>,
    /// 所属系统 id（保存时由命令层解析）
    #[serde(default)]
    pub system_ref: Option<i64>,
    /// 所属分组 id
    #[serde(default)]
    pub group_ref: Option<i64>,
}

/// 协议系统（三级分层第一层: 系统 → 分组 → 协议）
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ProtocolSystem {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    /// 是否启用公共 UDP 头 (发送时拼在协议报文最前面)
    pub header_enabled: bool,
    /// 公共头字段表 JSON (ProtocolField 数组)
    pub header_fields_json: Option<String>,
    pub created_at: String,
}

impl ProtocolSystem {
    /// 解析公共头字段表; 未启用或 JSON 非法时返回空表
    pub fn header_fields(&self) -> Vec<ProtocolField> {
        if !self.header_enabled {
            return Vec::new();
        }
        self.header_fields_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }
}

/// 公共头更新入参（独立于系统基本信息）
#[derive(Debug, Clone, Deserialize)]
pub struct SystemHeaderPayload {
    pub enabled: bool,
    pub fields: Vec<ProtocolField>,
}

/// 系统新建/修改入参
#[derive(Debug, Clone, Deserialize)]
pub struct SystemPayload {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// 协议分组（三级分层第二层, 隶属于某系统）。
/// 公共 UDP 头锚定在分组: 同系统下不同分组可各自定义不同的公共头（严格隔离, 不回落系统）。
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ProtocolGroup {
    pub header_profiles_json: Option<String>,
    pub id: i64,
    pub system_ref: i64,
    pub name: String,
    /// 是否启用公共 UDP 头（该分组所有协议发送时拼在报文最前面）
    pub header_enabled: bool,
    /// 公共头字段表 JSON（ProtocolField 数组）
    pub header_fields_json: Option<String>,
    pub created_at: String,
}

impl ProtocolGroup {
    /// 解析公共头字段表; 未启用或 JSON 非法时返回空表
    pub fn header_fields(&self) -> Vec<ProtocolField> {
        if !self.header_enabled {
            return Vec::new();
        }
        self.header_fields_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }
}

/// 分组新建/修改入参
#[derive(Debug, Clone, Deserialize)]
pub struct GroupPayload {
    pub system_ref: i64,
    pub name: String,
}

/// 分组公共头更新入参（独立于分组基本信息）
#[derive(Debug, Clone, Deserialize)]
pub struct GroupHeaderPayload {
    pub enabled: bool,
    pub fields: Vec<ProtocolField>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeaderDefinition {
    pub id: String,
    pub name: String,
    pub fields: Vec<ProtocolField>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GroupHeadersPayload {
    pub headers: Vec<HeaderDefinition>,
}

// ==================== 字典（字典 → 条目 两层结构） ====================

/// 字典（第一层）: 一组同义取值的集合, 如「设备类型」「应答结果」
///
/// 归属两档: `group_ref` 为 None 表示全局共享（如设备档案, 跨分组可见）,
/// 否则只在该分组内可见。同 Key 时分组专属覆盖全局。
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct DictSet {
    pub id: i64,
    /// ASCII 标识, 模板中以 `{dict:key}` 引用
    pub key: String,
    /// 中文显示名, 如「设备类型」
    pub name: String,
    /// 兼容保留（迁移 009 后恒为 NULL）: 作用域已下沉到分组
    #[serde(default)]
    pub system_ref: Option<i64>,
    /// 所属分组 id; None = 全局共享
    #[serde(default)]
    pub group_ref: Option<i64>,
    /// 每个条目的字节长度（1-64）, 绑定字段的长度必须与之相等
    pub byte_len: i64,
    pub description: Option<String>,
    pub created_at: String,
}

/// 字典条目（第二层）: 字典下的一个具体取值
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct DictItem {
    pub id: i64,
    /// 所属字典 id
    pub set_ref: i64,
    /// 紧凑大写十六进制值, 如 "01" / "0A00"
    pub value: String,
    /// 含义, 如「温度传感器」
    pub label: String,
    /// 同字典内的排序权重, 越小越前; 最前一条即 `{dict:key}` 的默认取值
    pub sort_order: i64,
}

/// 字典 + 其全部条目（前端一次拉全用）
#[derive(Debug, Clone, Serialize)]
pub struct DictSetView {
    pub id: i64,
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub system_ref: Option<i64>,
    /// 所属分组 id; None = 全局共享
    #[serde(default)]
    pub group_ref: Option<i64>,
    /// 所属系统名（经分组链路得到, None 表示全局）
    pub system_name: Option<String>,
    /// 所属分组名（LEFT JOIN 得到, None 表示全局共享）
    pub group_name: Option<String>,
    pub byte_len: i64,
    pub description: Option<String>,
    pub created_at: String,
    /// 条目列表, 按 sort_order → id 升序
    pub items: Vec<DictItem>,
}

/// 字典新建/修改入参
#[derive(Debug, Clone, Deserialize)]
pub struct DictSetPayload {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub system_ref: Option<i64>,
    /// 所属分组 id; None = 全局共享
    #[serde(default)]
    pub group_ref: Option<i64>,
    pub byte_len: i64,
    #[serde(default)]
    pub description: Option<String>,
}

/// 条目新建/修改入参
#[derive(Debug, Clone, Deserialize)]
pub struct DictItemPayload {
    pub set_ref: i64,
    pub value: String,
    pub label: String,
    #[serde(default)]
    pub sort_order: i64,
}

/// 模板中的字典定义（`<dict>` 段解析产物, 一段可含多张字典）
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DictSetMd {
    pub key: String,
    pub name: String,
    /// 所属系统名; None = 全局
    pub system: Option<String>,
    /// 所属分组名（dict 挂在 group 内时记录）; None = 系统级或全局
    pub group: Option<String>,
    /// 条目 (十六进制值, 含义), 按出现顺序
    pub items: Vec<(String, String)>,
}

impl DictSetMd {
    /// 由首个条目的十六进制长度推导字典字节数
    pub fn byte_len(&self) -> i64 {
        self.items
            .first()
            .map(|(v, _)| (v.len() / 2) as i64)
            .unwrap_or(1)
            .max(1)
    }
}

// ==================== 心跳绑定 ====================

/// 心跳绑定（数据库行）
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct HeartbeatBinding {
    pub id: i64,
    pub node_ref: i64,
    pub protocol_ref: String,
    pub interval_ms: i64,
    pub auto_start: bool,
}

/// 心跳绑定入参
#[derive(Debug, Clone, Deserialize)]
pub struct HeartbeatPayload {
    pub node_ref: i64,
    pub protocol_ref: String,
    pub interval_ms: i64,
    #[serde(default)]
    pub auto_start: bool,
}

/// 心跳绑定展示视图（联表带名称 + 运行状态）
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct HeartbeatView {
    pub id: i64,
    pub node_ref: i64,
    pub node_name: String,
    pub protocol_ref: String,
    pub interval_ms: i64,
    pub auto_start: bool,
}

// ==================== 日志 ====================

/// 通信日志（数据库行 / 查询结果）
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct LogRecord {
    pub id: i64,
    pub send_after_log_id: Option<i64>,
    pub ts: String,
    pub direction: Direction,
    pub node_ref: Option<i64>,
    pub local_addr: String,
    pub remote_addr: String,
    pub protocol_id: Option<String>,
    pub raw_hex: Option<String>,
    pub raw_text: Option<String>,
    pub parsed: Option<String>,
    pub status: String,
}

/// 经 Tauri Event `udp-log` 推送前端的实时日志
#[derive(Debug, Clone, Serialize)]
pub struct UdpLogEvent {
    pub id: i64,
    pub ts: String,
    pub direction: Direction,
    pub node_id: Option<i64>,
    pub node_name: Option<String>,
    pub local_addr: String,
    pub remote_addr: String,
    pub protocol_id: Option<String>,
    pub protocol_name: Option<String>,
    pub raw_hex: String,
    pub raw_text: String,
    pub parsed: Option<String>,
    pub status: String,
}

/// 日志查询过滤条件
#[derive(Debug, Clone, Default, Deserialize)]
pub struct LogFilter {
    #[serde(default)]
    pub keyword: Option<String>,
    #[serde(default)]
    pub node_id: Option<i64>,
    #[serde(default)]
    pub direction: Option<Direction>,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    200
}

/// 日志统计（Dashboard 用）
#[derive(Debug, Clone, Default, Serialize)]
pub struct LogStats {
    pub today_in: i64,
    pub today_out: i64,
    pub total: i64,
}

// ==================== 引擎状态视图 ====================

/// 周期发送任务标识
#[derive(Debug, Clone, Serialize)]
pub struct PeriodicSendKey {
    pub node_id: i64,
    pub protocol_id: String,
}

/// 引擎运行状态（推送给前端）
#[derive(Debug, Clone, Default, Serialize)]
pub struct EngineStatus {
    /// 正在监听的节点 id 列表
    pub listening_nodes: Vec<i64>,
    /// 正在运行的心跳绑定 id 列表
    pub running_heartbeats: Vec<i64>,
    /// 正在运行的周期发送任务
    pub periodic_sends: Vec<PeriodicSendKey>,
}

/// 手动发送结果
#[derive(Debug, Clone, Serialize)]
pub struct SendResult {
    pub bytes: usize,
    pub hex: String,
    pub log_id: i64,
}

/// Markdown 导入结果
#[derive(Debug, Clone, Default, Serialize)]
pub struct ImportResult {
    pub imported: usize,
    pub updated: usize,
    /// 一并导入的字典数量
    pub dict_sets: usize,
    pub failed: Vec<String>,
}

/// 入网成功事件负载
#[derive(Debug, Clone, Serialize)]
pub struct LoginSuccessEvent {
    pub node_id: i64,
    pub node_name: String,
    pub protocol_id: String,
    pub protocol_name: String,
}

// ==================== 报文解析详情 ====================

/// 单字段解析结果（详情页用, 含位置与进制展示）
#[derive(Debug, Clone, Serialize)]
pub struct FieldDetail {
    pub name: String,
    /// 短字段名；记录子字段为 unit 本身的名称。
    pub field_name: String,
    pub section: String,
    pub section_name: Option<String>,
    pub record_name: Option<String>,
    /// 数组记录编号，从 1 开始；普通字段为 None。
    pub record_index: Option<usize>,
    /// 字段含义/说明 (字典条目含义或编码方式描述)
    pub meaning: String,
    /// 起始字节序号 (0 起)
    pub start: usize,
    /// 实际可用字节长度；截断或空数组可为 0。
    pub len: usize,
    /// 本次布局应有的字节长度；与 len 的差值表示缺失字节。
    pub expected_len: usize,
    /// 十六进制值, 如 "0xFFA1"
    pub hex: String,
    /// 十进制值字符串; 长度超过 8 字节时为 None
    pub dec: Option<String>,
    /// 字典条目含义 (如「温度传感器」), 未绑定字典时按编码给默认说明
    pub dict_label: Option<String>,
    /// 兼容字段：section 为 header 或 header_extension。
    pub is_header: bool,
}

/// 一帧报文的完整解析（监控页双击详情）
#[derive(Debug, Clone, Serialize)]
pub struct DatagramParse {
    pub log_id: i64,
    pub ts: String,
    pub direction: crate::models::Direction,
    pub node_name: Option<String>,
    pub local_addr: String,
    pub remote_addr: String,
    pub protocol_id: String,
    pub protocol_name: String,
    /// 消息类型中文名 (请求/应答/心跳/事件)
    pub msg_type_label: String,
    pub total_len: usize,
    /// header / header_extension 分区实际可用的字节总数。
    pub header_len: usize,
    /// 协议字段实际覆盖的字节总长，不含未解析剩余字节。
    pub defined_len: usize,
    /// 原始报文紧凑十六进制 (大写无分隔), 前端渲染 HexDump 直接使用
    pub raw_hex: String,
    pub fields: Vec<FieldDetail>,
}
