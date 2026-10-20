// 与 Rust models.rs 一一对应的前端类型

export type NodeRole = "simulator" | "target";
export type PayloadFormat = "hex" | "text" | "json";
export type MsgType = "Request" | "Response" | "Heartbeat" | "Event";
export type Direction = "in" | "out";
/** 协议定义模式: 模板 / 字段表 */
export type DefMode = "template" | "fields";
/** 字段编码: hex=定长字节 / uint=无符号大端整数 / ascii=文本 */
export type FieldEnc = "hex" | "uint" | "ascii";
/** 报文详情显示分区；不影响字段的线序和编码。 */
export type FieldSection = "header" | "header_extension" | "payload" | "trailer";

/** 字段语义标记: 引擎与界面按标记工作, 不做字段名匹配 */
export type FieldRole =
  | "none"
  | "session"
  | "device_sn"
  | "device_list"
  | "device_count"
  | "op_type"
  | "result"
  | "src_id"
  | "dst_id"
  | "capture"
  | "type_code"
  | "correlation";

export interface ProtocolField {
  name: string;
  /** 组装后的公共头字段来源；用于同名字段的定向覆盖。 */
  header_key?: string | null;
  /** 未设置时按字段来源自动归入公共头或净荷。 */
  section?: FieldSection | null;
  /** 同一类分区可使用不同名称，例如公共头 A、公共头 B。 */
  section_name?: string | null;
  /** 显式设备属性映射；不同指令通过相同 Key 复用，不按显示名称猜测。 */
  profile_key?: string | null;
  /** 字节长度 */
  len: number;
  enc: FieldEnc;
  /** 固定值(hex/十进制/0x前缀/文本) 或 {node_id} 等变量; 绑定字典时存条目的十六进制值 */
  value: string;
  /**
   * 绑定的字典 Key（对应 DictSet.key）; null/undefined 表示未绑定
   *
   * 绑定后该字段取值被限定为字典条目, 长度必须等于字典 byte_len。
   * 存 Key 而非 id, 因此随协议导出到 .md 后换机导入仍能对上字典。
   */
  dict?: string | null;
  /** 变长字段({vlen})的单条记录子字段布局, 仅解析展示用 */
  unit_fields?: ProtocolField[];
  /** 字段语义标记(显式配置, 不做字段名匹配) */
  role?: FieldRole;
  /**
   * 显式声明"覆盖公共头的某个字段": 发送时该字段的值/字典写入分组公共头对应字段
   * 并从消息体剔除（布局以公共头为准）。未标记的字段即使与公共头同名也独立存在。
   */
  hdr_ref?: string | null;
  /** 出现条件(分支): 条件字段的值等于期望值时本字段才存在 */
  when?: { field: string; value: string } | null;
}

/** 字段断言匹配条件: 收包按协议帧布局解析后, 指定字段的值必须等于期望值 */
export interface MatchCond {
  /** 字段名（公共头字段或协议字段; 重名字段第二个起写作 名字#2） */
  field: string;
  /** 期望值（hex/十进制/文本/字典条目含义） */
  value: string;
}

export interface Node {
  id: number;
  name: string;
  role: NodeRole;
  local_ip: string;
  local_port: number;
  remote_ip: string;
  remote_port: number;
  node_id_hex: string | null;
  /** 认证会话(注册应答返回的 Session hex), 发送经 {session} 自动携带 */
  session_hex?: string | null;
  enabled: boolean;
  /** 绑定的协议分组 id 列表(JSON 字符串) */
  group_bind_json?: string | null;
  created_at: string;
  updated_at: string;
}

export interface NodePayload {
  name: string;
  role: NodeRole;
  local_ip: string;
  local_port: number;
  remote_ip: string;
  remote_port: number;
  node_id_hex: string | null;
  enabled: boolean;
  /** 绑定的协议分组 id 列表 */
  group_binds?: number[] | null;
}

export interface Protocol {
  id: number;
  protocol_id: string;
  name: string;
  format: PayloadFormat;
  msg_type: MsgType;
  def_mode: DefMode;
  fields_json: string | null;
  payload_template: string;
  match_rule: string | null;
  /** 字段断言匹配条件 JSON（MatchCond 数组） */
  match_fields_json: string | null;
  auto_respond: string | null;
  respond_delay_ms: number;
  /** 触发定时发送: 收到本协议后自动周期发送的协议 id(如 注册响应→心跳保活) */
  auto_periodic: string | null;
  /** 触发定时发送的间隔(毫秒) */
  auto_periodic_interval_ms: number;
  description: string | null;
  system_ref: number | null;
  group_ref: number | null;
  /** 有序公共头引用；null 兼容旧头，[] 明确无头。 */
  header_refs_json?: string | null;
  system_name: string | null;
  group_name: string | null;
  created_at: string;
  updated_at: string;
}

export interface ProtocolPayload {
  protocol_id: string;
  name: string;
  format: PayloadFormat;
  msg_type: MsgType;
  def_mode: DefMode;
  fields: ProtocolField[];
  payload_template: string;
  match_rule: string | null;
  /** 字段断言匹配条件（与正则可并用, 需全部满足） */
  match_fields: MatchCond[];
  auto_respond: string | null;
  respond_delay_ms: number;
  /** 触发定时发送: 收到本协议后自动周期发送的协议 id(如 注册响应→心跳保活) */
  auto_periodic: string | null;
  /** 触发定时发送的间隔(毫秒) */
  auto_periodic_interval_ms: number;
  description: string | null;
  system_ref: number | null;
  group_ref: number | null;
  header_refs?: string[] | null;
}

/** 协议系统（三级分层第一层: 系统 → 分组 → 协议; 公共头已下沉到分组, 系统仅作容器） */
export interface ProtocolSystem {
  id: number;
  name: string;
  description: string | null;
  /** 是否启用公共 UDP 头 */
  header_enabled: boolean;
  /** 公共头字段表 JSON */
  header_fields_json: string | null;
  created_at: string;
}

export interface SystemPayload {
  name: string;
  description: string | null;
}

/** 分组公共 UDP 头更新入参 */
export interface GroupHeaderPayload {
  enabled: boolean;
  fields: ProtocolField[];
}

export interface HeaderDefinition {
  id: string;
  name: string;
  fields: ProtocolField[];
}

export interface GroupHeadersPayload {
  headers: HeaderDefinition[];
}

/** 协议分组（三级分层第二层, 隶属于某系统）。公共 UDP 头锚定在分组（严格隔离, 不回落系统） */
export interface ProtocolGroup {
  id: number;
  system_ref: number;
  name: string;
  /** 是否启用公共 UDP 头 */
  header_enabled: boolean;
  /** 公共头字段表 JSON */
  header_fields_json: string | null;
  header_profiles_json?: string | null;
  created_at: string;
}

export interface GroupPayload {
  system_ref: number;
  name: string;
}

/**
 * 字典条目（第二层）: 字典下的一个具体取值
 */
export interface DictItem {
  id: number;
  /** 所属字典 id */
  set_ref: number;
  /** 紧凑大写十六进制值, 如 "01" / "0A00" */
  value: string;
  /** 含义, 如「温度传感器」 */
  label: string;
  /** 同字典内排序权重, 越小越前; 最前一条是 {dict:key} 的默认取值 */
  sort_order: number;
}

/**
 * 字典（第一层）+ 其全部条目
 *
 * system_ref 为 null 表示全局共享; 否则仅在该系统内可见,
 * 并覆盖同 Key 的全局字典 —— 这是不同系统同名字典能共存的原因。
 */
export interface DictSetView {
  id: number;
  /** ASCII 标识, 模板中以 {dict:key} 引用 */
  key: string;
  /** 中文显示名 */
  name: string;
  system_ref: number | null;
  /** 所属分组 id, null 表示全局共享 */
  group_ref: number | null;
  /** 所属系统名, null 表示全局 */
  system_name: string | null;
  /** 所属分组名, null 表示全局共享 */
  group_name: string | null;
  /** 每个条目的字节长度（1-64） */
  byte_len: number;
  description: string | null;
  created_at: string;
  items: DictItem[];
}

export interface DictSetPayload {
  key: string;
  name: string;
  system_ref: number | null;
  /** 所属分组 id; null = 全局共享 */
  group_ref: number | null;
  byte_len: number;
  description: string | null;
}

export interface DictItemPayload {
  set_ref: number;
  value: string;
  label: string;
  sort_order: number;
}

export interface HeartbeatView {
  id: number;
  node_ref: number;
  node_name: string;
  protocol_ref: string;
  interval_ms: number;
  auto_start: boolean;
}

export interface HeartbeatPayload {
  node_ref: number;
  protocol_ref: string;
  interval_ms: number;
  auto_start: boolean;
}

export interface UdpLogEvent {
  id: number;
  ts: string;
  direction: Direction;
  node_id: number | null;
  node_name: string | null;
  local_addr: string;
  remote_addr: string;
  protocol_id: string | null;
  protocol_name: string | null;
  raw_hex: string;
  raw_text: string;
  parsed: string | null;
  status: string;
}

export interface LogFilter {
  keyword?: string | null;
  node_id?: number | null;
  direction?: Direction | null;
  limit?: number;
  offset?: number;
}

export interface LogRecord {
  id: number;
  ts: string;
  direction: Direction;
  node_ref: number | null;
  local_addr: string;
  remote_addr: string;
  protocol_id: string | null;
  raw_hex: string | null;
  raw_text: string | null;
  parsed: string | null;
  status: string;
}

export interface EngineStatus {
  listening_nodes: number[];
  running_heartbeats: number[];
  periodic_sends: { node_id: number; protocol_id: string }[];
}

export interface LogStats {
  today_in: number;
  today_out: number;
  total: number;
}

export interface ImportResult {
  imported: number;
  updated: number;
  /** 一并导入的字典数量 */
  dict_sets: number;
  failed: string[];
}

export interface SendResult {
  bytes: number;
  hex: string;
}

export interface LoginSuccessEvent {
  node_id: number;
  node_name: string;
  protocol_id: string;
  protocol_name: string;
}

/** 会话更新事件: 收到报文中 Session 字段的值已存入节点 */
export interface SessionUpdatedEvent {
  device?: string;
  node_name: string;
  protocol_name: string;
  /** Session 紧凑十六进制 */
  session_hex: string;
}

/** 设备档案同步事件: 设备配置应答成功, 本批设备已写入/移出 device_sn 字典 */
export interface DeviceSavedEvent {
  node_name: string;
  protocol_name: string;
  /** 本次操作: 新增 / 删除 */
  op_label: string;
  /** 本批设备数 */
  count: number;
  /** 本批序列号可读形式 */
  devices: string[];
}


// ==================== 报文解析详情 ====================

/** 单字段解析结果 */
export interface FieldDetail {
  name: string;
  /** 原始字段名；列表记录的显示名称可能另带记录序号。 */
  field_name?: string;
  section?: FieldSection | "unparsed";
  section_name?: string | null;
  /** 列表字段名及从 1 开始的记录序号。 */
  record_name?: string | null;
  record_index?: number | null;
  /** 字段含义 (字典条目含义或编码说明) */
  meaning: string;
  /** 起始字节序号 */
  start: number;
  len: number;
  /** 本次报文应有长度；len 是实际可用字节数。 */
  expected_len?: number;
  /** 如 "0xFFA1" */
  hex: string;
  /** 十进制值; 超过 8 字节为 null */
  dec: string | null;
  dict_label: string | null;
  /** 属于系统公共 UDP 头 */
  is_header: boolean;
}

/** 一帧报文完整解析 */
export interface DatagramParse {
  log_id: number;
  ts: string;
  direction: Direction;
  node_name: string | null;
  local_addr: string;
  remote_addr: string;
  protocol_id: string;
  protocol_name: string;
  msg_type_label: string;
  total_len: number;
  header_len: number;
  defined_len: number;
  /** 原始报文紧凑十六进制 (大写无分隔) */
  raw_hex: string;
  fields: FieldDetail[];
}
export interface TerminalProfile {
  id: number;
  name: string;
  node_ref: number;
  system_ref: number;
  protocol_ref: string;
  device_key: string;
  fields_json: string;
  session_hex: string | null;
}
