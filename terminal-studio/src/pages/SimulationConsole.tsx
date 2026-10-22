import { FlowConsoleTracker, type FlowConsoleTrackerHandle } from "@/components/flows/FlowConsoleTracker";
import {
  Select as UXSelect,
  SelectTrigger as UXSelectTrigger,
  SelectValue as UXSelectValue,
  SelectContent as UXSelectContent,
  SelectItem as UXSelectItem,
} from "@/components/ui/select";
import {
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { toast } from "sonner";
import { ListEntries } from "@/components/protocols/ListEntries";
import { buildSimulationParams, computedFieldLabel } from "@/lib/simulationParams";
import { packetSchemaKey } from "@/lib/protocolHeaders";
import type { ListEntry } from "@/lib/protocolLists";
import { FieldBranches } from "@/components/protocols/FieldBranches";
import { listen } from "@tauri-apps/api/event";
import { listTerminals } from "@/lib/api";
import type { TerminalProfile } from "@/types";
import {
  defaultFieldValue,
  fieldIsActive,
  parseProperties,
  protocolFrame,
} from "@/lib/deviceProfiles";
import { deviceFieldValue, deviceSelectionValues, isDeviceField, type DeviceFieldSource } from "@/lib/deviceSelection";
import { WorkspaceActivityContext } from "@/components/layout/WorkspaceActivity";
import {
  Activity,
  ChevronDown,
  ChevronRight,
  Eraser,
  Pause,
  Play,
  Repeat,
  Send,
} from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { Card, CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import {
  listFlows,
  listGroups,
  listDictSets,
  listNodes,
  listProtocols,
  listSystems,
  sendUdpMessage,
  startPeriodicSend,
  stopPeriodicSend,
  type FlowRow,
} from "@/lib/api";
import { dictItemLabel, ComboInput } from "@/components/ui/combo-input";
import { useUdpLogs } from "@/hooks/useUdpLogs";
import { DatagramDetail } from "@/components/monitor/DatagramDetail";
import { LogRow } from "@/components/monitor/LogRow";
import type {
  FieldEnc,
  MsgType,
  Node,
  Protocol,
  ProtocolField,
  ProtocolGroup,
  ProtocolSystem,
  DictSetView,
} from "@/types";
const encLabel: Record<FieldEnc, string> = {
  hex: "",
  uint: "十进制",
  ascii: "文本",
};
const typeLabel: Record<MsgType, string> = {
  Request: "请求",
  Response: "应答",
  Heartbeat: "心跳",
  Event: "事件",
};
/** 展示用字段名: `序号#2` → `序号(2)` */
export function displayFieldName(name: string): string {
  const m = name.match(/^(.*)#(\d+)$/);
  return m ? `${m[1]}(${m[2]})` : name;
}
/** 内置模板变量 → 用户可读的自动填充说明（node_id 不在其中: 可改为任意设备ID） */
const AUTO_VAR_LABEL: Record<string, string> = {
  seq: "序号（自动自增）",
  tlen: "报文总长（自动计算）",
  timestamp: "时间戳（自动取当前）",
  random: "随机数（每次随机）",
};
const AUTO_VAR_RE = /^\{(seq|tlen|timestamp|random|rlen[^}]*|last:[^}]*)\}$/;
/** Field-local device selection shares the ordinary editable input. */
function FieldInput({
  f,
  value,
  dicts,
  devices,
  onDeviceSelect,
  onChange,
}: {
  f: ProtocolField;
  value: string;
  dicts: DictSetView[];
  devices: DeviceFieldSource[];
  onDeviceSelect: (device: DeviceFieldSource) => void;
  onChange: (v: string) => void;
}) {
  const v = value.trim();
  const computedLabel = computedFieldLabel(f.value);
  const isNodeId = f.value.trim() === "{node_id}";
  // 定义值为内置自动变量: 输入框留空=自动, 可手填固定值覆盖
  const autoDef = AUTO_VAR_RE.test(f.value.trim());
  const m = f.value.trim().match(AUTO_VAR_RE);
  const autoLabel = m
    ? m[1].startsWith("last:")
      ? `响应捕获「${m[1].slice(5)}」（收到自动存, 留空自动引用）`
      : (AUTO_VAR_LABEL[m[1]] ?? m[1])
    : null;
  const isSession = f.value.trim() === "{session}";
  const dictItems = f.dict
    ? (dicts.find((s) => s.key === f.dict)?.items ?? [])
    : [];
  const norm = (x: string) => x.replace(/\s+/g, "");
  // 字典字段: 定义值与条目值做空格归一化匹配; 空值默认第一条(后端 default_value 同口径)
  const selValue =
    dictItems.length > 0
      ? v === ""
        ? dictItems[0].value
        : (dictItems.find((it) => norm(it.value) === norm(v))?.value ?? "")
      : "";
  const deviceChoices = isDeviceField(f) && f.role !== "session" && !computedLabel
    ? devices.flatMap(device => {
        try { const value = deviceFieldValue(f, device); return [{ device, value, label: `${device.name} · ${value} · ${device.systemName}` }]; }
        catch { return []; } // Width/type incompatible profiles are never truncated into the field.
      }) : [];
  const choices = [
    ...deviceChoices,
    ...dictItems.map(item => ({ value: item.value, label: dictItemLabel(item.value, item.label), device: undefined })),
  ];
  return (
    <div className={cn("grid min-w-0 content-start gap-1", f.len > 32 && "col-span-full")}>
      <Label htmlFor={`f-${f.name}`} className="flex min-w-0 items-baseline gap-1 text-xs leading-5">
        <span className="truncate" title={displayFieldName(f.name)}>{displayFieldName(f.name)}</span>
        <span className="shrink-0 whitespace-nowrap font-normal text-muted-foreground">
          {f.len}B{f.enc !== "hex" && ` · ${encLabel[f.enc]}`}
        </span>
      </Label>
      {computedLabel ? (
        <Input
          id={`f-${f.name}`}
          className="h-8 font-mono text-xs"
          value={computedLabel ?? value}
          readOnly
        />
      ) : deviceChoices.length > 0 ? (
        <ComboInput
          id={`f-${f.name}`}
          value={value}
          suggestions={choices}
          placeholder="选择设备或手动填写"
          onSelectSuggestion={index => {
            const choice = choices[index];
            if (choice.device) onDeviceSelect(choice.device);
            else onChange(choice.value);
          }}
          onChange={(e) => onChange(e.target.value)}
        />
      ) : f.dict ? (
        <Select value={selValue || undefined} onValueChange={onChange}>
          <SelectTrigger id={`f-${f.name}`} className="h-8 font-mono text-xs">
            <SelectValue
              placeholder={dictItems.length ? "选择条目" : "字典无条目"}
            />
          </SelectTrigger>
          <SelectContent>
            {dictItems.map((it) => (
              <SelectItem key={it.id} value={it.value}>
                {dictItemLabel(it.value, it.label)}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      ) : (
        <Input
          id={`f-${f.name}`}
          className="h-8 font-mono text-xs"
          value={value}
          placeholder={
            isNodeId
              ? "留空自动 · 跟随通道节点ID"
              : f.role === "device_count"
                ? f.enc === "uint"
                  ? "条目数（十进制），允许 0"
                  : "条目数，10 = 16 条"
                : isSession
                  ? "留空自动 · 会话（未认证填 00）"
                  : autoDef
                    ? `留空自动 · ${autoLabel ?? "内置变量"}`
                    : f.value.includes("{vlen")
                      ? "在下方设备条目中逐台填写"
                      : f.enc === "hex"
                        ? "留空补 0"
                        : f.enc === "uint"
                          ? "十进制 / 0x 前缀 / 留空补 0"
                          : "文本（留空补 0）"
          }
          onChange={(e) => onChange(e.target.value)}
        />
      )}
    </div>
  );
}
export function SimulationConsole() {
  const flowTracker=useRef<FlowConsoleTrackerHandle>(null);
  const { active } = useContext(WorkspaceActivityContext);
  const [nodes, setNodes] = useState<Node[]>([]);
  const [protocols, setProtocols] = useState<Protocol[]>([]);
  const [systems, setSystems] = useState<ProtocolSystem[]>([]);
  const [groups, setGroups] = useState<ProtocolGroup[]>([]);
  const [dicts, setDicts] = useState<DictSetView[]>([]);
  // 级联选择(初始恢复上次会话的选择)
  // 通道不再手选: 按所选分组在通道绑定中自动定位(nodes.group_bind_json)
  const [systemId, setSystemId] = useState<string>("all");
  const [group, setGroup] = useState<string>("all");
  const [protocolId, setProtocolId] = useState("");
  const [terminals, setTerminals] = useState<TerminalProfile[]>([]);
  const [deviceSelection, setDeviceSelection] = useState<{ key: string; id: number } | null>(null);
  useEffect(() => {
    const reload = () => {
      void listTerminals()
        .then(setTerminals)
        .catch((e) => toast.error(String(e)));
    };
    reload();
    const unsub = [
      listen("session-updated", reload),
      listen("devices-updated", reload),
    ];
    return () => {
      unsub.forEach((p) => {
        void p.then((f) => f());
      });
    };
  }, []);
  // 字段参数表单（字段名 → 值; 预填定义值, 可改; 同协议返回时恢复上次填写）
  const [commandDraft, setCommandDraft] = useState<{
    key: string;
    values: Record<string, string>;
  }>({ key: "", values: {} });
  const [sending, setSending] = useState(false);
  const [periodicBusy, setPeriodicBusy] = useState(false);
  const [monitorOpen, setMonitorOpen] = useState(true);
  // 业务流程一键运行
  const [flows, setFlows] = useState<FlowRow[]>([]);
  useEffect(() => {
    if (active) listFlows().then(setFlows).catch(() => setFlows([]));
  }, [active]);
  // 周期发送
  const [interval, setIntervalMs] = useState("2000");
  const { engine, refreshEngine, logs, paused, setPaused, clearScreen } =
    useUdpLogs();
  // 内嵌实时监控: 与「实时监控」页共享同一份日志缓冲
  const [detailId, setDetailId] = useState<number | null>(null);
  const [autoScroll, setAutoScroll] = useState(true);
  const [dirFilter, setDirFilter] = useState<"all" | "in" | "out">("all");
  const listRef = useRef<HTMLDivElement>(null);
  // 最新收发排在最上面; 按收/发过滤后取最近 200 条
  const renderedLogs = useMemo(
    () =>
      logs
        .filter((l) => dirFilter === "all" || l.direction === dirFilter)
        .slice(-200)
        .reverse(),
    [logs, dirFilter],
  );
  useEffect(() => {
    // 最新在上: 新报文到达时回到顶部
    if (autoScroll && !paused && listRef.current) {
      listRef.current.scrollTop = 0;
    }
  }, [renderedLogs[0]?.id, autoScroll, paused]);
  const refresh = useCallback(async () => {
    try {
      const [n, p, s, g, d] = await Promise.all([
        listNodes(),
        listProtocols(),
        listSystems(),
        listGroups(),
        listDictSets(),
      ]);
      setNodes(n);
      setProtocols(p);
      setSystems(s);
      setGroups(g);
      setDicts(d);
    } catch (e) {
      toast.error("加载配置失败", { description: String(e) });
    }
  }, []);
  useEffect(() => {
    if (!active) return;
    void refresh();
    void refreshEngine();
    void listTerminals()
      .then(setTerminals)
      .catch((e) => toast.error(String(e)));
  }, [active, refresh, refreshEngine]);
  /** 绑定解析 */
  const bindsOf = useCallback((n: Node) => {
    try {
      return JSON.parse(n.group_bind_json ?? "[]") as number[];
    } catch {
      return [];
    }
  }, []);
  const curProtocol = protocols.find((p) => p.protocol_id === protocolId);
  // 选择具体协议后，以协议所属分组定位通道，即使筛选仍是“全部分组”。
  const curGroupId =
    curProtocol?.group_ref ?? (group !== "all" ? Number(group) : null);
  const matchingNodes = nodes.filter(
    (n) => curGroupId != null && bindsOf(n).includes(curGroupId),
  );
  const [channelId, setChannelId] = useState("");
  const curNode =
    matchingNodes.find((n) => String(n.id) === channelId) ??
    (curGroupId != null
      ? (nodes.find((n) => bindsOf(n).includes(curGroupId)) ?? null)
      : null);
  const systemFlows = useMemo(
    () => flows.filter((f) => f.system_ref === (systemId === "all" ? f.system_ref : Number(systemId))),
    [flows, systemId],
  );
  /** 协议所属分组: 公共头锚定在分组（严格隔离） */
  const curGroup = useMemo(
    () =>
      curProtocol?.group_ref != null
        ? groups.find((g) => g.id === curProtocol.group_ref)
        : undefined,
    [groups, curProtocol],
  );
  const { head: headFields, body: bodyFields, error: schemaError } = useMemo(
    () => protocolFrame(curProtocol, curGroup), [curGroup, curProtocol],
  );
  const headerSections = useMemo(() => {
    const sections: { id: string; name: string; fields: ProtocolField[] }[] = [];
    for (const field of headFields) {
      const id = field.header_key?.split('::')[0] ?? 'legacy-default';
      const previous = sections[sections.length - 1];
      if (previous?.id === id) previous.fields.push(field);
      else sections.push({ id, name: field.section_name ?? '公共头', fields: [field] });
    }
    return sections;
  }, [headFields]);
  const [headerOpen, setHeaderOpen] = useState(false);
  /** 当前协议作用域内的字典: 全局共享 + 协议所属分组专属（同 Key 分组覆盖全局） */
  const scopedDicts = useMemo(() => {
    const gid = curProtocol?.group_ref ?? null;
    const byKey = new Map<string, DictSetView>();
    for (const st of dicts) {
      if (st.group_ref != null && st.group_ref !== gid) continue;
      const prev = byKey.get(st.key);
      if (!prev || prev.group_ref == null) byKey.set(st.key, st);
    }
    return [...byKey.values()];
  }, [dicts, curProtocol]);
  const frameFields = useMemo(
    () => [...headFields, ...bodyFields],
    [headFields, bodyFields],
  );
  const identityField =
    frameFields.find((f) => f.role === "src_id" || f.role === "device_sn");
  const schemaKey = useMemo(() => packetSchemaKey(frameFields), [frameFields]);
  const draftKey = `${curNode?.id ?? ""}:${protocolId}:${schemaKey}`;
  const listDraftKey = draftKey;
  const listDrafts = useRef<{ key: string; rows: Record<string, ListEntry[]> }>({
    key: "", rows: {},
  });
  if (listDrafts.current.key !== listDraftKey)
    listDrafts.current = { key: listDraftKey, rows: {} };
  const overrides = commandDraft.key === draftKey ? commandDraft.values : {};
  if (commandDraft.key !== draftKey)
    setCommandDraft({ key: draftKey, values: {} });
  const baseline = useMemo(
    () => ({
      ...Object.fromEntries(
        frameFields
          .filter((f) => !f.value.includes("{"))
          .map((f) => [f.name, defaultFieldValue(f, scopedDicts)]),
      ),
    }),
    [frameFields, scopedDicts],
  );
  const fieldValues = useMemo(
    () => ({ ...baseline, ...overrides }),
    [baseline, overrides],
  );
  /** 系统筛选后的协议（分组在选项生成时使用） */
  const sysFiltered = useMemo(() => {
    if (systemId === "all") return protocols;
    const sid = Number(systemId);
    return protocols.filter((p) => p.system_ref === sid);
  }, [protocols, systemId]);
  /** 分组下拉选项（系统=全部时列出全部分组, 选定系统则仅该系统） */
  const groupOptions = useMemo(() => {
    if (systemId === "all") return groups;
    return groups.filter((g) => g.system_ref === Number(systemId));
  }, [groups, systemId]);
  /** 协议下拉选项（系统 + 分组两级筛选; 只列可发送的——应答类由自动应答/服务端触发, 手选无意义） */
  const protoOptions = useMemo(() => {
    let list = sysFiltered;
    if (group !== "all") {
      const gid = Number(group);
      list = list.filter((p) => p.group_ref === gid);
    }
    return list;
  }, [sysFiltered, group]);
  /** 分支联动: 字段的 when(出现条件)对当前表单值是否成立 — 不满足则隐藏该字段 */
  const whenOk = useCallback(
    (f: ProtocolField): boolean =>
      fieldIsActive(f, frameFields, fieldValues, scopedDicts),
    [fieldValues, frameFields, scopedDicts],
  );
  /** 字段显示值: 用户覆盖值优先; 自动字段解析实际值 ——
   *  {node_id}→通道节点ID / {session}→节点当前会话 / 其余内置变量→空(placeholder 提示自动) */
  const fieldDisplay = useCallback(
    (f: ProtocolField): string => {
      const ov = fieldValues[f.name];
      if (ov !== undefined) return ov;
      const v = f.value.trim();
      if (v === "{node_id}") return curNode?.node_id_hex ?? "";
      if (v === "{session}") return "";
      if (AUTO_VAR_RE.test(v)) return "";
      return f.value;
    },
    [fieldValues, curNode],
  );
  /** 修改字段值: 清空则删除覆盖键, 回退定义值（node_id 字段回到跟随通道） */
  const setFieldValue = useCallback(
    (name: string, v: string) => {
      if (name === identityField?.name) setDeviceSelection(null);
      setCommandDraft((prev) => {
        const values = prev.key === draftKey ? prev.values : {};
        if ((values[name] ?? baseline[name] ?? "") === v) return prev;
        const next = { ...values };
        if (v === baseline[name] || (name.startsWith("__invalid_list_") && !v))
          delete next[name];
        else next[name] = v;
        return { key: draftKey, values: next };
      });
    },
    [draftKey, baseline, identityField?.name],
  );
  const deviceSources = useMemo<DeviceFieldSource[]>(() => terminals.map(t => {
    const schemas = protocols.filter(p => p.system_ref === t.system_ref)
      .sort((a, b) => Number(b.protocol_id === t.protocol_ref) - Number(a.protocol_id === t.protocol_ref));
    return {
      id: t.id, name: t.name, address: t.device_key, properties: parseProperties(t.fields_json),
      systemName: systems.find(s => s.id === t.system_ref)?.name ?? `系统 #${t.system_ref}`,
      fields: schemas.flatMap(p => protocolFrame(p, groups.find(g => g.id === p.group_ref)).fields),
    };
  }), [terminals, protocols, groups, systems]);
  const selectFieldDevice = (field: ProtocolField, device: DeviceFieldSource) => {
    try {
      const values = deviceSelectionValues(field, device, frameFields);
      setCommandDraft(prev => ({ key: draftKey, values: { ...(prev.key === draftKey ? prev.values : {}), ...values } }));
      if (field === identityField) setDeviceSelection({ key: draftKey, id: device.id });
    } catch (error) { toast.error(`无法填入 ${device.name}：${(error as Error).message}`); }
  };
  // Automatic flow execution can use a selected sender only in its own channel/system.
  const flowTerminal = deviceSelection?.key === draftKey ? terminals.find(t =>
    t.id === deviceSelection.id && t.node_ref === curNode?.id && t.system_ref === curProtocol?.system_ref) : undefined;
  const buildParams = (): Record<string, string> | null => {
    if (schemaError) { toast.error(schemaError); return null; }
    let params: Record<string, string>;
    try {
      params = buildSimulationParams(
        frameFields, fieldValues, overrides, scopedDicts, listDrafts.current.rows,
      );
    } catch (error) {
      toast.error(String(error));
      return null;
    }
    return params;
  };
  // 选中协议被过滤掉时清空
  useEffect(() => {
    if (protocolId && !protoOptions.some((p) => p.protocol_id === protocolId)) {
      setProtocolId("");
    }
  }, [protoOptions, protocolId]);
  const periodicRunning = useMemo(
    () =>
      engine?.periodic_sends.some(
        (k) =>
          curNode != null &&
          k.node_id === curNode.id &&
          k.protocol_id === protocolId,
      ) ?? false,
    [engine, curNode, protocolId],
  );
  const handleSend = async () => {
    if (sending) return;
    if (!curNode || !curProtocol) {
      toast.error("该分组未绑定通道（在通道管理中绑定后即可发送）");
      return;
    }
    // 表单值按字段名覆盖（含公共头字段）; 设备列表协议由条目表单拼装; 空值后端自动回退定义值
    const params = buildParams();
    if (!params) return;
    setSending(true);
    try {
      await flowTracker.current?.beforeSend(curProtocol.protocol_id);
      const r = await sendUdpMessage(
        curNode.id,
        curProtocol.protocol_id,
        params,
      );
      toast.success(`已发送 ${r.bytes} 字节`, { description: r.hex });
      // 请求类协议发送即挂起入网等待, 应答到达时会弹出「入网成功」
    } catch (e) {
      toast.error("发送失败", { description: String(e) });
    } finally {
      setSending(false);
    }
  };
  const togglePeriodic = async (start: boolean) => {
    if (periodicBusy) return;
    if (!curNode || !curProtocol) {
      toast.error("该分组未绑定通道（在通道管理中绑定后即可发送）");
      return;
    }
    setPeriodicBusy(true);
    try {
      if (start) {
        const ms = Number(interval);
        if (!Number.isInteger(ms) || ms < 100) {
          toast.error("发送间隔不能小于 100 毫秒");
          return;
        }
        const params = buildParams();
        if (!params) return;
        await flowTracker.current?.beforeSend(curProtocol.protocol_id);
        await startPeriodicSend(
          curNode.id,
          curProtocol.protocol_id,
          ms,
          params,
        );
        toast.success(`已开始周期发送「${curProtocol.name}」, 间隔 ${ms} 毫秒`);
      } else {
        await stopPeriodicSend(curNode.id, curProtocol.protocol_id);
        toast.info(`已停止周期发送「${curProtocol.name}」`);
      }
      await refreshEngine();
    } catch (e) {
      toast.error("操作失败", { description: String(e) });
    } finally {
      setPeriodicBusy(false);
    }
  };

  return (
    <div className="console-workspace">
      {/* 表单区: 内部滚动(页面永不滚) */}
      <div className="console-editor">
        <Card className="console-card">
          <CardContent className="console-content">
            {/* 级联选择: 通道由所选分组按绑定自动确定 */}
            <div className="console-selectors">
              <div className="grid gap-2">
                <Label htmlFor="console-系统">系统</Label>
                <Select
                  value={systemId}
                  onValueChange={(v) => {
                    setSystemId(v);
                    setGroup("all");
                  }}
                >
                  <SelectTrigger id="console-系统">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="all">全部系统</SelectItem>
                    {systems.map((s) => (
                      <SelectItem key={s.id} value={String(s.id)}>
                        {s.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className="grid gap-2">
                <Label htmlFor="console-分组">分组</Label>
                <Select value={group} onValueChange={setGroup}>
                  <SelectTrigger id="console-分组">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="all">全部分组</SelectItem>
                    {groupOptions.map((g) => (
                      <SelectItem key={g.id} value={String(g.id)}>
                        {g.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className="grid gap-2">
                <Label htmlFor="console-协议">协议</Label>
                <Select value={protocolId} onValueChange={setProtocolId}>
                  <SelectTrigger id="console-协议">
                    <SelectValue placeholder="选择协议" />
                  </SelectTrigger>
                  <SelectContent>
                    {protoOptions.map((p) => (
                      <SelectItem key={p.id} value={p.protocol_id}>
                        {p.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </div>
              <div className="grid min-w-0 gap-2">
                <Label htmlFor="console-通道">通道</Label>
                <UXSelect
                  disabled={!matchingNodes.length}
                  value={String(curNode?.id ?? "") || "__empty__"}
                  onValueChange={(value) => {
                    setChannelId(value === "__empty__" ? "" : value);
                  }}
                >
                  <UXSelectTrigger id="console-通道" aria-label="业务通道">
                    <UXSelectValue />
                  </UXSelectTrigger>
                  <UXSelectContent>
                    {!matchingNodes.length && (
                      <UXSelectItem value={String("") || "__empty__"}>
                        暂无绑定通道
                      </UXSelectItem>
                    )}
                    {matchingNodes.map((n) => (
                      <UXSelectItem
                        key={n.id}
                        value={String(n.id) || "__empty__"}
                      >
                        {n.name}
                      </UXSelectItem>
                    ))}
                  </UXSelectContent>
                </UXSelect>
              </div>
            </div>
            <FlowConsoleTracker ref={flowTracker} flows={systemFlows} protocolId={protocolId} nodeId={curNode?.id} terminalIds={flowTerminal ? [flowTerminal.id] : []} protocols={protocols} groups={groups}/>

            {/* 选中协议信息 + 字段表单 */}
            {curProtocol ? (
              <div className="space-y-3 rounded-lg border p-4">
                <div className="flex flex-wrap items-center gap-2 text-sm">
                  <span className="font-medium">{curProtocol.name}</span>
                  <Badge variant="secondary">
                    {typeLabel[curProtocol.msg_type]}
                  </Badge>
                  {curNode ? (
                    <Badge variant="outline">通道 {curNode.name}</Badge>
                  ) : (
                    <span className="text-xs text-destructive">
                      当前分组未绑定通道, 无法发送 —
                      到「通道管理」编辑通道勾选本分组
                    </span>
                  )}
                  {curProtocol.system_name && (
                    <Badge variant="outline">
                      {curProtocol.system_name}
                      {curProtocol.group_name
                        ? ` · ${curProtocol.group_name}`
                        : ""}
                    </Badge>
                  )}
                  {curProtocol.auto_respond && (
                    <span className="text-xs text-muted-foreground">
                      应答:{" "}
                      {protocols.find(
                        (x) => x.protocol_id === curProtocol.auto_respond,
                      )?.name ?? "（应答协议不存在）"}
                      {curProtocol.respond_delay_ms > 0
                        ? ` (延迟 ${curProtocol.respond_delay_ms} 毫秒)`
                        : ""}
                    </span>
                  )}
                </div>

                {schemaError && <p role="alert" className="rounded-lg border border-destructive/40 bg-destructive/10 px-4 py-3 text-sm text-destructive">{schemaError}</p>}
                {/* 公共头按协议的选用顺序显示。 */}
                {headFields.length > 0 && (
                  <div className="rounded-lg border">
                    <button
                      type="button"
                      className="flex w-full items-center gap-2 rounded-lg px-3 py-2 text-left text-sm hover:bg-accent/60"
                      aria-expanded={headerOpen}
                      onClick={() => setHeaderOpen((v) => !v)}
                    >
                      {headerOpen ? (
                        <ChevronDown className="h-4 w-4 text-muted-foreground" />
                      ) : (
                        <ChevronRight className="h-4 w-4 text-muted-foreground" />
                      )}
                      <span className="font-medium">公共头</span>
                      <span className="truncate text-xs text-muted-foreground">{headerSections.map(section => section.name).join(' → ')}</span>
                    </button>
                    {headerOpen && (
                      <div className="space-y-3 border-t p-3">
                        {headerSections.map(section => <section key={section.id}>
                        {headerSections.length > 1 && <h4 className="mb-2 text-xs font-medium text-muted-foreground">{section.name}</h4>}
                        <div className="runtime-fields">
                        {section.fields.filter(whenOk).map((f) => (
                          <FieldInput
                            key={f.name}
                            f={f}
                            value={fieldDisplay(f)}
                            dicts={scopedDicts}
                            devices={deviceSources}
                            onDeviceSelect={device => selectFieldDevice(f, device)}
                            onChange={(v) => setFieldValue(f.name, v)}
                          />
                        ))}
                        </div></section>)}
                      </div>
                    )}
                  </div>
                )}

                {curProtocol.def_mode === "fields" ? (
                  bodyFields.length === 0 ? (
                    <p className="rounded-lg border border-dashed px-4 py-3 text-center text-xs text-muted-foreground">
                      无消息体字段
                    </p>
                  ) : (
                    <div className="space-y-3">
                      <FieldBranches
                        key={draftKey}
                        fields={bodyFields.map((f) => ({
                          name: f.name,
                          value: fieldDisplay(f),
                          whenField: f.when?.field,
                          whenValue: f.when?.value,
                        }))}
                        fullWidth={(i) =>
                          bodyFields[i].role === "device_list" ||
                          bodyFields[i].len > 32
                        }
                        activeWhen={(i) => whenOk(bodyFields[i])}
                        renderField={(i) => {
                          const f = bodyFields[i];
                          const frame = [...headFields, ...bodyFields];
                          const count = frame
                            .slice(0, frame.indexOf(f))
                            .reverse()
                            .find(
                              (x) => x.role === "device_count" && whenOk(x),
                            );
                          return f.role === "device_list" &&
                            f.unit_fields?.length ? (
                            <ListEntries
                              key={draftKey + ":" + f.name}
                              field={f}
                              count={count}
                              countValue={count ? fieldDisplay(count) : ""}
                              initialValue={fieldValues[f.name]}
                              initialRows={listDrafts.current.rows[f.name]}
                              onDraftChange={(name, rows) => {
                                listDrafts.current.rows[name] = rows;
                              }}
                              resetKey={draftKey}
                              dicts={scopedDicts}
                              onChange={setFieldValue}
                            />
                          ) : (
                            <div key={f.name}>
                              <FieldInput
                                f={f}
                                value={fieldDisplay(f)}
                                dicts={scopedDicts}
                            devices={deviceSources}
                            onDeviceSelect={device => selectFieldDevice(f, device)}
                                onChange={(v) => setFieldValue(f.name, v)}
                              />
                            </div>
                          );
                        }}
                      />
                    </div>
                  )
                ) : (
                  <div className="rounded-md bg-muted/50 p-3 font-mono text-xs text-muted-foreground">
                    {curProtocol.payload_template}
                  </div>
                )}
              </div>
            ) : (
              <p className="rounded-lg border border-dashed p-6 text-center text-sm text-muted-foreground">
                请选择协议
              </p>
            )}
          </CardContent>
        </Card>
      </div>

      {/* 发送 / 周期发送 */}
      <div className="console-actions">
        <Button
          onClick={handleSend}
          disabled={sending || !curNode || !curProtocol || !!schemaError}
        >
          <Send /> {sending ? "发送中..." : "发送报文"}
        </Button>

        {curProtocol?.msg_type === "Heartbeat" && (
          <div className="flex items-center gap-3 rounded-lg border px-4 py-2">
            <Repeat className="h-4 w-4 text-muted-foreground" />
            <Switch
              id="periodic"
              checked={periodicRunning}
              onCheckedChange={(v) => void togglePeriodic(v)}
              disabled={!curNode || periodicBusy || (!!schemaError && !periodicRunning)}
            />
            <Label htmlFor="periodic" className="text-sm">
              周期发送
            </Label>
            <Input
              className="h-8 w-24"
              type="number"
              min={100}
              step={100}
              aria-label="周期发送间隔（毫秒）"
              disabled={periodicRunning || periodicBusy}
              value={interval}
              onChange={(e) => setIntervalMs(e.target.value)}
              placeholder="毫秒"
            />
            <span className="text-xs text-muted-foreground">毫秒</span>
            {periodicRunning && (
              <Badge className="gap-1 bg-primary text-primary-foreground">
                <Activity className="h-3 w-3" /> 运行中
              </Badge>
            )}
          </div>
        )}

        {curNode && (
          <span className="text-xs text-muted-foreground">
            目标: {curNode.remote_ip}:{curNode.remote_port}
          </span>
        )}
      </div>

      {/* 实时监控: 发送后无需切换页面即可看到收发流（双击行查看解析详情）。
            max-h 上限 + 内部滚动: 内容再多也只在面板内滚, 不撑破页面;
            最新收发排在最上面, 可按 收/发 过滤 */}
      <div
        className={cn(
          "app-terminal console-monitor rounded-xl border bg-muted/30 dark:bg-zinc-950",
          !monitorOpen && "console-monitor-collapsed",
        )}
      >
        <div className="flex shrink-0 flex-wrap items-center gap-2 border-b border-border/60 px-3 py-1.5">
          <button
            type="button"
            className="flex items-center gap-1 text-sm font-medium"
            aria-expanded={monitorOpen}
            onClick={() => setMonitorOpen((v) => !v)}
          >
            <ChevronDown
              className={cn("h-4 w-4", !monitorOpen && "-rotate-90")}
            />
            实时监控
          </button>
          <div className="flex items-center rounded-md border p-0.5">
            {(
              [
                ["all", "全部"],
                ["in", "收"],
                ["out", "发"],
              ] as const
            ).map(([v, label]) => (
              <button
                key={v}
                type="button"
                className={cn(
                  "rounded px-2 py-0.5 text-xs transition-colors",
                  dirFilter === v
                    ? "bg-primary text-primary-foreground"
                    : "text-muted-foreground hover:text-foreground",
                )}
                aria-pressed={dirFilter === v}
                onClick={() => setDirFilter(v)}
              >
                {label}
              </button>
            ))}
          </div>
          <span className="font-mono text-xs text-muted-foreground">
            {paused ? "已暂停" : `实时 · ${renderedLogs.length} 条`}
          </span>
          <span className="flex-1" />
          <div className="flex items-center gap-2">
            <Switch
              id="console-scroll"
              checked={autoScroll}
              onCheckedChange={setAutoScroll}
            />
            <Label
              htmlFor="console-scroll"
              className="text-xs text-muted-foreground"
            >
              自动滚动
            </Label>
          </div>
          <Button
            variant={paused ? "default" : "outline"}
            size="sm"
            onClick={() => setPaused(!paused)}
          >
            {paused ? <Play /> : <Pause />}
            {paused ? "继续" : "暂停"}
          </Button>
          <Button variant="outline" size="sm" onClick={clearScreen}>
            <Eraser /> 清屏
          </Button>
        </div>
        <div
          ref={listRef}
          hidden={!monitorOpen}
          className="min-h-0 flex-1 overflow-auto"
        >
          {renderedLogs.length === 0 ? (
            <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
              {paused ? "已暂停" : "暂无报文"}
            </div>
          ) : (
            renderedLogs.map((log) => (
              <LogRow
                key={log.id}
                log={log}
                hexMode
                onOpen={(id) => setDetailId(id)}
              />
            ))
          )}
        </div>
      </div>

      {/* 报文解析详情覆盖层 */}
      {detailId != null && (
        <DatagramDetail logId={detailId} onClose={() => setDetailId(null)} />
      )}
    </div>
  );
}
