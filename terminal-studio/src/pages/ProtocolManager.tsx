import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { toast } from "sonner";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  ArrowDown,
  ArrowUp,
  ChevronDown,
  ChevronRight,
  GitBranch,
  Download,
  FileCode,
  FileUp,
  FolderPlus,
  Pencil,
  Plus,
  Server,
  Trash2,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Pagination } from "@/components/ui/pagination";
import { Badge } from "@/components/ui/badge";
import { useDictionaryCatalog } from "@/hooks/useDictionaryCatalog";
import { GroupHeaderCard } from "@/components/protocols/GroupHeaderCard";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  Table,
  TablePanel,
  TableToolbar,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Textarea } from "@/components/ui/textarea";
import { arrayMove } from "@dnd-kit/sortable";
import { SortableContainer } from "@/components/ui/sortable-list";
import { dedupeNames } from "@/lib/fieldNames";
import { groupHeaderDefinitions, protocolHeaderRefs, resolveProtocolHeaders, mergeResolvedHeaderFields } from "@/lib/protocolHeaders";
import { FieldBranches, canonicalWhenValue } from "@/components/protocols/FieldBranches";
import { ProtocolXmlDialog } from "@/components/protocols/ProtocolXmlDialog";
import {
  FieldRowItem,
  FIELD_ROW_GRID,
} from "@/components/protocols/FieldRowItem";

import {
  createGroup,
  createSystem,
  deleteGroup,
  deleteProtocol as apiDeleteProtocol,
  deleteSystem,
  exportProtocolsToXml,
  importProtocolsFromXml,
  listGroups,
  listProtocols,
  listSystems,
  updateGroup,
  updateSystem,
  upsertProtocol,
} from "@/lib/api";
import { cn } from "@/lib/utils";
import type {
  DefMode,
  FieldEnc,
  FieldRole,
  MsgType,
  PayloadFormat,
  Protocol,
  ProtocolField,
  ProtocolGroup,
  ProtocolPayload,
  ProtocolSystem,
  DictSetView,
} from "@/types";

/** 字段编辑行（长度用字符串以便输入框受控） */
interface FieldRow {
  /** 字段名 */
  name: string;
  /** 字节长度 */
  len: string;
  enc: FieldEnc;
  /** 取值: 字面值 / {变量} / 绑定字典时为条目十六进制值 */
  value: string;
  /** 绑定的字典 Key, 空串表示不绑定 */
  dict: string;
  /** 字段语义标记(引擎与界面按标记工作) */
  role: FieldRole;
  /** 显式覆盖分组公共头的某字段; 空串 = 不覆盖(同名字段独立存在) */
  hdr_ref: string;
  profile_key?: string;
  section?: ProtocolField["section"];
  section_name?: ProtocolField["section_name"];
  /** 变长列表的单台记录布局(仅 设备列表(变长) 字段有); 可展开编辑 */
  units?: import("@/components/protocols/FieldRowItem").UnitRow[];
  /** 出现条件(分支): 条件字段名 / 期望值; 空串 = 无条件 */
  whenField: string;
  whenValue: string;
}

/** 「不自动应答」在 Select 中的哨兵值 */
const NO_RESP = "__no_resp__";
/** 「不触发定时发送」在 Select 中的哨兵值 */
const NO_PERIODIC = "__no_periodic__";

/** 匹配条件行(字段断言) */
interface CondRow {
  field: string;
  value: string;
}

interface FormState {
  protocol_id: string;
  name: string;
  format: PayloadFormat;
  msg_type: MsgType;
  def_mode: DefMode;
  fields: FieldRow[];
  payload_template: string;
  /** 字段断言匹配条件(全部满足才命中) */
  match_fields: CondRow[];
  auto_respond: string;
  respond_delay_ms: string;
  /** 触发定时发送: 收到本协议后自动周期发送的协议(留空=不触发) */
  auto_periodic: string;
  auto_periodic_interval_ms: string;
  description: string;
  system_ref: string; // "none" | 系统 id
  group_ref: string; // "none" | 分组 id
  header_refs: string[];
}

const emptyForm: FormState = {
  protocol_id: "",
  name: "",
  format: "hex",
  msg_type: "Request",
  def_mode: "fields",
  fields: [
    {
      name: "帧头",
      len: "2",
      enc: "hex",
      value: "FF A1",
      dict: "",
      role: "none",
      hdr_ref: "",
      whenField: "",
      whenValue: "",
    },
    {
      name: "命令字",
      len: "1",
      enc: "hex",
      value: "01",
      dict: "",
      role: "none",
      hdr_ref: "",
      whenField: "",
      whenValue: "",
    },
  ],
  payload_template: "",
  match_fields: [],
  auto_respond: "",
  respond_delay_ms: "0",
  auto_periodic: "",
  auto_periodic_interval_ms: "30000",
  description: "",
  system_ref: "none",
  group_ref: "none",
  header_refs: [],
};

/** 解析协议的 match_fields_json */
function parseMatchFieldsJson(json: string | null): CondRow[] {
  if (!json) return [];
  try {
    return (JSON.parse(json) as { field: string; value: string }[]) ?? [];
  } catch {
    return [];
  }
}

/**
 * 解析协议的 fields_json 为可编辑的字段行
 *
 * @param json - protocols.fields_json 原文, 可能为 null（模板模式）
 * @returns 字段行数组; JSON 非法时返回空数组
 */
function parseFieldsJson(json: string | null): FieldRow[] {
  if (!json) return [];
  try {
    const arr = JSON.parse(json) as ProtocolField[];
    return arr.map((f) => ({
      name: f.name,
      len: String(f.len),
      enc: f.enc,
      value: f.value ?? "",
      dict: f.dict ?? "",
      role: f.role ?? "none",
      hdr_ref: f.hdr_ref ?? "",
      profile_key: f.profile_key ?? "",
      section: f.section,
      section_name: f.section_name,
      units: (f.unit_fields ?? []).map((u) => ({
        ...u,
        name: u.name,
        len: String(u.len),
        dict: u.dict ?? "",
        role: u.role ?? "none",
      })),
      whenField: f.when?.field ?? "",
      whenValue: f.when?.value ?? "",
    }));
  } catch {
    return [];
  }
}

function toForm(p: Protocol, group?: ProtocolGroup): FormState {
  return {
    protocol_id: p.protocol_id,
    name: p.name,
    format: p.format,
    msg_type: p.msg_type,
    def_mode: p.def_mode,
    fields: parseFieldsJson(p.fields_json),
    payload_template: p.payload_template,
    match_fields: parseMatchFieldsJson(p.match_fields_json),
    auto_respond: p.auto_respond ?? "",
    respond_delay_ms: String(p.respond_delay_ms),
    auto_periodic: p.auto_periodic ?? "",
    auto_periodic_interval_ms: String(p.auto_periodic_interval_ms || 30000),
    description: p.description ?? "",
    system_ref: p.system_ref != null ? String(p.system_ref) : "none",
    group_ref: p.group_ref != null ? String(p.group_ref) : "none",
    header_refs: protocolHeaderRefs(p, group),
  };
}

/** 新建协议的内部标识自动生成（界面不再展示/录入英文协议ID, 中文名是唯一入口） */
function genProtocolId(existing: Set<string>): string {
  for (;;) {
    const id = `p_${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;
    if (!existing.has(id)) return id;
  }
}

function toPayload(f: FormState): ProtocolPayload {
  return {
    protocol_id: f.protocol_id.trim(),
    name: f.name.trim(),
    format: f.format,
    msg_type: f.msg_type,
    def_mode: f.def_mode,
    fields: f.fields.map((r) => ({
      name: r.name.trim(),
      len: Number(r.len) || 0,
      enc: r.enc,
      value: r.value.trim(),
      dict: r.dict.trim() || null,
      role: r.role ?? "none",
      hdr_ref: r.hdr_ref.trim() || null,
      profile_key: r.profile_key?.trim() || null,
      section: r.section ?? null,
      section_name: r.section_name?.trim() || null,
      unit_fields: (r.units ?? []).map((u) => ({
        ...u,
        name: u.name.trim(),
        len: Number(u.len) || 0,
        enc: u.enc ?? "hex",
        value: u.value ?? "",
        dict: u.dict.trim() || null,
        role: (u.role as never) ?? "none",
      })),
      when:
        r.whenField.trim() && r.whenValue.trim()
          ? {
              field: r.whenField.trim(),
              value: canonicalWhenValue(r.whenValue.trim()),
            }
          : null,
    })),
    payload_template: f.payload_template,
    match_rule: null,
    match_fields: f.match_fields
      .filter((c) => c.field.trim() && c.value.trim())
      .map((c) => ({ field: c.field.trim(), value: c.value.trim() })),
    auto_respond: f.auto_respond.trim() || null,
    respond_delay_ms: Number(f.respond_delay_ms) || 0,
    auto_periodic: f.auto_periodic.trim() || null,
    auto_periodic_interval_ms: Number(f.auto_periodic_interval_ms) || 0,
    description: f.description.trim() || null,
    system_ref: f.system_ref !== "none" ? Number(f.system_ref) : null,
    group_ref: f.group_ref !== "none" ? Number(f.group_ref) : null,
    header_refs: f.header_refs,
  };
}

function validate(f: FormState): string | null {
  if (!f.name.trim()) return "请填写协议名称";
  if (f.system_ref === "none") return "请选择所属系统";
  if (f.group_ref === "none") return "请选择所属分组（公共头/字典按分组隔离）";
  for (const c of f.match_fields) {
    if (c.field.trim() && !c.value.trim()) return "匹配条件缺少期望值";
    if (!c.field.trim() && c.value.trim()) return "匹配条件缺少字段";
  }
  if (f.def_mode === "fields") {
    // 字段表可为空: 纯公共头报文（整帧由系统公共头构成）
    for (let i = 0; i < f.fields.length; i++) {
      const r = f.fields[i];
      if (!r.name.trim()) return `第 ${i + 1} 个字段缺少字段名`;
      const len = Number(r.len);
      if (!Number.isInteger(len) || len < 1 || len > 65535)
        return `字段 [${r.name}] 长度需为 1-65535 的整数`;
      // 字面 hex 值校验长度; 含 {变量} 的值（如 {random} {rlen:8-64}）跳过
      if (r.enc === "hex" && r.value.trim() && !r.value.includes("{")) {
        const compact = r.value.replace(/\s+/g, "");
        if (!/^[0-9a-fA-F]+$/.test(compact))
          return `字段 [${r.name}] 的值不是合法十六进制`;
        if (r.role === "device_list" && r.units?.length) {
          if (compact.length % (len * 2) !== 0)
            return `字段 [${r.name}] 列表默认值需为单条 ${len} 字节的整数倍`;
        } else if (compact.length / 2 !== len)
          return `字段 [${r.name}] 十六进制值需 ${len} 字节, 实际 ${compact.length / 2} 字节`;
      }
    }
  } else {
    if (!f.payload_template.trim()) return "请填写报文模板";
    if (f.format === "hex") {
      const probe = f.payload_template
        .replace(/\{[^}]+\}/g, "")
        .replace(/\s+/g, "");
      if (probe && !/^[0-9a-fA-F]*$/.test(probe))
        return "报文模板中除 {变量} 外必须都是十六进制字符";
    }
  }
  return null;
}

const msgTypeLabel: Record<MsgType, string> = {
  Request: "请求",
  Response: "应答",
  Heartbeat: "心跳",
  Event: "事件",
};

const typeBadgeVariant: Record<
  MsgType,
  "default" | "secondary" | "outline" | "destructive"
> = {
  Request: "default",
  Response: "secondary",
  Heartbeat: "outline",
  Event: "destructive",
};

function defPreview(p: Protocol): { badge: string; text: string } {
  if (p.def_mode === "fields") {
    const fields = parseFieldsJson(p.fields_json);
    const total = fields.reduce((s, f) => s + (Number(f.len) || 0), 0);
    return {
      badge: `字段 ${fields.length} 项 · ${total} 字节`,
      text: fields.map((f) => f.name).join(" · "),
    };
  }
  return { badge: "模板", text: p.payload_template };
}

/** 树选择: 系统层与分组层, group 为 null 表示该系统下全部 */
interface TreeSel {
  systemId: "all" | number;
  group: { id: number; name: string } | null;
}

export function ProtocolManager() {
  const [protocols, setProtocols] = useState<Protocol[]>([]);
  const [systems, setSystems] = useState<ProtocolSystem[]>([]);
  const [groups, setGroups] = useState<ProtocolGroup[]>([]);
  const { dicts, refresh: refreshDicts } = useDictionaryCatalog();
  const [dialogOpen, setDialogOpen] = useState(false);
  const [form, setForm] = useState<FormState>(emptyForm);
  const [saving, setSaving] = useState(false);
  const [batchOpen, setBatchOpen] = useState<null | "plain" | "branch">(null);
  /** 右侧表格消息类型过滤 */
  const [typeFilter, setTypeFilter] = useState<"all" | MsgType>("all");
  /** XML 在线编辑 */
  const [xmlOpen, setXmlOpen] = useState(false);
  const [batchText, setBatchText] = useState("");
  const [batchWhenField, setBatchWhenField] = useState("");
  const [batchWhenValue, setBatchWhenValue] = useState("");
  const [sel, setSel] = useState<TreeSel>({ systemId: "all", group: null });
  const [headersDirty, setHeadersDirty] = useState(false);
  const [pendingTreeSelection, setPendingTreeSelection] = useState<TreeSel | null>(null);
  const [savingBeforeSwitch, setSavingBeforeSwitch] = useState(false);
  const headerSaveRef = useRef<(() => Promise<boolean>) | null>(null);
  const selectTree = (next: TreeSel) => {
    if (headersDirty && (next.group?.id !== sel.group?.id || next.systemId !== sel.systemId)) {
      setPendingTreeSelection(next);
      return;
    }
    setSel(next);
  };
  const saveHeadersAndSwitch = async () => {
    setSavingBeforeSwitch(true);
    try {
      if (await headerSaveRef.current?.()) {
        if (pendingTreeSelection) setSel(pendingTreeSelection);
        setPendingTreeSelection(null);
      }
    } finally { setSavingBeforeSwitch(false); }
  };

  // 分页状态: 每页条数支持「自动」= 按窗口高度算满屏不出现滚动条
  const [page, setPage] = useState(1);
  const [pageSizeRaw, setPageSizeRaw] = useState<number | "auto">("auto");
  const [autoSize, setAutoSize] = useState(15);
  const tableScrollRef = useRef<HTMLDivElement>(null);

  /** 使用实际表格空间与数据行高度；空状态行不能作为每条数据的高度。 */
  const calcAutoSize = useCallback(() => {
    const wrap = tableScrollRef.current;
    if (!wrap || !wrap.clientHeight) return;
    const tr = wrap.querySelector("tbody tr:not(:has(td[colspan]))");
    const header = wrap.querySelector("thead");
    const rowHeight = tr?.getBoundingClientRect().height || 53;
    const available =
      wrap.clientHeight -
      (header?.getBoundingClientRect().height || 40) -
      2;
    setAutoSize(Math.max(1, Math.min(100, Math.floor(available / rowHeight))));
  }, []);

  useEffect(() => {
    calcAutoSize();
    const wrap = tableScrollRef.current;
    if (!wrap) return;
    const observer = new ResizeObserver(calcAutoSize);
    observer.observe(wrap);
    return () => observer.disconnect();
  }, [calcAutoSize, sel, protocols, typeFilter]);

  const pageSize = pageSizeRaw === "auto" ? autoSize : pageSizeRaw;

  // 树折叠: 一级(系统)节点收起/展开
  const [collapsed, setCollapsed] = useState<Set<number>>(new Set());
  const toggleSys = (id: number) =>
    setCollapsed((prev) => {
      const n = new Set(prev);
      if (n.has(id)) n.delete(id);
      else n.add(id);
      return n;
    });

  // 系统/分组编辑弹窗
  const [sysOpen, setSysOpen] = useState(false);
  const [sysEditing, setSysEditing] = useState<ProtocolSystem | null>(null);
  const [sysName, setSysName] = useState("");
  const [sysDesc, setSysDesc] = useState("");
  const [grpOpen, setGrpOpen] = useState(false);
  const [grpEditing, setGrpEditing] = useState<ProtocolGroup | null>(null);
  const [grpSystem, setGrpSystem] = useState<number | null>(null); // 新增分组时的归属系统
  const [grpName, setGrpName] = useState("");

  const refresh = useCallback(async (throwOnError = false) => {
    try {
      const [p, s, g] = await Promise.all([
        listProtocols(),
        listSystems(),
        listGroups(),
      ]);
      setProtocols(p);
      setSystems(s);
      setGroups(g);
    } catch (e) {
      if (throwOnError) throw e;
      toast.error("加载协议失败", { description: String(e) });
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const tree = useMemo(() => {
    return systems.map((sys) => {
      const sysGroups = groups.filter((g) => g.system_ref === sys.id);
      const counts = new Map<number, number>();
      let ungrouped = 0;
      for (const p of protocols.filter((x) => x.system_ref === sys.id)) {
        if (p.group_ref != null)
          counts.set(p.group_ref, (counts.get(p.group_ref) ?? 0) + 1);
        else ungrouped++;
      }
      return {
        sys,
        total: protocols.filter((x) => x.system_ref === sys.id).length,
        groups: sysGroups.map((g) => ({ g, cnt: counts.get(g.id) ?? 0 })),
        ungrouped,
      };
    });
  }, [systems, groups, protocols]);

  const scopeFiltered = useMemo(() => {
    if (sel.systemId === "all") return protocols;
    return protocols.filter(
      (p) =>
        p.system_ref === sel.systemId &&
        (sel.group === null || p.group_ref === sel.group.id),
    );
  }, [protocols, sel]);
  // 消息类型过滤(右侧表格)
  const filtered = useMemo(
    () =>
      typeFilter === "all"
        ? scopeFiltered
        : scopeFiltered.filter((p) => p.msg_type === typeFilter),
    [scopeFiltered, typeFilter],
  );

  const handleSave = async () => {
    const err = validate(form) || headerState.error;
    if (err) {
      toast.error(err);
      return;
    }
    setSaving(true);
    try {
      const payload = toPayload(form);
      mergeResolvedHeaderFields(headerFields, form.def_mode === "fields" ? payload.fields ?? [] : []);
      // 新建协议自动生成内部标识, 已有协议保持不变
      if (!payload.protocol_id)
        payload.protocol_id = genProtocolId(
          new Set(protocols.map((x) => x.protocol_id)),
        );
      await upsertProtocol(payload);
      toast.success(`协议「${form.name.trim()}」已保存`);
      setDialogOpen(false);
      await refresh();
    } catch (e) {
      toast.error("保存失败", { description: String(e) });
    } finally {
      setSaving(false);
    }
  };

  /** 批量文本 → 字段行: "名称 [长度] [值]"(空格/逗号/Tab 分隔), # 注释行忽略 */
  const parseBatch = (): { rows: FieldRow[]; errs: string[] } => {
    const rows: FieldRow[] = [];
    const errs: string[] = [];
    batchText.split("\n").forEach((line, i) => {
      const t = line.trim();
      if (!t || t.startsWith("#")) return;
      const parts = t.split(/[\s,，	]+/).filter(Boolean);
      const [name, lenStr, ...rest] = parts;
      if (!name) return;
      const len = Number(lenStr ?? 1);
      if (!/^[0-9]+$/.test(lenStr ?? ""))
        errs.push(`第 ${i + 1} 行: 长度「${lenStr ?? ""}」需为正整数`);
      else if (!(len >= 1 && len <= 65535))
        errs.push(`第 ${i + 1} 行: 长度 ${len} 超范围(1-65535)`);
      else
        rows.push({
          name,
          len: lenStr,
          enc: "hex",
          value: (rest.join(" ") ?? "").trim(),
          dict: "",
          role: "none",
          hdr_ref: "",
          whenField: batchOpen === "branch" ? batchWhenField.trim() : "",
          whenValue: batchOpen === "branch" ? batchWhenValue.trim() : "",
        });
    });
    return { rows, errs };
  };

  const applyBatch = () => {
    if (
      batchOpen === "branch" &&
      (!batchWhenField.trim() || !batchWhenValue.trim())
    ) {
      toast.error("按分支添加需先选条件字段并填期望值");
      return;
    }
    const { rows, errs } = parseBatch();
    if (errs.length > 0) {
      toast.error(errs[0]);
      return;
    }
    if (rows.length === 0) {
      toast.error("没有可添加的字段(每行: 名称 [长度] [值])");
      return;
    }
    setForm((f) => ({ ...f, fields: [...f.fields, ...rows] }));
    toast.success(
      batchOpen === "branch"
        ? `分支 ${batchWhenField.trim()} = ${batchWhenValue.trim()} 已添加 ${rows.length} 个字段`
        : `已添加 ${rows.length} 个字段`,
    );
    setBatchOpen(null);
    setBatchText("");
  };

  const handleDelete = async (p: Protocol) => {
    if (!window.confirm(`确定删除协议「${p.name}」?`)) return;
    try {
      await apiDeleteProtocol(p.id);
      toast.success("协议已删除");
      await refresh();
    } catch (e) {
      toast.error("删除失败", { description: String(e) });
    }
  };

  const handleSaveSystem = async () => {
    if (!sysName.trim()) {
      toast.error("请填写系统名称");
      return;
    }
    try {
      const payload = {
        name: sysName.trim(),
        description: sysDesc.trim() || null,
      };
      if (sysEditing) {
        await updateSystem(sysEditing.id, payload);
        toast.success("系统已更新");
      } else {
        await createSystem(payload);
        toast.success("系统已创建");
      }
      setSysOpen(false);
      await refresh();
    } catch (e) {
      toast.error("保存失败", { description: String(e) });
    }
  };

  const handleDeleteSystem = async (s: ProtocolSystem) => {
    if (
      !window.confirm(
        `确定删除系统「${s.name}」? 系统内有分组、协议或流程时不能删除`,
      )
    )
      return;
    try {
      await deleteSystem(s.id);
      if (sel.systemId === s.id) setSel({ systemId: "all", group: null });
      await refresh();
    } catch (e) {
      toast.error("删除失败", { description: String(e) });
    }
  };

  const handleSaveGroup = async () => {
    if (!grpName.trim()) {
      toast.error("请填写分组名称");
      return;
    }
    try {
      if (grpEditing) {
        await updateGroup(grpEditing.id, {
          system_ref: grpEditing.system_ref,
          name: grpName.trim(),
        });
        toast.success("分组已更新");
      } else if (grpSystem != null) {
        await createGroup({ system_ref: grpSystem, name: grpName.trim() });
        toast.success("分组已创建");
      }
      setGrpOpen(false);
      await refresh();
    } catch (e) {
      toast.error("保存失败", { description: String(e) });
    }
  };

  const handleDeleteGroup = async (g: ProtocolGroup) => {
    if (!window.confirm(`确定删除分组「${g.name}」? 其下协议变为未分组`))
      return;
    try {
      await deleteGroup(g.id);
      if (sel.group?.id === g.id)
        setSel({ systemId: sel.systemId, group: null });
      await refresh();
    } catch (e) {
      toast.error("删除失败", { description: String(e) });
    }
  };

  const handleImport = async () => {
    const path = await open({
      title: "选择协议 XML 文件",
      multiple: false,
      filters: [{ name: "XML", extensions: ["xml"] }],
    });
    if (!path) return;
    try {
      const r = await importProtocolsFromXml(path);
      toast.success(`导入完成: 新增 ${r.imported} 个, 更新 ${r.updated} 个`, {
        description: r.failed.length
          ? `失败 ${r.failed.length} 项: ${r.failed.join("; ")}`
          : undefined,
      });
      await refresh();
    } catch (e) {
      toast.error("导入失败", { description: String(e) });
    }
  };

  const handleExport = async () => {
    const path = await save({
      title: "导出全部协议为 XML",
      defaultPath: "protocols.xml",
      filters: [{ name: "XML", extensions: ["xml"] }],
    });
    if (!path) return;
    try {
      const n = await exportProtocolsToXml([], path);
      toast.success(`已导出 ${n} 个协议到 ${path}`);
    } catch (e) {
      toast.error("导出失败", { description: String(e) });
    }
  };

  // 行操作全部 useCallback: 引用稳定是 FieldRowItem memo 生效的前提
  const setField = useCallback((i: number, patch: Partial<FieldRow>) => {
    setForm((f) => {
      const fields = [...f.fields];
      const merged = { ...fields[i], ...patch };
      // 列表字段长度 = 单条子字段长度之和(自动推导, 杜绝"总长度与单条长度不一致")
      if (patch.units && merged.role === "device_list") {
        const sum = (
          patch.units as import("@/components/protocols/FieldRowItem").UnitRow[]
        ).reduce((acc, u) => acc + (Number(u.len) || 0), 0);
        if (sum > 0) merged.len = String(sum);
      }
      fields[i] = merged;
      return { ...f, fields };
    });
  }, []);
  const addField = useCallback(
    () =>
      setForm((f) => ({
        ...f,
        fields: [
          ...f.fields,
          {
            name: "",
            len: "1",
            enc: "hex",
            value: "",
            dict: "",
            role: "none",
            hdr_ref: "",
            whenField: "",
            whenValue: "",
          },
        ],
      })),
    [],
  );
  const removeField = useCallback(
    (i: number) =>
      setForm((f) => ({ ...f, fields: f.fields.filter((_, j) => j !== i) })),
    [],
  );
  const moveField = useCallback((i: number, dir: -1 | 1) => {
    setForm((f) => {
      const j = i + dir;
      if (j < 0 || j >= f.fields.length) return f;
      const fields = [...f.fields];
      [fields[i], fields[j]] = [fields[j], fields[i]];
      return { ...f, fields };
    });
  }, []);

  // 分页派生: 当前页数据与页数(筛选变化自动回第一页)
  const total = filtered.length;
  const totalPages = Math.max(1, Math.ceil(total / pageSize));
  const cur = Math.min(page, totalPages);
  const paged = filtered.slice((cur - 1) * pageSize, cur * pageSize);
  useEffect(() => {
    setPage(1);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sel, total]);

  /** 表单作用域内可绑定的字典: 全局共享 + 协议所属分组专属; 同 Key 时分组专属覆盖全局 */
  const scopeDicts = useMemo(() => {
    const gid = form.group_ref === "none" ? null : Number(form.group_ref);
    const byKey = new Map<string, DictSetView>();
    for (const s of dicts) {
      if (s.group_ref != null && s.group_ref !== gid) continue;
      const prev = byKey.get(s.key);
      if (!prev || prev.group_ref == null) byKey.set(s.key, s);
    }
    return [...byKey.values()];
  }, [dicts, form.group_ref]);

  /**
   * 不在当前作用域的字典（协议未归属其分组）。
   * 不参与绑定, 但在下拉中置灰展示并标注归属, 避免用户找不到已建字典却不知原因。
   */
  const outScopeDicts = useMemo(
    () =>
      dicts.filter((s) => {
        const gid = form.group_ref === "none" ? null : Number(form.group_ref);
        return s.group_ref != null && s.group_ref !== gid;
      }),
    [dicts, form.group_ref],
  );

  /** 表单所选分组对象(分组头字段下拉用) */
  const formGroup = useMemo(
    () =>
      form.group_ref === "none"
        ? null
        : (groups.find((g) => g.id === Number(form.group_ref)) ?? null),
    [groups, form.group_ref],
  );
  const headerState = useMemo(() => {
    let definitions: ReturnType<typeof groupHeaderDefinitions> = [];
    try {
      definitions = groupHeaderDefinitions(formGroup ?? undefined);
      const fields = resolveProtocolHeaders({ header_refs_json: JSON.stringify(form.header_refs) }, formGroup ?? undefined);
      let error = "";
      try { mergeResolvedHeaderFields(fields, form.def_mode === "fields" ? toPayload(form).fields : []); }
      catch (cause) { error = String(cause instanceof Error ? cause.message : cause); }
      return { definitions, fields, error };
    } catch (cause) {
      return { definitions, fields: [], error: String(cause instanceof Error ? cause.message : cause) };
    }
  }, [formGroup, form]);
  const headerFields = headerState.fields;
  const headerFieldOptions = useMemo(() => {
    const occurrences = new Map<string, number>();
    return headerFields.map(field => {
      const headerId = field.header_key!.split("::")[0];
      const headerName = headerState.definitions.find(header => header.id === headerId)?.name ?? "公共头";
      const sameHeadName = headerFields.filter(candidate => candidate.header_key?.split("::")[0] === headerId && candidate.name === field.name);
      const occurrenceKey = `${headerId}::${field.name}`;
      const occurrence = (occurrences.get(occurrenceKey) ?? 0) + 1;
      occurrences.set(occurrenceKey, occurrence);
      const matching = headerFields.filter(candidate => candidate.name === field.name);
      const bareRefTargetsFirst = matching.length === 1 || matching.every(candidate => candidate.header_key?.split("::")[0] === "legacy-default");
      return {
        value: field.header_key!,
        label: `${headerName} / ${field.name}${sameHeadName.length > 1 ? ` #${occurrence}` : ""}`,
        aliases: bareRefTargetsFirst && matching[0] === field ? [field.name] : [],
      };
    });
  }, [headerFields, headerState.definitions]);
  const frameNames = useMemo(() => dedupeNames([
    ...headerFields.map(field => field.name),
    ...form.fields.filter(field => !field.hdr_ref).map(field => field.name),
  ]), [headerFields, form.fields]);
  const headerFieldNames = frameNames.slice(0, headerFields.length);
  const bodyFieldNames = frameNames.slice(headerFields.length);
  const toggleHeader = (id: string, checked: boolean) => setForm(previous => ({
    ...previous,
    header_refs: checked ? [...previous.header_refs, id] : previous.header_refs.filter(ref => ref !== id),
  }));
  const moveHeader = (index: number, direction: -1 | 1) => setForm(previous => ({ ...previous, header_refs: arrayMove(previous.header_refs, index, index + direction) }));

  /** 绑定字典: 长度对齐字典 byte_len, 取值预填默认条目（排序最前） */
  const bindDict = useCallback(
    (i: number, key: string) => {
      const set = scopeDicts.find((s) => s.key === key);
      if (!set) return;
      setField(i, {
        dict: key,
        len: String(set.byte_len),
        value: set.items[0]?.value ?? "",
      });
    },
    [scopeDicts, setField],
  );
  /** 表单可选分组 = 所选系统下的分组 */
  const formGroups = useMemo(
    () =>
      form.system_ref === "none"
        ? []
        : groups.filter((g) => g.system_ref === Number(form.system_ref)),
    [groups, form.system_ref],
  );
  const curSys =
    sel.systemId === "all"
      ? null
      : systems.find((s) => s.id === sel.systemId);

  return (
    <div className="flex h-full p-6">
      {/* 左侧: 系统/分组树 */}
      <div className="mr-4 flex w-48 xl:w-64 flex-none flex-col rounded-xl border bg-card">
        <div className="flex-1 overflow-y-auto p-2 text-sm">
          <div className="mb-1 flex items-center gap-1">
            <button
              className={cn(
                "flex min-w-0 flex-1 items-center justify-between rounded-md px-2.5 py-1.5",
                sel.systemId === "all"
                  ? "bg-secondary text-secondary-foreground"
                  : "text-muted-foreground hover:bg-accent",
              )}
              onClick={() => selectTree({ systemId: "all", group: null })}
            >
              <span>全部协议</span>
              <span className="text-xs">{protocols.length}</span>
            </button>
            <Button
              variant="ghost"
              size="icon"
              className="h-7 w-7 shrink-0"
              title="新增系统"
              aria-label="新增系统"
              onClick={() => {
                setSysEditing(null);
                setSysName("");
                setSysDesc("");
                setSysOpen(true);
              }}
            >
              <Plus />
            </Button>
          </div>

          {tree.map(({ sys, total, groups: gs, ungrouped }) => (
            <div key={sys.id} className="mb-1">
              <div
                className={cn(
                  "group/sys flex items-center justify-between rounded-md px-2.5 py-1.5",
                  sel.systemId === sys.id && sel.group === null
                    ? "bg-secondary text-secondary-foreground"
                    : "hover:bg-accent",
                )}
              >
                <button
                  className="flex min-w-0 flex-1 items-center gap-1.5 text-left"
                  onClick={() => {
                    selectTree({ systemId: sys.id, group: null });
                    toggleSys(sys.id);
                  }}
                  title={sys.description ?? sys.name}
                >
                  {collapsed.has(sys.id) ? (
                    <ChevronRight className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                  ) : (
                    <ChevronDown className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                  )}
                  <Server className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
                  <span className="truncate">{sys.name}</span>
                </button>
                <span className="ml-2 flex shrink-0 items-center gap-0.5">
                  <span className="text-xs text-muted-foreground">{total}</span>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-6 w-6"
                    title="新增分组"
                    onClick={() => {
                      setGrpEditing(null);
                      setGrpSystem(sys.id);
                      setGrpName("");
                      setGrpOpen(true);
                    }}
                  >
                    <FolderPlus className="h-3 w-3" />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-6 w-6"
                    title="编辑系统"
                    onClick={() => {
                      setSysEditing(sys);
                      setSysName(sys.name);
                      setSysDesc(sys.description ?? "");
                      setSysOpen(true);
                    }}
                  >
                    <Pencil className="h-3 w-3" />
                  </Button>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-6 w-6 text-destructive hover:text-destructive"
                    title="删除系统"
                    onClick={() => void handleDeleteSystem(sys)}
                  >
                    <Trash2 className="h-3 w-3" />
                  </Button>
                </span>
              </div>
              {!collapsed.has(sys.id) &&
                gs.map(({ g, cnt }) => (
                  <div
                    key={g.id}
                    className={cn(
                      "group/grp ml-5 flex w-[calc(100%-1.25rem)] items-center justify-between rounded-md px-2.5 py-1 text-xs",
                      sel.systemId === sys.id && sel.group?.id === g.id
                        ? "bg-secondary text-secondary-foreground"
                        : "text-muted-foreground hover:bg-accent",
                    )}
                  >
                    <button
                      className="flex min-w-0 flex-1 items-center gap-1 text-left"
                      onClick={() =>
                        selectTree({
                          systemId: sys.id,
                          group: { id: g.id, name: g.name },
                        })
                      }
                    >
                      <ChevronRight className="h-3 w-3 shrink-0" />
                      <span className="truncate">{g.name}</span>
                    </button>
                    <span className="ml-2 flex shrink-0 items-center gap-0.5">
                      <span className="text-xs text-muted-foreground">
                        {cnt}
                      </span>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-5 w-5"
                        title="编辑分组"
                        onClick={() => {
                          setGrpEditing(g);
                          setGrpName(g.name);
                          setGrpOpen(true);
                        }}
                      >
                        <Pencil className="h-2.5 w-2.5" />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-5 w-5 text-destructive hover:text-destructive"
                        title="删除分组"
                        onClick={() => void handleDeleteGroup(g)}
                      >
                        <Trash2 className="h-2.5 w-2.5" />
                      </Button>
                    </span>
                  </div>
                ))}
              {!collapsed.has(sys.id) && ungrouped > 0 && (
                <button
                  className={cn(
                    "ml-5 flex w-[calc(100%-1.25rem)] items-center justify-between rounded-md px-2.5 py-1 text-xs",
                    sel.systemId === sys.id && sel.group === null
                      ? "bg-secondary/60 text-secondary-foreground"
                      : "text-muted-foreground/70 hover:bg-accent",
                  )}
                  onClick={() => selectTree({ systemId: sys.id, group: null })}
                >
                  <span>未分组</span>
                  <span>{ungrouped}</span>
                </button>
              )}
            </div>
          ))}


        </div>
      </div>

      {/* 右侧: 协议表 */}
      <div className="flex h-full min-w-0 flex-1 flex-col">
        <div className="mb-3 flex shrink-0 flex-wrap items-center justify-end">
          <div className="flex flex-wrap gap-2">
            <Button variant="outline" onClick={handleImport}>
              <FileUp /> 导入 .xml
            </Button>
            <Button variant="outline" onClick={handleExport}>
              <Download /> 导出全部
            </Button>
            <Button variant="outline" onClick={() => setXmlOpen(true)}>
              <FileCode /> XML 编辑
            </Button>
            <Button
              onClick={() => {
                setForm({
                  ...emptyForm,
                  system_ref:
                    sel.systemId !== "all" && sel.systemId !== -1
                      ? String(sel.systemId)
                      : "none",
                  group_ref: sel.group ? String(sel.group.id) : "none",
                });
                setDialogOpen(true);
              }}
            >
              <Plus /> 新增协议
            </Button>
          </div>
        </div>

        {/* 选中具体分组时的分组公共 UDP 头配置（默认折叠）; 公共头锚定在分组 */}
        {(() => {
          const g = sel.group
            ? groups.find((x) => x.id === sel.group!.id)
            : undefined;
          return g && curSys ? (
            <GroupHeaderCard group={g} dicts={dicts} onRefreshDicts={refreshDicts} onSaved={refresh} onDirtyChange={setHeadersDirty} saveRef={headerSaveRef} />
          ) : null;
        })()}

        <TablePanel className="flex-1">
        {/* 筛选、表格、分页共享边框和左右基线。 */}
        <TableToolbar role="group" aria-label="协议类型筛选">
          {(
            [
              ["all", "全部"],
              ["Request", "请求"],
              ["Response", "应答"],
              ["Heartbeat", "心跳"],
              ["Event", "事件"],
            ] as const
          ).map(([v, label]) => (
            <button
              key={v}
              type="button"
              aria-pressed={typeFilter === v}
              className={cn(
                "rounded-full border px-3 py-1 text-xs transition-colors",
                typeFilter === v
                  ? "border-primary bg-primary text-primary-foreground"
                  : "text-muted-foreground hover:border-primary/50 hover:text-foreground",
              )}
              onClick={() => {
                setTypeFilter(v);
                setPage(1);
              }}
            >
              {label}
            </button>
          ))}
          <span className="ml-auto text-xs text-muted-foreground">
            {filtered.length} 个
          </span>
        </TableToolbar>
            <Table aria-label="协议列表" containerRef={tableScrollRef} containerClassName="flex-1" className="min-w-[720px] whitespace-nowrap">
              <TableHeader>
                <TableRow>
                  <TableHead>名称</TableHead>
                  <TableHead>类型</TableHead>
                  <TableHead className="w-36">定义</TableHead>
                  <TableHead>自动应答</TableHead>
                  <TableHead className="sticky right-0 w-24 bg-muted text-right">
                    操作
                  </TableHead>
                </TableRow>
              </TableHeader>
              {/* key 绑定页码/条数: 行数变化时重建表体, 避免浏览器沿用旧行高布局 */}
              <TableBody key={`${cur}-${pageSize}`}>
                {filtered.length === 0 && (
                  <TableRow>
                    <TableCell
                      colSpan={5}
                      className="h-24 text-center text-muted-foreground"
                    >
                      当前筛选下暂无协议
                    </TableCell>
                  </TableRow>
                )}
                {paged.map((p) => {
                  const def = defPreview(p);
                  const resp = protocols.find(
                    (x) => x.protocol_id === p.auto_respond,
                  );
                  return (
                    <TableRow key={p.id}>
                      <TableCell className="font-medium">{p.name}</TableCell>
                      <TableCell>
                        <Badge variant={typeBadgeVariant[p.msg_type]}>
                          {msgTypeLabel[p.msg_type]}
                        </Badge>
                      </TableCell>
                      <TableCell>
                        <Badge
                          variant={
                            p.def_mode === "fields" ? "default" : "secondary"
                          }
                        >
                          {def.badge}
                        </Badge>
                      </TableCell>
                      <TableCell className="text-xs">
                        {p.auto_respond ? (
                          resp ? (
                            <span>{resp.name}</span>
                          ) : (
                            <span
                              className="text-destructive"
                              title={`应答协议 ${p.auto_respond} 不存在`}
                            >
                              应答协议不存在
                            </span>
                          )
                        ) : (
                          <span className="text-muted-foreground">-</span>
                        )}
                      </TableCell>
                      <TableCell className="sticky right-0 bg-card text-right">
                        <div className="flex justify-end gap-1">
                          <Button
                            variant="ghost"
                            size="icon"
                            onClick={() => {
                              try {
                                setForm(toForm(p, groups.find(group => group.id === p.group_ref)));
                                setDialogOpen(true);
                              } catch (error) {
                                toast.error("公共头引用无效", { description: String(error) });
                              }
                            }}
                            title="编辑"
                          >
                            <Pencil />
                          </Button>
                          <Button
                            variant="ghost"
                            size="icon"
                            className="text-destructive hover:text-destructive"
                            onClick={() => handleDelete(p)}
                            title="删除"
                          >
                            <Trash2 />
                          </Button>
                        </div>
                      </TableCell>
                    </TableRow>
                  );
                })}
              </TableBody>
            </Table>
          {/* 分页 */}
          {filtered.length > 0 && (
            <Pagination
              className="shrink-0"
              label="协议"
              total={total}
              page={cur}
              pageSize={pageSize}
              size={pageSizeRaw}
              autoSize={autoSize}
              onPageChange={setPage}
              onSizeChange={(size) => {
                setPageSizeRaw(size);
                setPage(1);
              }}
            />
          )}
        </TablePanel>
      </div>

      {/* 协议编辑弹窗 */}
      <Dialog open={dialogOpen} onOpenChange={setDialogOpen}>
        <DialogContent className="flex h-[calc(100vh-3rem)] w-[min(1080px,94vw)] max-w-none flex-col">
          <DialogHeader className="shrink-0">
            <DialogTitle>协议配置</DialogTitle>
          </DialogHeader>
          {/* 弹窗主体: 空间不足时整体滚动, 字段区保有最小高度, 底栏永远钉住 */}
          <div className="protocol-editor-body -m-1 flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-1">
          <div className="protocol-form-basics shrink-0">
            <div className="grid min-w-0 gap-1">
              <Label htmlFor="p-name" className="text-xs leading-5">
                协议名称 *
              </Label>
              <Input
                id="p-name"
                className="h-8 text-xs"
                value={form.name}
                placeholder="如 注册请求"
                onChange={(e) => setForm({ ...form, name: e.target.value })}
              />
            </div>
            <div className="grid min-w-0 gap-1">
              <Label htmlFor="p-message-type" className="text-xs leading-5">
                消息类型
              </Label>
              <Select
                value={form.msg_type}
                onValueChange={(v) =>
                  setForm({ ...form, msg_type: v as MsgType })
                }
              >
                <SelectTrigger id="p-message-type" className="h-8 text-xs">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent className="min-w-[300px]">
                  <SelectItem value="Request">
                    <span className="flex w-full items-center justify-between gap-4">
                      <span>请求</span>
                    </span>
                  </SelectItem>
                  <SelectItem value="Response">
                    <span className="flex w-full items-center justify-between gap-4">
                      <span>应答</span>
                    </span>
                  </SelectItem>
                  <SelectItem value="Heartbeat">
                    <span className="flex w-full items-center justify-between gap-4">
                      <span>心跳</span>
                    </span>
                  </SelectItem>
                  <SelectItem value="Event">
                    <span className="flex w-full items-center justify-between gap-4">
                      <span>事件</span>
                    </span>
                  </SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div className="grid min-w-0 gap-1">
              <Label htmlFor="p-system" className="text-xs leading-5">
                所属系统
              </Label>
              <Select
                value={form.system_ref === "none" ? "" : form.system_ref}
                onValueChange={(v) =>
                  setForm({ ...form, system_ref: v, group_ref: "none", header_refs: [] })
                }
              >
                <SelectTrigger id="p-system" className="h-8 text-xs">
                  <SelectValue placeholder="请选择系统" />
                </SelectTrigger>
                <SelectContent>
                  {systems.map((s) => (
                    <SelectItem key={s.id} value={String(s.id)}>
                      {s.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="grid min-w-0 gap-1">
              <Label htmlFor="p-group" className="text-xs leading-5">
                所属分组
              </Label>
              <Select
                value={form.group_ref}
                onValueChange={(v) => setForm({ ...form, group_ref: v, header_refs: [] })}
                disabled={form.system_ref === "none"}
              >
                <SelectTrigger id="p-group" className="h-8 text-xs">
                  <SelectValue
                    placeholder={
                      form.system_ref === "none" ? "请先选择系统" : "（不分组）"
                    }
                  />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="none">（不分组）</SelectItem>
                  {formGroups.map((g) => (
                    <SelectItem key={g.id} value={String(g.id)}>
                      {g.name}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
            <div className="grid min-w-0 gap-1">
              <Label htmlFor="p-definition" className="text-xs leading-5">
                定义方式
              </Label>
              <Select
                value={form.def_mode}
                onValueChange={(v) =>
                  setForm({ ...form, def_mode: v as DefMode })
                }
              >
                <SelectTrigger id="p-definition" className="h-8 text-xs">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="fields">字段定义</SelectItem>
                  <SelectItem value="template">报文模板</SelectItem>
                </SelectContent>
              </Select>
            </div>
          </div>

          <fieldset className="flex shrink-0 flex-wrap items-center gap-x-3 gap-y-2 rounded-lg border bg-muted/15 px-3 py-2" aria-label="协议公共头选择">
            <div className="flex shrink-0 items-center gap-2">
              <span className="text-sm font-medium">使用公共头</span>
              <span className="text-xs text-muted-foreground">{form.header_refs.length ? `已选 ${form.header_refs.length} 套` : "不使用"}</span>
            </div>
            <div className="flex min-w-0 flex-1 flex-wrap items-center gap-2">
              {headerState.definitions.map(header => (
                <label key={header.id} className={cn("flex min-w-0 max-w-full cursor-pointer items-center gap-2 rounded-md border px-2 py-1.5 text-xs", form.header_refs.includes(header.id) ? "border-primary/60 bg-primary/10" : "bg-background hover:bg-muted")}>
                  <input type="checkbox" className="h-4 w-4 accent-primary" aria-label={`使用公共头：${header.name}`} checked={form.header_refs.includes(header.id)} onChange={event => toggleHeader(header.id, event.target.checked)} />
                  <span className="min-w-0 break-words">{header.name}</span><span className="shrink-0 text-muted-foreground">{header.fields.reduce((total, field) => total + field.len, 0)} B</span>
                </label>
              ))}
              {!headerState.definitions.length && <span className="text-xs text-muted-foreground">{formGroup ? "当前分组暂无公共头" : "请先选择分组"}</span>}
            </div>
            {form.header_refs.length > 0 && <Button variant="ghost" size="sm" className="ml-auto h-6 shrink-0 px-2 text-xs" onClick={() => setForm(previous => ({ ...previous, header_refs: [] }))}>全部取消</Button>}
            {form.header_refs.length > 1 && <div className="flex w-full flex-wrap items-center gap-1.5 border-t pt-2" aria-label="公共头拼接顺序">
              <span className="mr-1 text-xs text-muted-foreground">拼接顺序</span>
              {form.header_refs.map((id, index) => {
                const header = headerState.definitions.find(item => item.id === id);
                return <div key={id} className="flex items-center gap-1.5" data-header-order={id}>
                  {index > 0 && <span className="text-muted-foreground">→</span>}
                  <div className="flex items-center gap-1 rounded-md border bg-background py-1 pl-2 pr-1">
                    <span className="mr-1 text-xs">{index + 1}. {header?.name ?? `缺失：${id}`}</span>
                    <Button variant="ghost" size="icon" className="h-5 w-5" disabled={index === 0} aria-label={`前移公共头：${header?.name ?? id}`} onClick={() => moveHeader(index, -1)}><ArrowUp className="h-3 w-3" /></Button>
                    <Button variant="ghost" size="icon" className="h-5 w-5" disabled={index === form.header_refs.length - 1} aria-label={`后移公共头：${header?.name ?? id}`} onClick={() => moveHeader(index, 1)}><ArrowDown className="h-3 w-3" /></Button>
                  </div>
                </div>;
              })}
              <span className="text-xs text-muted-foreground">→ 净荷</span>
            </div>}
            {headerState.error && <p role="alert" className="w-full text-xs text-destructive">{headerState.error}。请重新选择公共头或调整下方的覆盖目标。</p>}
          </fieldset>

          {form.def_mode === "fields" ? (
            <div className="protocol-field-editor flex min-h-[240px] flex-[3] flex-col gap-2">
              <div className="flex shrink-0 flex-wrap items-center justify-between gap-2">
                <div className="flex items-center gap-1">
                  <Label>报文字段</Label>
                </div>
                <div className="flex items-center gap-2">
                  <Button variant="outline" size="sm" onClick={addField}>
                    <Plus /> 添加字段
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => setBatchOpen("plain")}
                  >
                    <Plus /> 批量添加
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => setBatchOpen("branch")}
                  >
                    <GitBranch /> 按分支添加
                  </Button>
                </div>
              </div>
              {/* 字段表在自身区域内滚动: 表头吸顶, 弹窗其余部分固定 */}
              <div className="min-h-0 flex-1 overflow-auto rounded-md border">
                <div className="min-w-[976px]">
                <div className="sticky top-0 z-10 border-b bg-background px-2">
                  <div
                    className={`${FIELD_ROW_GRID} bg-muted/60 py-1.5 text-xs font-medium text-muted-foreground`}
                  >
                    <span />
                    <span>序号</span>
                    <span>字段名</span>
                    <span>长度(字节)</span>
                    <span>字典</span>
                    <span>默认值 / 条目结构</span>
                    <span>字段用途</span>
                    <span>覆盖公共头</span>
                    <span className="text-right">操作</span>
                  </div>
                </div>
                <div className="p-2">
                  {form.fields.length === 0 && (
                    <p className="py-4 text-center text-sm text-muted-foreground">
                      暂无字段
                    </p>
                  )}
                  <SortableContainer
                    count={form.fields.length}
                    onReorder={(from, to) =>
                      setForm((f) => ({
                        ...f,
                        fields: arrayMove(f.fields, from, to),
                      }))
                    }
                    renderOverlay={(i) => {
                      const r = form.fields[i];
                      if (!r) return null;
                      return (
                        <div
                          className={`${FIELD_ROW_GRID} rounded-md border bg-card p-2 shadow-xl`}
                        >
                          <span />
                          <span className="text-right text-xs text-muted-foreground">
                            {i + 1}
                          </span>
                          <span className="truncate text-sm">
                            {r.name || "（未命名字段）"}
                          </span>
                          <span className="text-xs text-muted-foreground">
                            {r.len}B
                          </span>
                          <span className="truncate text-xs text-muted-foreground">
                            {r.dict || ""}
                          </span>
                          <span className="truncate font-mono text-xs">
                            {r.value || "—"}
                          </span>
                          <span />
                          <span />
                          <span />
                        </div>
                      );
                    }}
                    className="space-y-1.5"
                  >
                    <FieldBranches
                      fields={form.fields}
                      conditionFields={headerFields.map((field, index) => ({ ...field, name: headerFieldNames[index] }))}
                      onBranchChange={(indices, condition) =>
                        setForm((f) => ({
                          ...f,
                          fields: f.fields.map((r, i) =>
                            indices.includes(i)
                              ? {
                                  ...r,
                                  whenField: condition.field,
                                  whenValue: condition.value,
                                }
                              : r,
                          ),
                        }))
                      }
                      renderField={(i) => (
                        <FieldRowItem
                          key={i}
                          id={`${i}`}
                          r={form.fields[i]}
                          index={i}
                          count={form.fields.length}
                          scopeDicts={scopeDicts}
                          outScopeDicts={outScopeDicts}
                          onRefreshDicts={refreshDicts}
                          headerFieldOptions={headerFieldOptions}
                          onPatch={setField}
                          onBindDict={bindDict}
                          onMove={moveField}
                          onRemove={removeField}
                        />
                      )}
                    />
                  </SortableContainer>
                </div>
                </div>
              </div>
            </div>
          ) : (
            <div className="flex shrink-0 items-center gap-2">
              <Label htmlFor="p-tpl" className="shrink-0">
                报文模板 *
              </Label>
              <Textarea
                id="p-tpl"
                className="min-h-20 font-mono text-xs"
                value={form.payload_template}
                placeholder={
                  form.format === "hex"
                    ? "FF 00 01 {node_id} {timestamp}"
                    : "LOGIN {node_id}"
                }
                onChange={(e) =>
                  setForm({ ...form, payload_template: e.target.value })
                }
              />
            </div>
          )}

          <div className="protocol-form-automation shrink-0">
            <div className="grid min-w-0 gap-1">
              <Label htmlFor="p-response" className="text-xs leading-5">
                自动应答协议
              </Label>
              <Select
                value={
                  protocols.some((x) => x.protocol_id === form.auto_respond)
                    ? form.auto_respond
                    : NO_RESP
                }
                onValueChange={(v) =>
                  setForm({ ...form, auto_respond: v === NO_RESP ? "" : v })
                }
              >
                <SelectTrigger id="p-response" className="h-8 text-xs">
                  <SelectValue placeholder="（不自动应答）" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={NO_RESP}>（不自动应答）</SelectItem>
                  {protocols
                    .filter((x) => x.protocol_id !== form.protocol_id)
                    .map((x) => (
                      <SelectItem key={x.id} value={x.protocol_id}>
                        {[x.system_name, x.group_name, x.name]
                          .filter(Boolean)
                          .join(" / ")}
                      </SelectItem>
                    ))}
                </SelectContent>
              </Select>
            </div>
            <div className="grid min-w-0 gap-1">
              <Label htmlFor="p-delay" className="text-xs leading-5">
                应答延迟 (毫秒)
              </Label>
              <Input
                id="p-delay"
                className="h-8 text-xs"
                type="number"
                value={form.respond_delay_ms}
                onChange={(e) =>
                  setForm({ ...form, respond_delay_ms: e.target.value })
                }
              />
            </div>
            <div className="grid min-w-0 gap-1">
              <Label htmlFor="p-periodic" className="text-xs leading-5">
                触发定时发送
              </Label>
              <Select
                value={
                  protocols.some((x) => x.protocol_id === form.auto_periodic)
                    ? form.auto_periodic
                    : NO_PERIODIC
                }
                onValueChange={(v) =>
                  setForm({
                    ...form,
                    auto_periodic: v === NO_PERIODIC ? "" : v,
                  })
                }
              >
                <SelectTrigger id="p-periodic" className="h-8 text-xs">
                  <SelectValue placeholder="（不触发）" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={NO_PERIODIC}>（不触发）</SelectItem>
                  {protocols
                    .filter((x) => x.protocol_id !== form.protocol_id)
                    .map((x) => (
                      <SelectItem key={x.id} value={x.protocol_id}>
                        {[x.system_name, x.group_name, x.name]
                          .filter(Boolean)
                          .join(" / ")}
                      </SelectItem>
                    ))}
                </SelectContent>
              </Select>
            </div>
            <div className="grid min-w-0 gap-1">
              <Label htmlFor="p-pint" className="text-xs leading-5">
                发送间隔 (毫秒)
              </Label>
              <Input
                id="p-pint"
                className="h-8 text-xs"
                type="number"
                value={form.auto_periodic_interval_ms}
                disabled={form.auto_periodic === ""}
                onChange={(e) =>
                  setForm({
                    ...form,
                    auto_periodic_interval_ms: e.target.value,
                  })
                }
              />
            </div>
          </div>

          {/* 字段断言匹配: 全部满足才命中; 与字段表分区各自收缩、内部滚动 */}
          <div
            className={`flex flex-col gap-2 ${form.match_fields.length ? "min-h-[96px] flex-1" : "shrink-0"}`}
          >
            <div className="flex items-center justify-between">
              <Label className="text-xs">字段匹配条件</Label>
              <Button
                variant="outline"
                size="sm"
                onClick={() =>
                  setForm((f) => ({
                    ...f,
                    match_fields: [...f.match_fields, { field: "", value: "" }],
                  }))
                }
              >
                <Plus /> 添加条件
              </Button>
            </div>
            {form.match_fields.length > 0 && (
              <div className="min-h-0 flex-1 overflow-y-auto rounded-md border p-2">
                {form.match_fields.map((c, i) => {
                  const setCond = (patch: Partial<CondRow>) =>
                    setForm((f) => {
                      const rows = [...f.match_fields];
                      rows[i] = { ...rows[i], ...patch };
                      return { ...f, match_fields: rows };
                    });
                  // 期望值建议: 该字段绑定字典时列出条目（含义+hex）
                  const condFieldDef = form.fields.find(
                    (x) => x.name === c.field || x.hdr_ref === c.field,
                  );
                  const condDict = condFieldDef?.dict
                    ? scopeDicts.find((x) => x.key === condFieldDef.dict)
                    : undefined;
                  const condSuggestions = condDict
                    ? condDict.items.flatMap((it) => [it.label, it.value])
                    : [];
                  return (
                    <div key={i} className="flex items-center gap-2">
                      <Select
                        value={c.field}
                        onValueChange={(v) => setCond({ field: v })}
                      >
                        <SelectTrigger className="h-8 w-64 text-xs">
                          <SelectValue placeholder="选择字段" />
                        </SelectTrigger>
                        <SelectContent className="max-h-64">
                          {headerFieldNames.map((h) => (
                            <SelectItem key={`h-${h}`} value={h}>
                              {h}（公共头）
                            </SelectItem>
                          ))}
                          {bodyFieldNames.map((nm) => (
                            <SelectItem key={`f-${nm}`} value={nm}>
                              {nm}
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                      <span className="text-xs text-muted-foreground">=</span>
                      <Input
                        className="h-8 flex-1 font-mono text-xs"
                        placeholder={
                          condDict
                            ? "期望值, 如 设备配置 / 11 / *(任意条目)"
                            : "期望值, 如 11 / 0x22 / *(存在即可)"
                        }
                        value={c.value}
                        onChange={(e) => setCond({ value: e.target.value })}
                        list={`cond-sug-${i}`}
                      />
                      <datalist id={`cond-sug-${i}`}>
                        {condDict && <option value="*" label="任意条目" />}
                        {condSuggestions.map((sg) => (
                          <option key={sg} value={sg} />
                        ))}
                      </datalist>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7 shrink-0 text-destructive hover:text-destructive"
                        onClick={() =>
                          setForm((f) => ({
                            ...f,
                            match_fields: f.match_fields.filter(
                              (_, j) => j !== i,
                            ),
                          }))
                        }
                      >
                        <Trash2 className="h-3 w-3" />
                      </Button>
                    </div>
                  );
                })}
              </div>
            )}
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <Label htmlFor="p-desc" className="shrink-0">
              描述
            </Label>
            <Input
              id="p-desc"
              className="h-9"
              value={form.description}
              onChange={(e) =>
                setForm({ ...form, description: e.target.value })
              }
            />
          </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDialogOpen(false)}>
              取消
            </Button>
            <Button onClick={handleSave} disabled={saving}>
              {saving ? "保存中..." : "保存"}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 批量添加字段弹窗 */}
      <Dialog
        open={batchOpen !== null}
        onOpenChange={(o) => !o && setBatchOpen(null)}
      >
        <DialogContent className="w-[min(720px,94vw)] max-w-none">
          <DialogHeader className="shrink-0">
            <DialogTitle>
              {batchOpen === "branch" ? "按分支添加字段" : "批量添加字段"}
            </DialogTitle>
          </DialogHeader>
          <div className="flex flex-col gap-3">
            {batchOpen === "branch" && (
              <div className="flex flex-wrap items-center gap-2">
                <Label className="shrink-0 text-xs">本分支字段当</Label>
                <Select
                  value={batchWhenField || "__none__"}
                  onValueChange={(v) =>
                    setBatchWhenField(v === "__none__" ? "" : v)
                  }
                >
                  <SelectTrigger
                    aria-label="新增分支条件字段"
                    className="h-9 w-44 text-xs"
                  >
                    <SelectValue placeholder="选择条件字段" />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="__none__">（请选择）</SelectItem>
                    {(() => {
                      const names = new Set<string>();
                      for (const f of form.fields)
                        if (f.name.trim()) names.add(f.name.trim());
                      for (const h of headerFieldNames) names.add(h);
                      return [...names].map((nm) => (
                        <SelectItem key={nm} value={nm}>
                          {nm}
                        </SelectItem>
                      ));
                    })()}
                  </SelectContent>
                </Select>
                <Label className="shrink-0 text-xs">等于</Label>
                <Input
                  className="h-9 w-48 font-mono text-xs"
                  placeholder="期望值, 如 01 / 设备配置"
                  value={batchWhenValue}
                  onChange={(e) => setBatchWhenValue(e.target.value)}
                />
              </div>
            )}
            <Textarea
              className="min-h-56 font-mono text-xs"
              placeholder={
                "设备编号 3 00 00 01\n设备序列号 4\n设备IP 4 C0 A8 00 01\n设备端口 2 1F 40\n保留 1"
              }
              value={batchText}
              onChange={(e) => setBatchText(e.target.value)}
            />
            <div className="flex items-center justify-end gap-2">
              <Button variant="outline" onClick={() => setBatchOpen(null)}>
                取消
              </Button>
              <Button onClick={applyBatch}>
                <Plus /> 添加 {batchOpen === "branch" ? "整个分支" : ""}
              </Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>

      {/* XML 在线编辑 */}
      <ProtocolXmlDialog open={xmlOpen} onOpenChange={setXmlOpen} onApplied={() => refresh(true)} />

      {/* 系统编辑弹窗 */}
      <Dialog open={sysOpen} onOpenChange={setSysOpen}>
        <DialogContent className="max-w-md" aria-describedby={undefined}>
          <DialogHeader>
            <DialogTitle>{sysEditing ? "编辑系统" : "新增系统"}</DialogTitle>
          </DialogHeader>
          <div className="grid gap-4">
            <div className="grid gap-2">
              <Label htmlFor="s-name">系统名称 *</Label>
              <Input
                id="s-name"
                value={sysName}
                placeholder="如 物联网协议"
                onChange={(e) => setSysName(e.target.value)}
              />
            </div>
            <div className="grid gap-2">
              <Label htmlFor="s-desc">描述</Label>
              <Input
                id="s-desc"
                value={sysDesc}
                placeholder="可选"
                onChange={(e) => setSysDesc(e.target.value)}
              />
            </div>
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setSysOpen(false)}>
              取消
            </Button>
            <Button onClick={handleSaveSystem}>保存</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={pendingTreeSelection !== null} onOpenChange={open => { if (!open && !savingBeforeSwitch) setPendingTreeSelection(null); }}>
        <DialogContent className="max-w-md">
          <DialogHeader>
            <DialogTitle>公共头尚未保存</DialogTitle>
            <DialogDescription>切换前保存当前修改？</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" disabled={savingBeforeSwitch} onClick={() => setPendingTreeSelection(null)}>继续编辑</Button>
            <Button variant="ghost" disabled={savingBeforeSwitch} onClick={() => {
              if (pendingTreeSelection) setSel(pendingTreeSelection);
              setPendingTreeSelection(null);
              setHeadersDirty(false);
            }}>放弃并切换</Button>
            <Button disabled={savingBeforeSwitch} onClick={saveHeadersAndSwitch}>{savingBeforeSwitch ? "保存中…" : "保存并切换"}</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* 分组编辑弹窗 */}
      <Dialog open={grpOpen} onOpenChange={setGrpOpen}>
        <DialogContent className="max-w-sm">
          <DialogHeader>
            <DialogTitle>{grpEditing ? "编辑分组" : "新增分组"}</DialogTitle>
            <DialogDescription>
              归属系统：
              {grpEditing
                ? (systems.find((s) => s.id === grpEditing.system_ref)?.name ??
                  "-")
                : (systems.find((s) => s.id === grpSystem)?.name ?? "-")}
            </DialogDescription>
          </DialogHeader>
          <div className="grid gap-2">
            <Label htmlFor="g-name">分组名称 *</Label>
            <Input
              id="g-name"
              value={grpName}
              placeholder="如 设备 / 网关"
              onChange={(e) => setGrpName(e.target.value)}
            />
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setGrpOpen(false)}>
              取消
            </Button>
            <Button onClick={handleSaveGroup}>保存</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
