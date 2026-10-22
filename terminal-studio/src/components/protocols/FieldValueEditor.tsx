import { useEffect, useRef, useState, type ReactNode } from "react";
import { ChevronDown, Check } from "lucide-react";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { DeviceValuePicker } from "./DeviceValuePicker";

const sources = [
  ["fixed", "固定值"], ["seq", "自动流水号"], ["session", "当前会话"],
  ["tlen", "报文总长"], ["timestamp", "时间戳（秒）"],
  ["timestamp_ms", "时间戳（毫秒）"], ["last", "最近接收字段"],
  ["custom", "自定义表达式"],
] as const;
type Source = typeof sources[number][0];
function sourceOf(value: string): Source {
  const match = value.trim().match(/^\{(seq|session|tlen|timestamp|timestamp_ms)(?::\d+)?\}$/);
  if (match) return match[1] as Source;
  if (/^\{last:[^{}]*\}$/.test(value.trim())) return "last";
  return /[{}]/.test(value) ? "custom" : "fixed";
}

/** Presentation only: preserve the engine's stored expression byte-for-byte until edited. */
export function FieldValueEditor({ value, onChange, label, enc = "hex", fixedEditor, bytes, onDeviceChange, onSourceValueChange, allowedSources }: {
  value: string;
  onChange: (value: string) => void;
  label: string;
  enc?: string;
  fixedEditor?: ReactNode;
  bytes?: number;
  onDeviceChange?: (value: string) => void;
  onSourceValueChange?: (value: string) => void;
  allowedSources?: readonly Source[];
}) {
  const [open, setOpen] = useState(false);
  const [override, setOverride] = useState<Source>();
  const emitted = useRef<string | undefined>(undefined);
  useEffect(() => {
    if (value !== emitted.current) setOverride(undefined);
  }, [value]);
  const source = override ?? sourceOf(value);
  const title = sources.find(([key]) => key === source)![1];
  const edit = (next: string) => { emitted.current = next; onChange(next); };
  const choose = (next: Source) => {
    setOpen(false);
    if (next === source) return;
    setOverride(next);
    const nextValue = next === "fixed" || next === "custom" ? "" : next === "last" ? "{last:}" : `{${next}}`;
    emitted.current = nextValue;
    (onSourceValueChange ?? onChange)(nextValue);
  };
  const fieldName = source === "last" ? value.trim().slice(6, -1) : "";
  return <div className="min-w-0 space-y-1">
    <div className="flex min-w-0 items-center gap-1">
      {source === "fixed" && <div className="min-w-0 flex-1">
        {fixedEditor ?? <Input className="h-8 font-mono text-xs" aria-label={label}
          placeholder={enc === "ascii" ? "文本" : enc === "uint" ? "数值" : "固定值"}
          value={value} onChange={event => edit(event.target.value)} />}
      </div>}
      <Popover open={open} onOpenChange={setOpen}>
        <PopoverTrigger asChild>
          <button type="button" aria-label={`值来源：${label}`} title={title}
            className={`flex h-8 items-center justify-between gap-1 rounded-md border border-input bg-background px-2 text-xs hover:bg-accent focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring ${source === "fixed" ? "w-7 shrink-0 justify-center px-0" : "w-full min-w-0"}`}>
            {source !== "fixed" && <span className="truncate">{title}</span>}
            <ChevronDown className="h-3 w-3 shrink-0 text-muted-foreground" />
          </button>
        </PopoverTrigger>
        <PopoverContent align="start" className="w-44 p-1" aria-label="选择值来源">
          {fixedEditor && onSourceValueChange && <p className="px-2 py-1 text-xs text-muted-foreground">切换取值来源将解除字典绑定</p>}
          {sources.filter(([key]) => !allowedSources || allowedSources.includes(key)).map(([key, name]) => <button type="button" key={key} aria-pressed={source === key}
            className="flex w-full items-center justify-between rounded px-2 py-1.5 text-left text-xs hover:bg-accent focus-visible:bg-accent focus-visible:outline-none"
            onClick={() => choose(key)}>
            {name}{source === key && <Check className="h-3 w-3" />}
          </button>)}
        </PopoverContent>
      </Popover>
      {bytes !== undefined && (!fixedEditor || onDeviceChange) && <DeviceValuePicker enc={enc} bytes={bytes} label={label}
        replacesDictionary={Boolean(fixedEditor)}
        onChange={next => {
          setOverride("fixed"); emitted.current = next;
          (onDeviceChange ?? onChange)(next);
        }} />}
    </div>
    {source === "last" && <Input className="h-8 text-xs" aria-label={`接收字段：${label}`}
      placeholder="接收字段名" value={fieldName}
      onChange={event => edit(`{last:${event.target.value.replace(/[{}]/g, "")}}`)} />}
    {source === "custom" && <Input className="h-8 font-mono text-xs" aria-label={`表达式：${label}`}
      placeholder="自定义表达式" value={value} onChange={event => edit(event.target.value)} />}
  </div>;
}
