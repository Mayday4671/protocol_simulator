import {
  Select as UXSelect,
  SelectTrigger as UXSelectTrigger,
  SelectValue as UXSelectValue,
  SelectContent as UXSelectContent,
  SelectItem as UXSelectItem,
} from "@/components/ui/select";
/**
 * 分组公共头库。定义与协议选用相互独立；每条协议保存自己的拼接顺序。
 */
import { useEffect, useMemo, useState, type MutableRefObject } from "react";
import { useWorkspaceCloseGuard } from "@/components/layout/WorkspaceActivity";
import { toast } from "sonner";
import { ChevronDown, Copy, Plus, Save, Settings2, Trash2 } from "lucide-react";
import { arrayMove } from "@dnd-kit/sortable";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { DictionaryPicker } from "@/components/protocols/DictionaryPicker";
import { FieldValueEditor } from "@/components/protocols/FieldValueEditor";
import { dictItemLabel } from "@/components/ui/combo-input";
import { SortableContainer, SortableRow } from "@/components/ui/sortable-list";
import { updateGroupHeaders } from "@/lib/api";
import { groupHeaderDefinitions } from "@/lib/protocolHeaders";
import { cn } from "@/lib/utils";
import { FieldSectionSettings } from "@/components/protocols/FieldSectionSettings";
import type {
  DictSetView,
  HeaderDefinition,
  FieldEnc,
  FieldRole,
  ProtocolField,
  ProtocolGroup,
} from "@/types";
import {
  FIELD_ROLE_OPTIONS,
  roleDescOf,
} from "@/components/protocols/fieldRoles";
interface HeaderRow extends Omit<ProtocolField, "len" | "dict" | "role" | "profile_key"> {
  len: string;
  dict: string;
  role: FieldRole;
  profile_key?: string;
}
interface DraftHeader { id: string; name: string; fields: HeaderRow[] }
function editableHeaders(group: ProtocolGroup): DraftHeader[] {
  return groupHeaderDefinitions(group).map(header => ({
    ...header,
    fields: header.fields.map(field => ({
      ...field,
      len: String(field.len),
      dict: field.dict ?? "",
      role: field.role ?? "none",
      value: field.value ?? "",
      profile_key: field.profile_key ?? "",
    })),
  }));
}
function headerId() {
  return `h_${crypto.randomUUID().replace(/-/g, "")}`;
}
export function GroupHeaderCard({
  group,
  dicts,
  onSaved,
  onRefreshDicts,
  onDirtyChange,
  saveRef,
}: {
  group: ProtocolGroup;
  dicts: DictSetView[];
  onSaved: () => Promise<void>;
  onRefreshDicts?: () => Promise<unknown>;
  onDirtyChange?: (dirty: boolean) => void;
  saveRef?: MutableRefObject<(() => Promise<boolean>) | null>;
}) {
  const [open, setOpen] = useState(false);
  const loaded = useMemo(() => {
    try { return { headers: editableHeaders(group), error: "" }; }
    catch (error) { return { headers: [] as DraftHeader[], error: String(error instanceof Error ? error.message : error) }; }
  }, [group.id, group.header_profiles_json, group.header_fields_json, group.name]);
  const [headers, setHeaders] = useState<DraftHeader[]>(loaded.headers);
  const [selectedId, setSelectedId] = useState(() => loaded.headers[0]?.id ?? "");
  const [saving, setSaving] = useState(false);
  const [dirty, setDirty] = useState(false);
  useWorkspaceCloseGuard(dirty);
  useEffect(() => { onDirtyChange?.(dirty); }, [dirty, onDirtyChange]);
  useEffect(() => () => { onDirtyChange?.(false); }, [onDirtyChange]);
  const [settingsIndex, setSettingsIndex] = useState<number | null>(null);
  const [removeId, setRemoveId] = useState<string | null>(null);
  const selected = headers.find(header => header.id === selectedId);
  const rows = selected?.fields ?? [];
  useEffect(() => {
    const next = loaded.headers;
    setHeaders(next);
    setSelectedId(previous => next.some(header => header.id === previous) ? previous : next[0]?.id ?? "");
    setDirty(false);
    setRemoveId(null);
    setSettingsIndex(null);
  }, [loaded]);
  useEffect(() => { setOpen(false); }, [group.id]);
  const setRows = (update: (rows: HeaderRow[]) => HeaderRow[]) => {
    setHeaders(previous => previous.map(header => header.id === selectedId ? { ...header, fields: update(header.fields) } : header));
    setDirty(true);
  };
  const addHeader = (source?: DraftHeader) => {
    const header: DraftHeader = source
      ? { ...structuredClone(source), id: headerId(), name: `${source.name} 副本` }
      : { id: headerId(), name: `公共头 ${headers.length + 1}`, fields: [] };
    setHeaders(previous => [...previous, header]);
    setSelectedId(header.id);
    setSettingsIndex(null);
    setDirty(true);
  };
  const deleteHeader = (id: string) => {
    const next = headers.filter(header => header.id !== id);
    setHeaders(next);
    if (selectedId === id) setSelectedId(next[0]?.id ?? "");
    setRemoveId(null);
    setSettingsIndex(null);
    setDirty(true);
  };
  /** 作用域内字典: 全局共享 + 本分组专属, 同 Key 时专属覆盖全局 */
  const scopeDicts = useMemo(() => {
    const byKey = new Map<string, DictSetView>();
    for (const s of dicts) {
      if (s.group_ref != null && s.group_ref !== group.id) continue;
      const prev = byKey.get(s.key);
      if (!prev || prev.group_ref == null) byKey.set(s.key, s);
    }
    return [...byKey.values()];
  }, [dicts, group.id]);
  const outScopeDicts = useMemo(() => dicts.filter(dictionary => dictionary.group_ref != null && dictionary.group_ref !== group.id), [dicts, group.id]);
  const setRow = (i: number, patch: Partial<HeaderRow>) => {
    setRows((rs) => {
      const next = [...rs];
      next[i] = { ...next[i], ...patch };
      return next;
    });
  };
  const bindDict = (i: number, key: string) => {
    const set = scopeDicts.find((s) => s.key === key);
    if (!set) return;
    setRow(i, {
      dict: key,
      len: String(set.byte_len),
      value: set.items[0]?.value ?? "",
    });
  };
  const totalBytes = rows.reduce((s, r) => s + (Number(r.len) || 0), 0);
  const handleSave = async (): Promise<boolean> => {
    if (loaded.error) { toast.error(loaded.error); return false; }
    const names = new Set<string>();
    for (const header of headers) {
      if (!header.name.trim()) { toast.error("请填写公共头名称"); setSelectedId(header.id); return false; }
      if (names.has(header.name.trim())) { toast.error(`公共头名称“${header.name}”重复`); setSelectedId(header.id); return false; }
      names.add(header.name.trim());
      if (!header.fields.length) { toast.error(`公共头“${header.name}”至少需要一个字段`); setSelectedId(header.id); return false; }
      const fieldNames = new Set<string>();
      for (let i = 0; i < header.fields.length; i++) {
        const row = header.fields[i];
        const fail = (message: string) => { toast.error(message); setSelectedId(header.id); };
        if (!row.name.trim()) { fail(`“${header.name}”第 ${i + 1} 个字段缺少名称`); return false; }
        if (header.id !== "legacy-default" && fieldNames.has(row.name.trim())) { fail(`“${header.name}”内字段“${row.name}”重名`); return false; }
        fieldNames.add(row.name.trim());
        const length = Number(row.len);
        if (!Number.isInteger(length) || length < 1 || length > 65535) { fail(`字段“${row.name}”长度需为 1–65535 的整数`); return false; }
        if (row.enc === "hex" && !row.dict && row.value.trim() && !row.value.includes("{")) {
          const compact = row.value.replace(/\s+/g, "");
          if (!/^[0-9a-fA-F]+$/.test(compact) || compact.length / 2 !== length) { fail(`字段“${row.name}”需 ${length} 字节十六进制值`); return false; }
        }
      }
    }
    setSaving(true);
    try {
      const definitions: HeaderDefinition[] = headers.map(header => ({
        id: header.id,
        name: header.name.trim(),
        fields: header.fields.map(row => ({
          ...row,
          name: row.name.trim(),
          len: Number(row.len),
          value: row.value.trim(),
          dict: row.dict.trim() || null,
          profile_key: row.profile_key?.trim() || null,
          section: row.section ?? null,
          section_name: row.section_name?.trim() || null,
        })),
      }));
      await updateGroupHeaders(group.id, { headers: definitions });
      await onSaved();
      setDirty(false);
      toast.success("公共头已保存");
      return true;
    } catch (error) {
      toast.error("公共头保存失败", { description: String(error) });
      return false;
    } finally {
      setSaving(false);
    }
  };
  useEffect(() => {
    if (saveRef) saveRef.current = handleSave;
    return () => { if (saveRef) saveRef.current = null; };
  });
  return (
    <div className="mb-4 rounded-xl border bg-card">
      {/* 折叠标题栏 */}
      <button
        type="button"
        className="flex w-full items-center gap-3 px-4 py-2.5 text-left"
        onClick={() => setOpen((o) => !o)}
      >
        <ChevronDown
          className={cn(
            "h-4 w-4 shrink-0 text-muted-foreground transition-transform",
            !open && "-rotate-90",
          )}
        />
        <span className="text-sm font-medium">公共头</span>
        <span className="ml-auto flex items-center gap-2">
          <Badge variant="outline">{headers.length} 套</Badge>
          {dirty && <span className="text-xs text-amber-500">未保存</span>}
        </span>
      </button>

      {/* 展开内容 */}
      {open && (
        <div className="space-y-3 border-t px-4 py-3">
          <div className="flex flex-wrap items-center gap-2">
            {headers.map(header => (
              <button
                key={header.id}
                type="button"
                aria-label={`编辑公共头：${header.name}`}
                aria-pressed={selectedId === header.id}
                className={cn("flex items-center gap-2 rounded-md border px-3 py-2 text-sm transition-colors", selectedId === header.id ? "border-primary bg-primary/10 text-primary" : "hover:bg-muted")}
                onClick={() => { setSelectedId(header.id); setSettingsIndex(null); setRemoveId(null); }}
              >
                <span>{header.name}</span><span className="text-xs text-muted-foreground">{header.fields.length} 字段</span>
              </button>
            ))}
            <Button variant="outline" size="sm" disabled={Boolean(loaded.error)} onClick={() => addHeader()}><Plus /> 新增公共头</Button>
            <div className="ml-auto flex items-center gap-2">
            {dirty && <Button variant="ghost" size="sm" disabled={saving} onClick={() => {
              setHeaders(loaded.headers);
              setSelectedId(previous => loaded.headers.some(header => header.id === previous) ? previous : loaded.headers[0]?.id ?? "");
              setDirty(false);
              setRemoveId(null);
              setSettingsIndex(null);
            }}>撤销修改</Button>}
            <Button size="sm" onClick={handleSave} disabled={saving || !dirty || Boolean(loaded.error)}>
              <Save /> {saving ? "保存中…" : "保存公共头"}
            </Button>
            </div>
          </div>
          {loaded.error && <p role="alert" className="text-sm text-destructive">{loaded.error}</p>}
          {selected ? <>
            <div className="flex flex-wrap items-center gap-2">
              <Label htmlFor={`header-name-${group.id}`} className="shrink-0 text-xs">名称</Label>
              <Input id={`header-name-${group.id}`} aria-label="公共头名称" className="h-8 max-w-sm" value={selected.name} onChange={event => {
                const name = event.target.value;
                setHeaders(previous => previous.map(header => header.id === selectedId ? { ...header, name } : header));
                setDirty(true);
              }} />
              <span className="text-xs text-muted-foreground">{rows.length} 字段 · {totalBytes} B</span>
              <Button variant="ghost" size="sm" className="ml-auto" aria-label={`复制公共头：${selected.name}`} onClick={() => addHeader(selected)}><Copy /> 复制</Button>
              <Button variant="ghost" size="sm" className="text-destructive" aria-label={`删除公共头：${selected.name}`} onClick={() => setRemoveId(selected.id)}><Trash2 /> 删除</Button>
            </div>
            {removeId === selected.id && <div className="flex flex-wrap items-center gap-2 rounded-md border border-destructive/40 bg-destructive/5 p-2 text-sm">
              <span className="mr-auto">删除“{selected.name}”？协议正在使用的公共头无法删除。</span>
              <Button variant="ghost" size="sm" onClick={() => setRemoveId(null)}>取消</Button>
              <Button variant="destructive" size="sm" onClick={() => deleteHeader(selected.id)}>确认删除公共头</Button>
            </div>}
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-1">
              <Label className="text-xs">头部字段</Label>
            </div>
            <Button
              variant="outline"
              size="sm"
              onClick={() =>
                setRows((rs) => [
                  ...rs,
                  {
                    name: "",
                    len: "1",
                    enc: "hex",
                    value: "",
                    dict: "",
                    role: "none",
                  },
                ])
              }
            >
              <Plus /> 添加字段
            </Button>
          </div>

          {/* 字段表在自身区域内滚动(约7行高), 表头吸顶 */}
          <div className="max-h-[320px] overflow-auto rounded-md border">
            <div className="min-w-[880px]">
            <div className="sticky top-0 z-10 border-b bg-background">
              <div className="grid grid-cols-[1.4rem_1.75rem_9rem_4.5rem_9rem_minmax(0,1fr)_8.5rem_3.5rem] items-center gap-2 bg-muted/60 px-2 py-1.5 text-xs font-medium text-muted-foreground">
                <span />
                <span>#</span>
                <span>字段名</span>
                <span>长度(字节)</span>
                <span>字典</span>
                <span>值</span>
                <span>字段用途</span>
                <span className="text-right">操作</span>
              </div>
            </div>
            <div className="p-2">
              {rows.length === 0 && (
                <p className="py-4 text-center text-sm text-muted-foreground">
                  暂无字段
                </p>
              )}
              <SortableContainer
                count={rows.length}
                onReorder={(from, to) =>
                  setRows((rs) => arrayMove(rs, from, to))
                }
                renderOverlay={(i) => {
                  const r = rows[i];
                  if (!r) return null;
                  return (
                    <div className="grid min-w-[670px] grid-cols-[1.4rem_1.75rem_9rem_4.5rem_9rem_minmax(0,1fr)_8.5rem_3.5rem] items-center gap-2 rounded-md border bg-card p-2 shadow-xl">
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
                    </div>
                  );
                }}
                className="space-y-1.5"
              >
                {rows.map((r, i) => (
                  <SortableRow
                    key={i}
                    id={`${i}`}
                    className="grid grid-cols-[1.4rem_1.75rem_9rem_4.5rem_9rem_minmax(0,1fr)_8.5rem_3.5rem] items-center gap-2 bg-card"
                  >
                    {(handle) => (
                      <>
                        {handle}
                        <span className="text-right text-xs text-muted-foreground">
                          {i + 1}
                        </span>
                        <Input
                          className="h-8"
                          placeholder="字段名"
                          aria-label={`公共头第${i + 1}个字段名称`}
                          value={r.name}
                          onChange={(e) => setRow(i, { name: e.target.value })}
                        />
                        <Input
                          className="h-8"
                          type="number"
                          min={1}
                          placeholder="长度"
                          aria-label={`公共头第${i + 1}个字段长度`}
                          value={r.len}
                          onChange={(e) => setRow(i, { len: e.target.value })}
                        />
                        <DictionaryPicker
                          value={r.dict}
                          available={scopeDicts}
                          unavailable={outScopeDicts}
                          ariaLabel={`公共头字典：${r.name}`}
                          onRefresh={onRefreshDicts}
                          onChange={key => {
                            if (!key) setRow(i, { dict: "" });
                            else bindDict(i, key);
                          }}
                        />
                        <FieldValueEditor value={r.value} enc={r.enc} bytes={Number(r.len)}
                          onDeviceChange={value => setRow(i, { value, dict: "" })}
                          onSourceValueChange={value => setRow(i, { value, dict: "" })}
                          label={`公共头第${i + 1}个字段值`}
                          onChange={value => setRow(i, { value })}
                          fixedEditor={r.dict ? (
                          <Select
                            value={r.value}
                            onValueChange={(v) => setRow(i, { value: v })}
                          >
                            <SelectTrigger className="h-8 font-mono text-xs">
                              <SelectValue placeholder="选择条目" />
                            </SelectTrigger>
                            <SelectContent>
                              {(
                                scopeDicts.find((s) => s.key === r.dict)
                                  ?.items ?? []
                              ).map((it) => (
                                <SelectItem key={it.id} value={it.value}>
                                  {dictItemLabel(it.value, it.label)}
                                </SelectItem>
                              ))}
                            </SelectContent>
                          </Select>
                        ) : undefined} />
                        <Select
                          value={r.role ?? "none"}
                          onValueChange={(v) =>
                            setRow(i, { role: v as FieldRole })
                          }
                        >
                          <SelectTrigger
                            className="h-8 text-xs"
                            title={roleDescOf(r.role)}
                          >
                            <SelectValue />
                          </SelectTrigger>
                          <SelectContent className="min-w-[320px]">
                            {FIELD_ROLE_OPTIONS.map((o) => (
                              <SelectItem key={o.value} value={o.value}>
                                <span className="flex w-full items-center justify-between gap-4">
                                  <span className="shrink-0">{o.label}</span>
                                </span>
                              </SelectItem>
                            ))}
                          </SelectContent>
                        </Select>
                        <div className="flex justify-end">
                          <Button
                            variant="ghost"
                            size="icon"
                            className="h-7 w-7"
                            aria-label={`公共头字段设置：${r.name}`}
                            aria-expanded={settingsIndex === i}
                            onClick={() =>
                              setSettingsIndex(settingsIndex === i ? null : i)
                            }
                          >
                            <Settings2 className="h-3 w-3" />
                          </Button>
                          <Button
                            variant="ghost"
                            size="icon"
                            className="h-7 w-7 text-destructive hover:text-destructive"
                            title="删除"
                            onClick={() =>
                              setRows((rs) => rs.filter((_, j) => j !== i))
                            }
                          >
                            <Trash2 className="h-3 w-3" />
                          </Button>
                        </div>
                        {settingsIndex === i && (
                          <div className="col-span-full flex flex-wrap items-center gap-3 border-t border-dashed py-2 pl-10 text-xs">
                            <label className="flex items-center gap-2">
                              设备属性 Key
                              <Input
                                aria-label={`公共头设备属性 Key：${r.name}`}
                                className="h-8 w-48 font-mono text-xs"
                                value={r.profile_key ?? ""}
                                placeholder="不映射"
                                onChange={(e) =>
                                  setRow(i, { profile_key: e.target.value })
                                }
                              />
                            </label>
                            <label className="flex items-center gap-2">
                              编码
                              <UXSelect
                                value={String(r.enc) || "__empty__"}
                                onValueChange={(value) =>
                                  setRow(i, {
                                    enc: (value === "__empty__"
                                      ? ""
                                      : value) as FieldEnc,
                                  })
                                }
                              >
                                <UXSelectTrigger
                                  aria-label={`公共头编码：${r.name}`}
                                  className="h-8 w-28 text-xs"
                                >
                                  <UXSelectValue />
                                </UXSelectTrigger>
                                <UXSelectContent>
                                  <UXSelectItem
                                    value={String("hex") || "__empty__"}
                                  >
                                    Hex
                                  </UXSelectItem>
                                  <UXSelectItem
                                    value={String("uint") || "__empty__"}
                                  >
                                    UInt
                                  </UXSelectItem>
                                  <UXSelectItem
                                    value={String("ascii") || "__empty__"}
                                  >
                                    ASCII
                                  </UXSelectItem>
                                </UXSelectContent>
                              </UXSelect>
                            </label>
                            <FieldSectionSettings
                              field={r}
                              onChange={(patch) => setRow(i, patch)}
                            />
                          </div>
                        )}
                      </>
                    )}
                  </SortableRow>
                ))}
              </SortableContainer>
            </div>
            </div>
          </div>
          </> : <div className="py-5 text-center text-sm text-muted-foreground">暂无公共头</div>}
        </div>
      )}
    </div>
  );
}
