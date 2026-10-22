// Rust 后端 Tauri Commands 的类型安全封装
import { invoke } from "@tauri-apps/api/core";
export const listTerminals = () => invoke<import("@/types").TerminalProfile[]>("list_terminals");
export const saveTerminal = (payload: import("@/types").TerminalProfile) => invoke<void>("save_terminal", { payload });
export const deleteTerminal = (id: number) => invoke<void>("delete_terminal", { id });
import type {
  EngineStatus,
  HeartbeatPayload,
  HeartbeatView,
  ImportResult,
  LogFilter,
  LogRecord,
  LogStats,
  Node,
  NodePayload,
  Protocol,
  ProtocolGroup,
  ProtocolPayload,
  ProtocolSystem,
  SendResult,
  GroupHeaderPayload,
  GroupHeadersPayload,
  SystemPayload,
  GroupPayload,
  DictItem,
  DictItemPayload,
  DictSetPayload,
  DictSetView,
  DatagramParse,
} from "@/types";

/** Dictionary consumers stay mounted in workspace tabs, so writes must invalidate their catalog. */
export const DICTIONARY_CATALOG_CHANGED = "simterm:dictionary-catalog-changed";

async function notifyDictionaryChange<T>(operation: Promise<T>): Promise<T> {
  const result = await operation;
  window.dispatchEvent(new Event(DICTIONARY_CATALOG_CHANGED));
  return result;
}

// ---- 节点 ----
export const listNodes = () => invoke<Node[]>("list_nodes");
export const createNode = (payload: NodePayload) =>
  invoke<Node>("create_node", { payload });
export const updateNode = (id: number, payload: NodePayload) =>
  invoke<Node>("update_node", { id, payload });
export const deleteNode = (id: number) => invoke<void>("delete_node", { id });

// ---- 协议 ----
export const listProtocols = () => invoke<Protocol[]>("list_protocols");
export const upsertProtocol = (payload: ProtocolPayload) =>
  invoke<Protocol>("upsert_protocol", { payload });
export const deleteProtocol = (id: number) =>
  invoke<void>("delete_protocol", { id });
export const importProtocolsFromXml = (filePath: string) =>
  notifyDictionaryChange(invoke<ImportResult>("import_protocols_from_xml", { filePath }));
export const exportProtocolsXmlText = (protocolIds: string[]) =>
  invoke<string>("export_protocols_xml_text", { protocolIds });
export const importProtocolsXmlText = (xml: string) =>
  notifyDictionaryChange(invoke<ImportResult>("import_protocols_xml_text", { xml }));
export const exportProtocolsToXml = (
  protocolIds: string[],
  filePath: string,
) => invoke<number>("export_protocols_to_xml", { protocolIds, filePath });

// ---- 协议系统（三级分层） ----
export const listSystems = () =>
  invoke<ProtocolSystem[]>("list_systems");
export const createSystem = (payload: SystemPayload) =>
  invoke<ProtocolSystem>("create_system", { payload });
export const updateSystem = (id: number, payload: SystemPayload) =>
  invoke<ProtocolSystem>("update_system", { id, payload });
export const updateGroupHeader = (id: number, payload: GroupHeaderPayload) =>
  invoke<ProtocolGroup>("update_group_header", { id, payload });
export const updateGroupHeaders = (id: number, payload: GroupHeadersPayload) =>
  invoke<ProtocolGroup>("update_group_headers", { id, payload });
export const deleteSystem = (id: number) =>
  invoke<void>("delete_system", { id });

// ---- 协议分组（三级分层第二层） ----
export const listGroups = () => invoke<ProtocolGroup[]>("list_groups");
export const createGroup = (payload: GroupPayload) =>
  invoke<ProtocolGroup>("create_group", { payload });
export const updateGroup = (id: number, payload: GroupPayload) =>
  invoke<ProtocolGroup>("update_group", { id, payload });
export const deleteGroup = (id: number) =>
  invoke<void>("delete_group", { id });

// ---- 字典（第一层） ----
/** 全部字典, 含各自的条目; 全局字典在前 */
export const listDictSets = () => invoke<DictSetView[]>("list_dict_sets");
/** id 为 null 时新建, 否则修改 */
export const upsertDictSet = (id: number | null, payload: DictSetPayload) =>
  notifyDictionaryChange(invoke<DictSetView>("upsert_dict_set", { id, payload }));
/** 仍被协议字段引用时后端会拒绝删除 */
export const deleteDictSet = (id: number) =>
  notifyDictionaryChange(invoke<void>("delete_dict_set", { id }));
/** 引用了该字典的协议 ID 列表 */
export const dictSetRefs = (id: number) =>
  invoke<string[]>("dict_set_refs", { id });

// ---- 字典条目（第二层） ----
export const upsertDictItem = (id: number | null, payload: DictItemPayload) =>
  notifyDictionaryChange(invoke<DictItem>("upsert_dict_item", { id, payload }));
export const deleteDictItem = (id: number) =>
  notifyDictionaryChange(invoke<void>("delete_dict_item", { id }));

// ---- 周期发送 ----
export const startPeriodicSend = (
  nodeId: number,
  protocolId: string,
  intervalMs: number,
  params: Record<string,string> = {},
) =>
  invoke<void>("start_periodic_send", {
    params,
    nodeId,
    protocolId,
    intervalMs,
  });
export const stopPeriodicSend = (nodeId: number, protocolId: string) =>
  invoke<void>("stop_periodic_send", { nodeId, protocolId });

// ---- 引擎 ----
export const startUdpServer = (nodeId: number) =>
  invoke<void>("start_udp_server", { nodeId });
export const stopUdpServer = (nodeId: number) =>
  invoke<void>("stop_udp_server", { nodeId });
export const engineStatus = () => invoke<EngineStatus>("engine_status");
export const sendUdpMessage = (
  nodeId: number,
  protocolId: string,
  params: Record<string, string> | null,
) => invoke<SendResult>("send_udp_message", { nodeId, protocolId, params });
export const login = (nodeId: number, protocolId: string) =>
  invoke<SendResult>("login", { nodeId, protocolId });
export const stopAll = () => invoke<void>("stop_all");

// ---- 心跳绑定 ----
export const listHeartbeats = () =>
  invoke<HeartbeatView[]>("list_heartbeats");
export const upsertHeartbeat = (payload: HeartbeatPayload) =>
  invoke<HeartbeatView>("upsert_heartbeat", { payload });
export const deleteHeartbeat = (id: number) =>
  invoke<void>("delete_heartbeat", { id });
export const startHeartbeatTask = (bindingId: number) =>
  invoke<void>("start_heartbeat_task", { bindingId });
export const stopHeartbeatTask = (bindingId: number) =>
  invoke<void>("stop_heartbeat_task", { bindingId });

// ---- 日志 ----
export const queryLogs = (filter: LogFilter) =>
  invoke<LogRecord[]>("query_logs", { filter });
export const clearLogs = (keepRecent: number) =>
  invoke<number>("clear_logs", { keepRecent });
export const logStats = () => invoke<LogStats>("log_stats");

// ---- 报文解析详情 ----
export const parseDatagram = (logId: number) =>
  invoke<DatagramParse>("parse_datagram", { logId });

// 业务流程(OA 式步骤图)
export interface SignalField {
  name: string;
  len: number;
  enc: string;
  value: string;
  dict?: string | null;
  role?: string;
}
export interface SignalDef {
  name: string;
  fields: SignalField[];
  header_refs?: string[] | null;
}
export interface RouteRule {
  field: string;
  value: string;
  to: string;
}
export interface FlowStep {
  action?: string;
  correlation?: { request: string; response: string }[];
  allow_unkeyed_reply?: boolean;
  result_field?: string;
  success_hex?: string;
  id: string;
  name: string;
  x: number;
  y: number;
  protocol_id: string;
  terminal_mode: string;
  terminal_id?: number | null;
  params: Record<string, string>;
  expect_protocol?: string | null;
  timeout_ms: number;
  delay_ms: number;
  next?: string | null;
  on_success?: string | null;
  on_failure?: string | null;
  on_timeout?: string | null;
  request_def?: SignalDef | null;
  expect_def?: SignalDef | null;
  routes?: RouteRule[];
}
export interface FlowRow {
  id: number;
  name: string;
  system_ref: number | null;
  group_ref?: number | null;
  description: string | null;
  steps: FlowStep[];
  trigger_protocols: string[];
  schedule_interval_ms: number;
}
export interface FlowRunEvent {
  run_id: number;
  flow_id: number;
  flow_name: string;
  step_id: string;
  step_name: string;
  terminal: string | null;
  status: string;
  note: string;
}
export const listFlows = () => invoke<FlowRow[]>("list_flows");
export const upsertFlow = (payload: FlowRow) => invoke<FlowRow>("upsert_flow", { payload });
export const deleteFlow = (id: number) => invoke<void>("delete_flow", { id });
export const startFlowRun = (flowId: number, terminalIds: number[], nodeId?: number, manual = false) =>
  invoke<number>("start_flow_run", { flowId, terminalIds, nodeId, manual });
export interface FlowTraceEvent extends FlowRunEvent {
  index: number; at: number; attempt: number;
  packet: import("@/types").LogRecord | null;
  next_step: string | null; branch: string | null;
}
export interface FlowTrace {
  run_id: number; flow: FlowRow; node_id: number; node_name: string;
  manual: boolean; started_at: number; ended_at: number | null;
  terminal_ids: number[];
  status: string; events: FlowTraceEvent[]; omitted: number;
  step_states: Record<string, FlowTraceEvent>;
  transitions: FlowTraceEvent[];
}
export const getFlowTraces = (flowId: number) => invoke<FlowTrace[]>("flow_traces", { flowId });
export const stopFlowRun = (runId: number) => invoke<void>("stop_flow_run", { runId });
export const listFlowRuns = () =>
  invoke<{ run_id: number; flow_id: number; node_id: number }[]>("flow_runs");
