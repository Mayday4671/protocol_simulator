import {
  Select as UXSelect,
  SelectTrigger as UXSelectTrigger,
  SelectValue as UXSelectValue,
  SelectContent as UXSelectContent,
  SelectItem as UXSelectItem,
} from "@/components/ui/select";
import { useEffect, useRef, useState } from "react";
import type { ProtocolField, DictSetView } from "@/types";
import { Input } from "@/components/ui/input";
import {
  encodeListEntries,
  initialListEntries,
  visibleListEntries,
  type ListEntry,
} from "@/lib/protocolLists";
interface ListEntriesProps {
  field: ProtocolField;
  count?: ProtocolField;
  countValue: string;
  dicts: DictSetView[];
  /** Snapshot restored on mount/reset, not a two-way value that resets each keystroke. */
  initialValue?: string;
  /** Full draft survives branch unmounts even when only its prefix is sent. */
  initialRows?: ListEntry[];
  onDraftChange?: (name: string, rows: ListEntry[]) => void;
  resetKey?: string | number;
  onChange: (name: string, value: string) => void;
}
export function ListEntries({
  field,
  count,
  countValue,
  dicts,
  initialValue,
  initialRows,
  onDraftChange,
  resetKey = "",
  onChange,
}: ListEntriesProps) {
  const units = field.unit_fields ?? [];
  // onChange 走 ref: 内联箭头每轮渲染都是新身份, 进依赖数组会与外层 setState 互馈成无限重渲染
  const onChangeRef = useRef(onChange);
  onChangeRef.current = onChange;
  const onDraftChangeRef = useRef(onDraftChange);
  onDraftChangeRef.current = onDraftChange;
  const identity = `${resetKey}:${field.name}:${JSON.stringify(units)}`;
  const initial = () => {
    try {
      return {
        identity,
        rows: initialRows ?? initialListEntries(field, initialValue),
        error: "",
      };
    } catch (e) {
      return {
        identity,
        rows: [] as Record<string, string>[],
        error: String(e),
      };
    }
  };
  const [state, setState] = useState(initial);
  const [open, setOpen] = useState(true);
  const current = state.identity === identity ? state : initial();
  if (state.identity !== identity) setState(current);
  let error = current.error,
    hex = "";
  let rows: ListEntry[] = [];
  try {
    rows = visibleListEntries(field, count, countValue, dicts, current.rows);
    hex = encodeListEntries(field, rows, dicts);
  } catch (e) {
    error = String(e);
  }
  const n = rows.length;
  useEffect(() => {
    onDraftChangeRef.current?.(field.name, current.rows);
  }, [field.name, current.rows]);
  useEffect(() => {
    onChangeRef.current(field.name, hex);
    onChangeRef.current(`__invalid_list_${field.name}`, error);
  }, [hex, error, field.name]);
  const update = (row: number, name: string, value: string) =>
    setState({
      identity,
      rows: Array.from(
        { length: Math.max(current.rows.length, rows.length) },
        (_, i) => {
          const entry = current.rows[i] ?? rows[i];
          return i === row ? { ...entry, [name]: value } : entry;
        },
      ),
      error: "",
    });
  return (
    <section className="col-span-full rounded border-l-4 border-primary bg-muted/20 p-3">
      <button
        type="button"
        aria-expanded={open}
        title={`数量来源：${count?.name ?? "未配置"}`}
        className="font-medium"
        onClick={() => setOpen(!open)}
      >
        {open ? "▾" : "▸"} 重复分组：{field.name} · {n} 条
      </button>
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {error}
        </p>
      )}
      {open && (
        <div className="max-h-80 space-y-2 overflow-auto">
          {rows.map((r, i) => (
            <fieldset
              key={i}
              className="runtime-fields rounded border p-2"
            >
              <legend className="text-xs">第 {i + 1} 条</legend>
              {units.map((u) => (
                <label key={u.name} className={`grid min-w-0 content-start gap-1 text-xs ${u.len > 32 ? "col-span-full" : ""}`}>
                  <span className="truncate leading-5" title={u.name}>{u.name}{u.enc !== "hex" && <span className="ml-1 font-normal text-muted-foreground">{u.enc === "uint" ? "十进制" : "文本"}</span>}</span>
                  <span className="sr-only">{u.len}B</span>
                  {u.dict ? (
                    <UXSelect
                      value={String(r[u.name] ?? "") || "__empty__"}
                      onValueChange={(value) =>
                        update(i, u.name, value === "__empty__" ? "" : value)
                      }
                    >
                      <UXSelectTrigger
                        aria-label={`${field.name} 第${i + 1}条 ${u.name}`}
                        className="h-8 font-mono text-xs"
                      >
                        <UXSelectValue />
                      </UXSelectTrigger>
                      <UXSelectContent>
                        <UXSelectItem
                          value={String("") || "__empty__"}
                          disabled
                        >
                          选择条目
                        </UXSelectItem>
                        {!(
                          dicts.find((d) => d.key === u.dict)?.items ?? []
                        ).some((item) => item.value === r[u.name]) &&
                          r[u.name] && (
                            <UXSelectItem
                              value={String(r[u.name]) || "__empty__"}
                            >
                              {r[u.name]} · 无效字典项
                            </UXSelectItem>
                          )}
                        {(dicts.find((d) => d.key === u.dict)?.items ?? []).map(
                          (item) => (
                            <UXSelectItem
                              key={item.id}
                              value={String(item.value) || "__empty__"}
                            >
                              {item.value} · {item.label}
                            </UXSelectItem>
                          ),
                        )}
                      </UXSelectContent>
                    </UXSelect>
                  ) : (
                    <Input
                      aria-label={`${field.name} 第${i + 1}条 ${u.name}`}
                      className="h-8 font-mono text-xs"
                      value={r[u.name] ?? ""}
                      onChange={(e) => update(i, u.name, e.target.value)}
                    />
                  )}
                </label>
              ))}
            </fieldset>
          ))}
        </div>
      )}
    </section>
  );
}
