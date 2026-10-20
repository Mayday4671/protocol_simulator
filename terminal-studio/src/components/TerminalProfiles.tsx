import {
  Select as UXSelect,
  SelectTrigger as UXSelectTrigger,
  SelectValue as UXSelectValue,
  SelectContent as UXSelectContent,
  SelectItem as UXSelectItem,
} from "@/components/ui/select";
import {
  Fragment,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";
import { listen } from "@tauri-apps/api/event";
import {
  ChevronDown,
  ChevronRight,
  Pencil,
  Plus,
  Search,
  Trash2,
  X,
} from "lucide-react";
import { toast } from "sonner";
import {
  deleteTerminal,
  listDictSets,
  listGroups,
  listNodes,
  listProtocols,
  listSystems,
  listTerminals,
  saveTerminal,
} from "@/lib/api";
import {
  decodeRecordHex,
  deviceFormProperties,
  deviceFormValues,
  deviceSections,
  fieldIsActive,
  parseProperties,
  protocolFrame,
} from "@/lib/deviceProfiles";
import type {
  DictSetView,
  Node,
  Protocol,
  ProtocolField,
  ProtocolGroup,
  ProtocolSystem,
  TerminalProfile,
} from "@/types";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Pagination } from "@/components/ui/pagination";
import { FieldBranches } from "@/components/protocols/FieldBranches";
import { ListEntries } from "@/components/protocols/ListEntries";
import {
  useWorkspaceCloseGuard,
  WorkspaceActivityContext,
} from "@/components/layout/WorkspaceActivity";
const selectClass = "h-9";
function PropertyDetails({
  profile,
  protocols,
  groups,
  dicts,
}: {
  profile: TerminalProfile;
  protocols: Protocol[];
  groups: ProtocolGroup[];
  dicts: DictSetView[];
}) {
  const properties = parseProperties(profile.fields_json);
  const sections = deviceSections(protocols, profile.system_ref, groups, dicts)
    .map((s) => ({
      ...s,
      saved: s.fields.filter((f) => properties[f.profile_key!.trim()] !== undefined),
    }))
    // 属性键被其他分区完全覆盖的协议不重复展示, 与编辑器折叠规则一致
    .filter((s) => s.saved.length && !s.collapseByDefault);
  return (
    <div className="space-y-4 bg-muted/20 p-4">
      {sections.map((s) => (
        <div key={s.protocol.protocol_id}>
          <div className="mb-2 text-xs font-medium text-muted-foreground">
            {s.protocol.name} · {s.saved.length} 项
          </div>
          <div className="grid gap-x-6 gap-y-3 md:grid-cols-3">
            {s.saved.map((f) => {
              const value = properties[f.profile_key!.trim()];
              if (f.role === "device_list" && f.unit_fields?.length) {
                let records: Record<string, string>[] = [];
                try {
                  records = decodeRecordHex(f.unit_fields, value);
                } catch {
                  /* Display the persisted raw value when a schema has changed. */
                }
                return (
                  <details key={f.name} className="col-span-full rounded border p-2">
                    <summary className="cursor-pointer text-sm">
                      {f.name} · {records.length} 条
                    </summary>
                    <div className="mt-2 max-h-64 overflow-auto">
                      <table className="w-full text-left text-xs">
                        <thead>
                          <tr>
                            {f.unit_fields.map((u) => (
                              <th key={u.name} className="px-2 py-1">
                                {u.name}
                              </th>
                            ))}
                          </tr>
                        </thead>
                        <tbody>
                          {records.map((row, i) => (
                            <tr key={i} className="border-t">
                              {f.unit_fields!.map((u) => (
                                <td key={u.name} className="px-2 py-1 font-mono">
                                  {row[u.name]}
                                  {dictLabel(s.dicts, u, row[u.name]) &&
                                    ` · ${dictLabel(s.dicts, u, row[u.name])}`}
                                </td>
                              ))}
                            </tr>
                          ))}
                        </tbody>
                      </table>
                      {!records.length && value && (
                        <code className="break-all">{value}</code>
                      )}
                    </div>
                  </details>
                );
              }
              return (
                <div key={f.name} className="min-w-0 text-sm">
                  <span className="text-xs text-muted-foreground">{f.name}</span>
                  <div className="break-all font-mono">
                    {value}
                    {dictLabel(s.dicts, f, value) && (
                      <span className="ml-2 font-sans text-muted-foreground">
                        {dictLabel(s.dicts, f, value)}
                      </span>
                    )}
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      ))}
      {!sections.length && (
        <span className="text-sm text-muted-foreground">暂无配置属性</span>
      )}
    </div>
  );
}
function dictLabel(dicts: DictSetView[], f: ProtocolField, value: string) {
  return dicts
    .find((d) => d.key === f.dict)
    ?.items.find((i) => i.value === value)?.label;
}
function DeviceEditor({
  initial,
  nodes,
  protocols,
  groups,
  systems,
  dicts,
  onCancel,
  onSaved,
}: {
  initial: TerminalProfile;
  nodes: Node[];
  protocols: Protocol[];
  groups: ProtocolGroup[];
  systems: ProtocolSystem[];
  dicts: DictSetView[];
  onCancel: () => void;
  onSaved: () => Promise<void>;
}) {
  const [draft, setDraft] = useState(initial);
  const [saving, setSaving] = useState(false);
  const [revision, setRevision] = useState(0);
  /** 设备属性为唯一事实来源: 同一 Key 跨协议分区自动同值 */
  const [properties, setProperties] = useState<Record<string, string>>(() =>
    parseProperties(initial.fields_json),
  );
  const [listErrors, setListErrors] = useState<Record<string, string>>({});
  const [openMap, setOpenMap] = useState<Record<string, boolean>>({});
  const sections = useMemo(
    () => deviceSections(protocols, draft.system_ref, groups, dicts),
    [protocols, draft.system_ref, groups, dicts],
  );
  const sectionValues = useMemo(() => {
    const map: Record<string, Record<string, string>> = {};
    for (const s of sections)
      map[s.protocol.protocol_id] = deviceFormValues(
        s.frame,
        properties,
        s.dicts,
      );
    return map;
  }, [sections, properties]);
  useWorkspaceCloseGuard(true);
  const patchProperty = useCallback(
    (field: ProtocolField, value: string) =>
      setProperties((prev) =>
        prev[field.profile_key!.trim()] === value
          ? prev
          : { ...prev, [field.profile_key!.trim()]: value },
      ),
    [],
  );
  const save = async () => {
    try {
      if (!draft.name.trim() || !draft.node_ref)
        throw new Error("请填写名称和通道");
      if (!sections.length)
        throw new Error("该协议系统内尚无协议绑定设备属性，请先在协议字段设置中绑定");
      const key = draft.device_key.replace(/\s+/g, "").toUpperCase();
      const identitySections = sections.filter((s) =>
        s.frame.some(
          (f) => f.role === "src_id" || f.role === "device_sn",
        ),
      );
      if (!identitySections.length)
        throw new Error("该系统内协议需声明源设备或设备序列号字段");
      for (const s of identitySections) {
        const identity =
          s.frame.find((f) => f.role === "src_id") ??
          s.frame.find((f) => f.role === "device_sn")!;
        if (!/^[0-9A-F]+$/.test(key) || key.length !== identity.len * 2)
          throw new Error(
            `协议「${s.protocol.name}」的设备标识需要 ${identity.len} 字节 Hex`,
          );
      }
      const merged: Record<string, string> = {};
      let conflict = "";
      for (const s of sections) {
        const pid = s.protocol.protocol_id;
        const values = sectionValues[pid] ?? {};
        const invalid = Object.entries(listErrors).find(
          ([name, err]) => err && name.startsWith(`${pid}:`),
        );
        if (invalid) throw new Error(`「${s.protocol.name}」${invalid[1]}`);
        let partial: Record<string, string>;
        try {
          partial = deviceFormProperties(s.frame, values, s.dicts);
        } catch (e) {
          throw new Error(`「${s.protocol.name}」${String(e)}`);
        }
        for (const [k, v] of Object.entries(partial)) {
          if (merged[k] !== undefined && merged[k] !== v && !conflict)
            conflict = `设备属性 Key「${k}」在多套协议中的取值不一致，请统一后保存`;
          merged[k] = v;
        }
      }
      if (conflict) throw new Error(conflict);
      setSaving(true);
      await saveTerminal({
        ...draft,
        name: draft.name.trim(),
        device_key: key,
        protocol_ref: sections[0].protocol.protocol_id,
        fields_json: JSON.stringify(merged),
      });
      await onSaved();
      toast.success("设备已保存");
    } catch (e) {
      toast.error(String(e));
    } finally {
      setSaving(false);
    }
  };
  const renderSectionField = (
    s: (typeof sections)[number],
    values: Record<string, string>,
    i: number,
  ) => {
    const f = s.fields[i];
    const pid = s.protocol.protocol_id;
    const count = s.frame
      .slice(0, s.frame.indexOf(f))
      .reverse()
      .find(
        (c) =>
          c.role === "device_count" &&
          fieldIsActive(c, s.frame, values, s.dicts),
      );
    if (f.role === "device_list" && f.unit_fields?.length)
      return (
        <ListEntries
          field={f}
          count={count}
          countValue={count ? (values[count.name] ?? count.value) : ""}
          initialValue={values[f.name]}
          resetKey={`${pid}:${revision}`}
          dicts={s.dicts}
          onChange={(name, value) => {
            if (name.startsWith("__invalid_list_"))
              setListErrors((prev) =>
                prev[`${pid}:${f.name}`] === value
                  ? prev
                  : { ...prev, [`${pid}:${f.name}`]: value },
              );
            else patchProperty(f, value);
          }}
        />
      );
    const items = s.dicts.find((d) => d.key === f.dict)?.items ?? [];
    return (
      <label className="grid min-w-0 gap-1 text-xs">
        {f.name}
        {f.dict ? (
          <UXSelect
            value={String(values[f.name] ?? "") || "__empty__"}
            onValueChange={(value) =>
              patchProperty(f, value === "__empty__" ? "" : value)
            }
          >
            <UXSelectTrigger aria-label={f.name} className={selectClass}>
              <UXSelectValue />
            </UXSelectTrigger>
            <UXSelectContent>
              <UXSelectItem value={String("") || "__empty__"} disabled>
                选择条目
              </UXSelectItem>
              {!items.some((item) => item.value === values[f.name]) &&
                values[f.name] && (
                  <UXSelectItem value={String(values[f.name]) || "__empty__"}>
                    {values[f.name]} · 无效字典项
                  </UXSelectItem>
                )}
              {items.map((item) => (
                <UXSelectItem key={item.id} value={String(item.value) || "__empty__"}>
                  {item.value} · {item.label}
                </UXSelectItem>
              ))}
            </UXSelectContent>
          </UXSelect>
        ) : (
          <Input
            aria-label={f.name}
            className="h-9 font-mono text-sm"
            value={values[f.name] ?? ""}
            onChange={(e) => patchProperty(f, e.target.value)}
          />
        )}
      </label>
    );
  };
  return (
    <section
      aria-label="设备配置表单"
      className="flex min-h-0 flex-1 flex-col rounded-lg border bg-card"
    >
      <div className="flex shrink-0 items-center justify-between border-b px-4 py-3">
        <h2 className="font-medium">{draft.id ? "编辑设备" : "新建设备"}</h2>
        <Button
          variant="ghost"
          size="icon"
          aria-label="取消设备编辑"
          onClick={onCancel}
        >
          <X className="h-4 w-4" />
        </Button>
      </div>
      <div className="grid shrink-0 gap-3 p-4 md:grid-cols-2 xl:grid-cols-3">
        <label className="grid gap-1 text-xs">
          设备名称
          <Input
            aria-label="设备名称"
            value={draft.name}
            onChange={(e) => setDraft((d) => ({ ...d, name: e.target.value }))}
          />
        </label>
        <label className="grid gap-1 text-xs">
          通道
          <UXSelect
            disabled={!!draft.id}
            value={String(draft.node_ref || "") || "__empty__"}
            onValueChange={(value) =>
              setDraft((d) => ({
                ...d,
                node_ref: Number(value === "__empty__" ? "" : value),
              }))
            }
          >
            <UXSelectTrigger aria-label="设备通道" className={selectClass}>
              <UXSelectValue />
            </UXSelectTrigger>
            <UXSelectContent>
              <UXSelectItem value={String("") || "__empty__"} disabled>
                选择通道
              </UXSelectItem>
              {nodes.map((n) => (
                <UXSelectItem key={n.id} value={String(n.id) || "__empty__"}>
                  {n.name}
                </UXSelectItem>
              ))}
            </UXSelectContent>
          </UXSelect>
        </label>
        <label className="grid gap-1 text-xs">
          协议系统
          <UXSelect
            disabled={!!draft.id}
            value={String(draft.system_ref) || "__empty__"}
            onValueChange={(value) => {
              setDraft((d) => ({
                ...d,
                system_ref: Number(value === "__empty__" ? "" : value),
              }));
              setRevision((r) => r + 1);
            }}
          >
            <UXSelectTrigger aria-label="设备协议系统" className={selectClass}>
              <UXSelectValue />
            </UXSelectTrigger>
            <UXSelectContent>
              <UXSelectItem value={String("-1") || "__empty__"} disabled>
                选择系统
              </UXSelectItem>
              {systems.map((s) => (
                <UXSelectItem key={s.id} value={String(s.id) || "__empty__"}>
                  {s.name}
                </UXSelectItem>
              ))}
            </UXSelectContent>
          </UXSelect>
        </label>
        <label className="grid gap-1 text-xs">
          设备标识 Hex
          <Input
            aria-label="设备标识 Hex"
            className="font-mono"
            disabled={!!draft.id}
            value={draft.device_key}
            onChange={(e) =>
              setDraft((d) => ({ ...d, device_key: e.target.value }))
            }
          />
        </label>
        <label className="grid gap-1 text-xs">
          Session
          <Input
            aria-label="设备 Session"
            className="font-mono"
            readOnly
            value={initial.session_hex ?? ""}
            placeholder="未注册"
          />
        </label>
      </div>
      <div className="min-h-0 flex-1 space-y-3 overflow-y-auto border-t p-4">
        <p className="text-xs text-muted-foreground">
          同一属性 Key 在各协议分区中共享一个值；不同协议各自的字段在此分别配置。
        </p>
        {sections.map((s) => {
          const pid = s.protocol.protocol_id;
          const values = sectionValues[pid] ?? {};
          const open = openMap[pid] ?? !s.collapseByDefault;
          return (
            <section
              key={pid}
              aria-label={`协议分区：${s.protocol.name}`}
              className="rounded-md border"
            >
              <div className="flex items-center gap-2 border-b bg-muted/20 px-3 py-2">
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-7 w-7"
                  aria-label={`${open ? "收起" : "展开"}协议分区：${s.protocol.name}`}
                  aria-expanded={open}
                  onClick={() => setOpenMap((prev) => ({ ...prev, [pid]: !open }))}
                >
                  {open ? (
                    <ChevronDown className="h-4 w-4" />
                  ) : (
                    <ChevronRight className="h-4 w-4" />
                  )}
                </Button>
                <span className="text-sm font-medium">
                  {s.protocol.name}
                </span>
                <span className="text-xs text-muted-foreground">
                  {s.keys.length} 项设备属性
                </span>
              </div>
              {open && (
                <div className="p-3">
                  <FieldBranches
                    key={`${pid}:${revision}`}
                    fields={s.fields.map((f) => ({
                      name: f.name,
                      value: values[f.name],
                      whenField: f.when?.field,
                      whenValue: f.when?.value,
                    }))}
                    fullWidth={(i) =>
                      s.fields[i].role === "device_list" || s.fields[i].len > 32
                    }
                    activeWhen={(i) =>
                      fieldIsActive(s.fields[i], s.frame, values, s.dicts)
                    }
                    renderField={(i) => renderSectionField(s, values, i)}
                  />
                </div>
              )}
            </section>
          );
        })}
        {!sections.length && (
          <p className="p-2 text-sm text-muted-foreground">
            该协议系统内尚无协议绑定设备属性，请先在协议管理的字段设置中绑定设备属性
            Key。
          </p>
        )}
      </div>
      <div className="flex shrink-0 gap-2 border-t px-4 py-3">
        <Button disabled={saving || !sections.length} onClick={() => void save()}>
          保存设备
        </Button>
        <Button variant="outline" onClick={onCancel}>
          取消
        </Button>
      </div>
    </section>
  );
}
export function TerminalProfiles() {
  const { active } = useContext(WorkspaceActivityContext);
  const [rows, setRows] = useState<TerminalProfile[]>([]);
  const [nodes, setNodes] = useState<Node[]>([]);
  const [protocols, setProtocols] = useState<Protocol[]>([]);
  const [groups, setGroups] = useState<ProtocolGroup[]>([]);
  const [systems, setSystems] = useState<ProtocolSystem[]>([]);
  const [dicts, setDicts] = useState<DictSetView[]>([]);
  const [draft, setDraft] = useState<TerminalProfile | null>(null);
  const [query, setQuery] = useState("");
  const [system, setSystem] = useState("all");
  const [page, setPage] = useState(1);
  const [pageSize, setPageSize] = useState(10);
  const [expanded, setExpanded] = useState<Set<number>>(new Set());
  const reload = useCallback(async () => {
    const [t, n, p, g, s, d] = await Promise.all([
      listTerminals(),
      listNodes(),
      listProtocols(),
      listGroups(),
      listSystems(),
      listDictSets(),
    ]);
    setRows(t);
    setNodes(n);
    setProtocols(p);
    setGroups(g);
    setSystems(s);
    setDicts(d);
  }, []);
  useEffect(() => {
    if (active) void reload().catch((e) => toast.error(String(e)));
  }, [active, reload]);
  useEffect(() => {
    const update = () => {
      void reload().catch((e) => toast.error(String(e)));
    };
    const listeners = [
      listen("devices-updated", update),
      listen("session-updated", update),
    ];
    return () => {
      listeners.forEach((p) => {
        void p.then((unlisten) => unlisten());
      });
    };
  }, [reload]);
  /** 可由设备发送且声明了设备属性的非应答协议; 设备表单按系统覆盖这些协议的并集 */
  const eligible = useMemo(
    () =>
      protocols.filter(
        (p) =>
          p.msg_type !== "Response" &&
          p.def_mode === "fields" &&
          protocolFrame(
            p,
            groups.find((g) => g.id === p.group_ref),
          ).fields.some((f) => f.profile_key?.trim()),
      ),
    [protocols, groups],
  );
  const edit = (profile: TerminalProfile) => {
    setDraft({ ...profile });
  };
  const create = () => {
    const source = eligible.find(
      (p) => system === "all" || String(p.system_ref) === system,
    );
    setDraft({
      id: 0,
      name: "",
      node_ref: nodes[0]?.id ?? 0,
      system_ref: source?.system_ref ?? -1,
      protocol_ref: source?.protocol_id ?? "",
      device_key: "",
      fields_json: "{}",
      session_hex: null,
    });
  };
  const filtered = rows.filter(
    (t) =>
      (system === "all" || String(t.system_ref) === system) &&
      `${t.name} ${t.device_key}`
        .toLowerCase()
        .includes(query.toLowerCase().trim()),
  );
  const pages = Math.max(1, Math.ceil(filtered.length / pageSize));
  const currentPage = Math.min(page, pages);
  const visible = filtered.slice(
    (currentPage - 1) * pageSize,
    currentPage * pageSize,
  );
  const remove = async (profile: TerminalProfile) => {
    if (!window.confirm(`删除设备「${profile.name}」及其会话？`)) return;
    try {
      await deleteTerminal(profile.id);
      if (draft?.id === profile.id) setDraft(null);
      await reload();
      toast.success("设备已删除");
    } catch (e) {
      toast.error(String(e));
    }
  };
  return (
    <section className="flex h-full min-h-0 flex-col gap-4">
      <div className="flex flex-wrap items-center gap-3">
        <div className="relative min-w-56 max-w-md flex-1">
          <Search className="absolute left-3 top-2.5 h-4 w-4 text-muted-foreground" />
          <Input
            aria-label="搜索设备"
            className="pl-9"
            placeholder="搜索名称 / 设备标识"
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setPage(1);
            }}
          />
        </div>
        <UXSelect
          value={String(system) || "__empty__"}
          onValueChange={(value) => {
            setSystem(value === "__empty__" ? "" : value);
            setPage(1);
          }}
        >
          <UXSelectTrigger
            aria-label="筛选设备系统"
            className="w-52 max-w-full shrink-0"
          >
            <UXSelectValue />
          </UXSelectTrigger>
          <UXSelectContent>
            <UXSelectItem value={String("all") || "__empty__"}>
              全部协议系统
            </UXSelectItem>
            {systems.map((s) => (
              <UXSelectItem key={s.id} value={String(s.id) || "__empty__"}>
                {s.name}
              </UXSelectItem>
            ))}
          </UXSelectContent>
        </UXSelect>
        <Button
          className="ml-auto"
          onClick={create}
          disabled={!eligible.length || !nodes.length}
        >
          <Plus className="h-4 w-4" />
          新建设备
        </Button>
      </div>
      {draft && (
        <DeviceEditor
          key={`${draft.id}:${draft.device_key}`}
          initial={draft}
          nodes={nodes}
          protocols={eligible}
          groups={groups}
          systems={systems}
          dicts={dicts}
          onCancel={() => setDraft(null)}
          onSaved={async () => {
            await reload();
            setDraft(null);
          }}
        />
      )}
      {!draft && (
        <div className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border bg-card">
          <div className="min-h-0 flex-1 overflow-auto">
        <table aria-label="设备档案" className="w-full text-left text-sm">
          <thead className="sticky top-0 z-10 border-b bg-card text-xs text-muted-foreground">
            <tr className="bg-muted/30">
              <th className="w-[52px] px-3" />
              <th className="px-3 py-3">设备</th>
              <th className="px-3 py-3">设备标识</th>
              <th className="px-3 py-3">协议系统 / 设备属性</th>
              <th className="px-3 py-3">通道</th>
              <th className="px-3 py-3">Session</th>
              <th className="w-24 px-3 py-3 text-right">操作</th>
            </tr>
          </thead>
          <tbody>
            {visible.map((t) => (
              <Fragment key={t.id}>
                <tr className="border-b last:border-b-0 hover:bg-muted/20">
                  <td className="px-3">
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-7 w-7"
                      aria-label={`${expanded.has(t.id) ? "收起" : "查看"}设备：${t.name}`}
                      aria-expanded={expanded.has(t.id)}
                      onClick={() =>
                        setExpanded((prev) => {
                          const next = new Set(prev);
                          if (next.has(t.id)) next.delete(t.id);
                          else next.add(t.id);
                          return next;
                        })
                      }
                    >
                      {expanded.has(t.id) ? (
                        <ChevronDown className="h-4 w-4" />
                      ) : (
                        <ChevronRight className="h-4 w-4" />
                      )}
                    </Button>
                  </td>
                  <td className="px-3 py-3 font-medium">{t.name}</td>
                  <td className="px-3 py-3 font-mono text-xs">
                    {t.device_key}
                  </td>
                  <td className="px-3 py-3">
                    <div>
                      {systems.find((s) => s.id === t.system_ref)?.name ??
                        "未分组"}
                    </div>
                    <div className="text-xs text-muted-foreground">
                      {Object.keys(parseProperties(t.fields_json)).length} 项属性
                    </div>
                  </td>
                  <td className="px-3 py-3">
                    {nodes.find((n) => n.id === t.node_ref)?.name ??
                      "通道已删除"}
                  </td>
                  <td className="px-3 py-3 font-mono text-xs">
                    {t.session_hex || (
                      <span className="font-sans text-muted-foreground">
                        未注册
                      </span>
                    )}
                  </td>
                  <td className="px-3 py-3">
                    <div className="flex justify-end">
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-8 w-8"
                        aria-label={`编辑设备：${t.name}`}
                        onClick={() => edit(t)}
                      >
                        <Pencil className="h-4 w-4" />
                      </Button>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-8 w-8 text-destructive"
                        aria-label={`删除设备：${t.name}`}
                        onClick={() => void remove(t)}
                      >
                        <Trash2 className="h-4 w-4" />
                      </Button>
                    </div>
                  </td>
                </tr>
                {expanded.has(t.id) && (
                  <tr>
                    <td colSpan={7}>
                      <PropertyDetails
                        profile={t}
                        protocols={protocols}
                        groups={groups}
                        dicts={dicts}
                      />
                    </td>
                  </tr>
                )}
              </Fragment>
            ))}
          </tbody>
        </table>
        {!visible.length && (
          <div className="p-8 text-center text-sm text-muted-foreground">
            暂无设备
          </div>
        )}
          </div>
          <Pagination
            className="shrink-0"
            label="设备"
            buttonSuffix="设备"
            total={filtered.length}
            page={currentPage}
            pageSize={pageSize}
            size={pageSize}
            onPageChange={setPage}
            onSizeChange={(size) => {
              if (size !== "auto") setPageSize(size);
              setPage(1);
            }}
          />
        </div>
      )}
    </section>
  );
}
