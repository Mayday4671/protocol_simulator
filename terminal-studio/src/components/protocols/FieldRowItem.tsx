import {
  Select as UXSelect,
  SelectTrigger as UXSelectTrigger,
  SelectValue as UXSelectValue,
  SelectContent as UXSelectContent,
  SelectItem as UXSelectItem,
} from "@/components/ui/select";
/**
 * 协议字段行（memo 化 + 拖拽排序）。
 *
 * 每行含多个受控输入/Radix Select, 全部行内联渲染时打一个字就整表重渲染,
 * 字段多时明显卡顿; 抽成 memo 组件后只有被编辑的那一行重渲染。
 * 拖拽(useSortable)并入本组件, 使 memo 拦截在 dnd-kit 之前生效。
 */
import * as React from "react";
import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { ArrowDown, ArrowUp, ChevronDown, Settings2, Plus, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
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
import { cn } from "@/lib/utils";
import { FieldSectionSettings } from "@/components/protocols/FieldSectionSettings";
import {
  FIELD_ROLE_OPTIONS,
  roleDescOf,
} from "@/components/protocols/fieldRoles";
import type { DictSetView, FieldRole, ProtocolField } from "@/types";
export type UnitRow = Omit<ProtocolField, "len" | "dict"> & {
  len: string;
  dict: string;
};
/** 行内「不覆盖公共头」哨兵 */
export const NO_HDR = "__no_hdr__";
/** 可编辑字段行数据（与 ProtocolManager.FieldRow 同构） */
export interface FieldRowData {
  name: string;
  len: string;
  enc: "hex" | "uint" | "ascii";
  value: string;
  dict: string;
  role: FieldRole;
  hdr_ref: string;
  profile_key?: string;
  section?: ProtocolField["section"];
  section_name?: ProtocolField["section_name"];
  /** 变长列表的单台记录布局(设备列表字段) */
  units?: UnitRow[];
  /** 出现条件(分支) */
  whenField?: string;
  whenValue?: string;
}
/** 行 grid 布局（与表头一致） */
export const FIELD_ROW_GRID =
  "grid min-w-[960px] grid-cols-[1.4rem_1.75rem_9rem_4.5rem_9rem_minmax(8rem,1fr)_8.5rem_7.5rem_6rem] items-center gap-2";
export const FieldRowItem = React.memo(function FieldRowItem({
  id,
  r,
  index,
  count,
  scopeDicts,
  outScopeDicts,
  onRefreshDicts,
  headerFieldOptions,
  onPatch,
  onBindDict,
  onMove,
  onRemove,
}: {
  id: string;
  r: FieldRowData;
  index: number;
  count: number;
  scopeDicts: DictSetView[];
  outScopeDicts: DictSetView[];
  onRefreshDicts?: () => Promise<unknown>;
  headerFieldOptions: { value: string; label: string; aliases?: string[] }[];
  onPatch: (i: number, patch: Partial<FieldRowData>) => void;
  onBindDict: (i: number, key: string) => void;
  onMove: (i: number, dir: -1 | 1) => void;
  onRemove: (i: number) => void;
}) {
  const {
    attributes,
    listeners,
    setNodeRef,
    setActivatorNodeRef,
    transform,
    transition,
    isDragging,
  } = useSortable({ id });
  const noHdr = headerFieldOptions.length === 0;
  // Old bare references display their unique qualified option without adding duplicate menu entries.
  const selectedHeader = headerFieldOptions.find(option => option.value === r.hdr_ref)
    ?? headerFieldOptions.find(option => option.aliases?.includes(r.hdr_ref.trim()));
  const selectedHeaderValue = selectedHeader?.value ?? r.hdr_ref;
  const missingHeaderRef = Boolean(r.hdr_ref && !selectedHeader);
  const [open, setOpen] = React.useState(false);
  const [structureOpen, setStructureOpen] = React.useState(true);
  const structureId = React.useId();
  const isArray = r.role === "device_list";
  const defaultHex = r.value.replace(/\s/g, "");
  const unitLength = /^\d+$/.test(r.len) ? Number(r.len) : 0;
  const defaultCount = /^[0-9a-f]+$/i.test(defaultHex) &&
    Number.isSafeInteger(unitLength) && unitLength > 0 &&
    defaultHex.length % (unitLength * 2) === 0
      ? defaultHex.length / (unitLength * 2)
      : null;
  const defaultSummary = !r.value.trim()
    ? "未设置默认数据"
    : defaultCount !== null
      ? `${defaultCount} 条默认数据 · ${defaultHex.length / 2} B`
      : "已设置默认数据";
  return (
    <div
      ref={setNodeRef}
      data-field-index={index}
      style={{
        transform: CSS.Transform.toString(transform),
        transition,
        zIndex: isDragging ? 30 : undefined,
        opacity: isDragging ? 0.85 : 1,
      }}
      className="bg-card"
    >
      <div className={FIELD_ROW_GRID}>
        <button
          type="button"
          ref={setActivatorNodeRef}
          title="拖拽调整顺序"
          className="cursor-grab touch-none rounded p-0.5 text-muted-foreground hover:text-foreground active:cursor-grabbing"
          {...attributes}
          {...listeners}
        >
          <svg viewBox="0 0 16 16" className="h-3.5 w-3.5" fill="currentColor">
            <circle cx="5" cy="3" r="1.4" />
            <circle cx="11" cy="3" r="1.4" />
            <circle cx="5" cy="8" r="1.4" />
            <circle cx="11" cy="8" r="1.4" />
            <circle cx="5" cy="13" r="1.4" />
            <circle cx="11" cy="13" r="1.4" />
          </svg>
        </button>
        <span className="text-right text-xs text-muted-foreground">
          {index + 1}
        </span>
        <Input
          className="h-8"
          placeholder="字段名"
          title={isArray ? r.name : undefined}
          value={r.name}
          onChange={(e) => onPatch(index, { name: e.target.value })}
        />
        {isArray ? (
          <label className="flex min-w-0 flex-col gap-0.5">
            <span className="text-[10px] leading-none text-muted-foreground">单条</span>
            <span className="relative">
              <Input
                aria-label={`单条字节数：${r.name}`}
                className="h-8 pr-5"
                type="number"
                min={1}
                placeholder="长度"
                value={r.len}
                onChange={(e) => onPatch(index, { len: e.target.value })}
              />
              <span className="pointer-events-none absolute right-2 top-2 text-xs text-muted-foreground">B</span>
            </span>
          </label>
        ) : (
          <Input
            className="h-8"
            type="number"
            min={1}
            placeholder="长度"
            value={r.len}
            onChange={(e) => onPatch(index, { len: e.target.value })}
          />
        )}
        <DictionaryPicker
          value={r.dict}
          available={scopeDicts}
          unavailable={outScopeDicts}
          ariaLabel={`字典：${r.name}`}
          onRefresh={onRefreshDicts}
          onChange={key => {
            if (!key) onPatch(index, { dict: "" });
            else onBindDict(index, key);
          }}
        />
        {isArray ? (
          <button
            type="button"
            aria-label={`条目结构：${r.name}`}
            aria-expanded={structureOpen}
            aria-controls={structureId}
            className="flex min-w-0 flex-col gap-1 rounded-md border border-input px-2.5 py-1.5 text-left hover:bg-accent focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
            onClick={() => setStructureOpen(value => !value)}
          >
            <span className="flex items-center gap-1.5 text-xs font-medium">
              <ChevronDown className={cn("h-3.5 w-3.5 shrink-0", !structureOpen && "-rotate-90")} />
              条目结构 · {(r.units ?? []).length} 字段
            </span>
            <span className="text-[11px] leading-snug text-muted-foreground">{defaultSummary}</span>
          </button>
        ) : (
          <FieldValueEditor value={r.value} enc={r.enc} bytes={Number(r.len)} label={`字段值：${r.name}`}
            onDeviceChange={value => onPatch(index, { value, dict: "" })}
            onSourceValueChange={value => onPatch(index, { value, dict: "" })}
            onChange={value => onPatch(index, { value })}
            fixedEditor={r.dict ? (
          <Select
            value={r.value}
            onValueChange={(v) => onPatch(index, { value: v })}
          >
            <SelectTrigger
              className={cn(
                "h-8 font-mono text-xs",
                !scopeDicts.some((s) => s.key === r.dict) &&
                  "border-destructive text-destructive",
              )}
            >
              <SelectValue placeholder="选择条目" />
            </SelectTrigger>
            <SelectContent>
              {(scopeDicts.find((s) => s.key === r.dict)?.items ?? []).map(
                (it) => (
                  <SelectItem key={it.id} value={it.value}>
                    {dictItemLabel(it.value, it.label)}
                  </SelectItem>
                ),
              )}
            </SelectContent>
          </Select>
        ) : undefined} />
        )}
        <Select
          value={r.role ?? "none"}
          onValueChange={(v) => onPatch(index, { role: v as FieldRole })}
        >
          <SelectTrigger className="h-8 text-xs" title={roleDescOf(r.role)}>
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
        <Select
          value={selectedHeaderValue || NO_HDR}
          onValueChange={(v) =>
            onPatch(index, { hdr_ref: v === NO_HDR ? "" : v })
          }
          disabled={noHdr && !r.hdr_ref}
        >
          <SelectTrigger
            className="h-8 text-xs"
            aria-label={`覆盖公共头：${r.name}`}
            title={noHdr ? "先勾选使用的公共头" : "选择要覆盖的头字段"}
          >
            <SelectValue placeholder="不覆盖" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={NO_HDR}>不覆盖</SelectItem>
            {missingHeaderRef && <SelectItem value={r.hdr_ref} disabled>重新选择：{r.hdr_ref}</SelectItem>}
            {headerFieldOptions.map(option => (
              <SelectItem key={option.value} value={option.value}>
                {option.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <div className="flex items-center justify-end gap-0.5">
          {
            <Button
              variant="ghost"
              size="icon"
              className={cn("h-7 w-7", open && "bg-accent")}
              onClick={() => setOpen((o) => !o)}
              title="字段设置"
              aria-label={`字段设置：${r.name}`}
              aria-expanded={open}
            >
              <Settings2 className="h-3 w-3" />
            </Button>
          }
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            onClick={() => onMove(index, -1)}
            disabled={index === 0}
            title="上移"
          >
            <ArrowUp className="h-3 w-3" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            onClick={() => onMove(index, 1)}
            disabled={index === count - 1}
            title="下移"
          >
            <ArrowDown className="h-3 w-3" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7 text-destructive hover:text-destructive"
            onClick={() => onRemove(index)}
            title="删除"
          >
            <Trash2 className="h-3 w-3" />
          </Button>
        </div>
      </div>

      {open && (
        <div className="grid min-w-[800px] grid-cols-2 gap-3 border-t border-dashed px-2 py-3 pl-10">
          <label className="flex items-center gap-2 whitespace-nowrap text-xs">
            字段编码
            <UXSelect
              value={String(r.enc) || "__empty__"}
              onValueChange={(value) =>
                onPatch(index, {
                  enc: (value === "__empty__"
                    ? ""
                    : value) as FieldRowData["enc"],
                })
              }
            >
              <UXSelectTrigger
                aria-label={`字段编码：${r.name}`}
                className="h-8 w-44 text-xs"
              >
                <UXSelectValue />
              </UXSelectTrigger>
              <UXSelectContent>
                <UXSelectItem value={String("hex") || "__empty__"}>
                  Hex 十六进制
                </UXSelectItem>
                <UXSelectItem value={String("uint") || "__empty__"}>
                  UInt 十进制整数
                </UXSelectItem>
                <UXSelectItem value={String("ascii") || "__empty__"}>
                  ASCII 文本
                </UXSelectItem>
              </UXSelectContent>
            </UXSelect>
          </label>
          <label className="flex items-center gap-2 whitespace-nowrap text-xs">
            设备属性 Key
            <Input
              aria-label={`设备属性 Key：${r.name}`}
              className="h-8 max-w-xs font-mono text-xs"
              value={r.profile_key ?? ""}
              placeholder="不映射"
              onChange={(e) => onPatch(index, { profile_key: e.target.value })}
            />
          </label>
          <FieldSectionSettings
            field={r}
            onChange={(patch) => onPatch(index, patch)}
          />
        </div>
      )}

      {isArray && (
            <section id={structureId} hidden={!structureOpen} aria-label={`条目结构编辑：${r.name}`} className="ml-10 mt-2 min-w-[750px] rounded-md border bg-muted/20 p-3">
              <div className="mb-3 flex items-start justify-between gap-4">
                <div className="min-w-0">
                  <h4 className="break-all text-sm font-medium">{r.name || "未命名数组"}</h4>
                  <div className="mt-0.5 text-xs text-muted-foreground">单条 {r.len || "—"} B · {(r.units ?? []).length} 个子字段</div>
                </div>
                <Button
                  variant="outline"
                  size="sm"
                  className="h-7 shrink-0"
                  aria-label={`添加子字段：${r.name}`}
                  onClick={() =>
                    onPatch(index, {
                      units: [
                        ...(r.units ?? []),
                        { name: "", len: "1", dict: "", enc: "hex", value: "" },
                      ],
                    })
                  }
                >
                  <Plus /> 添加子字段
                </Button>
              </div>
              <div className="space-y-1.5">
                {(r.units ?? []).length > 0 && <div aria-hidden="true" className="grid grid-cols-[1.25rem_minmax(8rem,1fr)_4.5rem_minmax(8rem,1fr)_5rem_minmax(8rem,1fr)_1.75rem] gap-2 text-[11px] text-muted-foreground">
                  <span /><span>字段名</span><span>长度 / B</span><span>字典</span><span>编码</span><span>默认值</span><span />
                </div>}
                {(r.units ?? []).map((u, ui) => (
                  <div key={ui} className="grid grid-cols-[1.25rem_minmax(8rem,1fr)_4.5rem_minmax(8rem,1fr)_5rem_minmax(8rem,1fr)_1.75rem] items-center gap-2">
                    <span className="w-5 text-right text-xs text-muted-foreground">
                      {ui + 1}
                    </span>
                    <Input
                      aria-label={`${r.name} 第${ui + 1}个子字段 名称`}
                      className="h-8 min-w-0"
                      placeholder="子字段名, 如 设备编号"
                      value={u.name}
                      onChange={(e) =>
                        onPatch(index, {
                          units: (r.units ?? []).map((x, j) =>
                            j === ui ? { ...x, name: e.target.value } : x,
                          ),
                        })
                      }
                    />
                    <Input
                      aria-label={`${r.name} 第${ui + 1}个子字段 长度`}
                      className="h-8 min-w-0"
                      type="number"
                      min={1}
                      placeholder="长度"
                      value={u.len}
                      onChange={(e) =>
                        onPatch(index, {
                          units: (r.units ?? []).map((x, j) =>
                            j === ui ? { ...x, len: e.target.value } : x,
                          ),
                        })
                      }
                    />
                    <DictionaryPicker
                      value={u.dict}
                      available={scopeDicts}
                      unavailable={outScopeDicts}
                      ariaLabel={`${r.name} 第${ui + 1}个子字段 字典`}
                      onRefresh={onRefreshDicts}
                      onChange={key => onPatch(index, {
                        units: (r.units ?? []).map((x, j) => j === ui ? {
                          ...x,
                          dict: key,
                          len: key ? String(scopeDicts.find(sd => sd.key === key)?.byte_len ?? x.len) : x.len,
                          value: key ? scopeDicts.find(sd => sd.key === key)?.items[0]?.value ?? "" : x.value,
                        } : x),
                      })}
                    />
                    <UXSelect
                      value={String(u.enc ?? "hex") || "__empty__"}
                      onValueChange={(value) =>
                        onPatch(index, {
                          units: (r.units ?? []).map((x, j) =>
                            j === ui
                              ? {
                                  ...x,
                                  enc: (value === "__empty__"
                                    ? ""
                                    : value) as ProtocolField["enc"],
                                }
                              : x,
                          ),
                        })
                      }
                    >
                      <UXSelectTrigger
                        aria-label={`${r.name} 第${ui + 1}个子字段 编码`}
                        className="h-8 min-w-0 text-xs"
                      >
                        <UXSelectValue />
                      </UXSelectTrigger>
                      <UXSelectContent>
                        <UXSelectItem value={String("hex") || "__empty__"}>
                          Hex
                        </UXSelectItem>
                        <UXSelectItem value={String("uint") || "__empty__"}>
                          UInt
                        </UXSelectItem>
                        <UXSelectItem value={String("ascii") || "__empty__"}>
                          ASCII
                        </UXSelectItem>
                      </UXSelectContent>
                    </UXSelect>
                    <FieldValueEditor
                      label={`${r.name} 第${ui + 1}个子字段 默认值`}
                      enc={u.enc}
                      bytes={Number(u.len)}
                      onDeviceChange={value =>
                        onPatch(index, { units: (r.units ?? []).map((x, j) => j === ui ? { ...x, value, dict: "" } : x) })}
                      onSourceValueChange={value =>
                        onPatch(index, { units: (r.units ?? []).map((x, j) => j === ui ? { ...x, value, dict: "" } : x) })}
                      value={u.value ?? ""}
                      onChange={(value) =>
                        onPatch(index, {
                          units: (r.units ?? []).map((x, j) =>
                            j === ui ? { ...x, value } : x,
                          ),
                        })
                      }
                    />
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-7 w-7 text-destructive hover:text-destructive"
                      aria-label={`${r.name} 删除第${ui + 1}个子字段`}
                      onClick={() => onPatch(index, { units: (r.units ?? []).filter((_, j) => j !== ui) })}
                      title="删除子字段"
                    >
                      <Trash2 className="h-3 w-3" />
                    </Button>
                  </div>
                ))}
                {(r.units ?? []).length === 0 && (
                  <p className="py-1 text-center text-xs text-muted-foreground">
                    暂无子字段
                  </p>
                )}
              </div>
              <details className="mt-3 border-t pt-2">
                <summary aria-label={`展开默认条目数据：${r.name}`} className="cursor-pointer rounded text-xs text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring">默认条目数据（HEX）</summary>
                <Textarea
                  aria-label={`默认条目数据（HEX）：${r.name}`}
                  className="mt-2 min-h-20 resize-y break-all font-mono text-xs"
                  rows={3}
                  placeholder="完整默认数据，留空使用子字段默认值"
                  value={r.value}
                  onChange={event => onPatch(index, { value: event.target.value })}
                />
              </details>
            </section>
      )}
    </div>
  );
});
