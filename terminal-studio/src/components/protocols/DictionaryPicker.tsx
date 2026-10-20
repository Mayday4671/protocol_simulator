import { useContext, useEffect, useId, useMemo, useRef, useState } from "react";
import { Check, ChevronDown, ChevronRight, Search } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { WorkspaceActivityContext } from "@/components/layout/WorkspaceActivity";
import { cn } from "@/lib/utils";
import type { DictSetView } from "@/types";

interface DictionaryPickerProps {
  value: string;
  available: DictSetView[];
  unavailable?: DictSetView[];
  ariaLabel: string;
  onChange: (key: string) => void;
  onRefresh?: () => Promise<unknown>;
}

function scopeName(dictionary: DictSetView) {
  return dictionary.group_ref == null ? "全局共享"
    : [dictionary.system_name, dictionary.group_name || `分组 ${dictionary.group_ref}`].filter(Boolean).join(" / ");
}

export function DictionaryPicker({ value, available, unavailable = [], ariaLabel, onChange, onRefresh }: DictionaryPickerProps) {
  const { active } = useContext(WorkspaceActivityContext);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [showOther, setShowOther] = useState(false);
  const searchRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const listId = useId();
  useEffect(() => { if (!active) setOpen(false); }, [active]);
  const selected = available.find(dictionary => dictionary.key === value)
    ?? unavailable.find(dictionary => dictionary.key === value);
  const invalid = Boolean(value && !available.some(dictionary => dictionary.key === value));
  const duplicateNames = useMemo(() => {
    const counts = new Map<string, number>();
    for (const dictionary of [...available, ...unavailable]) counts.set(dictionary.name, (counts.get(dictionary.name) ?? 0) + 1);
    return counts;
  }, [available, unavailable]);
  const groups = useMemo(() => {
    const term = query.trim().toLocaleLowerCase();
    const matches = (dictionary: DictSetView) => !term || [dictionary.name, dictionary.key, scopeName(dictionary), dictionary.description ?? "", ...dictionary.items.map(item => `${item.value} ${item.label}`)].join(" ").toLocaleLowerCase().includes(term);
    const sort = (dictionaries: DictSetView[]) => dictionaries.filter(matches).sort((a, b) => a.name.localeCompare(b.name, "zh-CN") || a.key.localeCompare(b.key));
    return [
      { name: "本分组", dictionaries: sort(available.filter(dictionary => dictionary.group_ref != null)), disabled: false },
      { name: "全局共享", dictionaries: sort(available.filter(dictionary => dictionary.group_ref == null)), disabled: false },
      { name: "其他分组 · 不可绑定", dictionaries: term || showOther ? sort(unavailable) : [], disabled: true },
    ];
  }, [available, unavailable, query, showOther]);
  const select = (key: string) => { onChange(key); setOpen(false); };
  const focusOption = (direction: number, edge?: "first" | "last") => {
    const options = [...(listRef.current?.querySelectorAll<HTMLButtonElement>('[role="option"]:not(:disabled)') ?? [])];
    const index = options.indexOf(document.activeElement as HTMLButtonElement);
    const next = edge === "first" ? 0 : edge === "last" ? options.length - 1 : index < 0 ? (direction > 0 ? 0 : options.length - 1) : (index + direction + options.length) % options.length;
    options[next]?.focus();
    options[next]?.scrollIntoView({ block: "nearest" });
  };
  return <Popover open={active && open} onOpenChange={next => {
    setOpen(next);
    if (next) { setQuery(""); setShowOther(false); void onRefresh?.(); }
  }}>
    <PopoverTrigger asChild>
      <Button type="button" variant="outline" role="combobox" aria-label={ariaLabel} aria-expanded={active && open} aria-controls={listId} aria-haspopup="listbox"
        title={selected ? `${selected.name} · ${selected.byte_len} B · ${scopeName(selected)}${invalid ? " · 当前分组不可用" : ""}` : invalid ? `字典 ${value} 已不存在` : "不绑定字典"}
        className={cn("h-8 min-w-0 w-full justify-between gap-1 px-2.5 text-xs font-normal", invalid && "border-destructive text-destructive")}>
        <span className="truncate">{selected?.name ?? (value ? "字典已不可用" : "不绑定")}</span>
        {selected && <span className="ml-auto shrink-0 text-[10px] text-muted-foreground">{selected.byte_len}B</span>}
        <ChevronDown className="h-3.5 w-3.5 shrink-0 opacity-50" />
      </Button>
    </PopoverTrigger>
    <PopoverContent align="start" collisionPadding={12} className="flex flex-col overflow-hidden p-0" style={{ width: "min(400px, calc(100vw - 24px))", maxHeight: "min(440px, var(--radix-popover-content-available-height))" }}
      onOpenAutoFocus={event => { event.preventDefault(); searchRef.current?.focus(); }}>
      <div className="relative shrink-0 border-b p-2">
        <Search className="pointer-events-none absolute left-4 top-4 h-4 w-4 text-muted-foreground" />
        <Input ref={searchRef} aria-label="搜索字典" placeholder="搜索字典、条目或含义" className="h-8 pl-8 text-xs" value={query} onChange={event => setQuery(event.target.value)}
          onKeyDown={event => {
            if (event.nativeEvent.isComposing || event.nativeEvent.keyCode === 229) return;
            if (event.key === "ArrowDown" || event.key === "ArrowUp") { event.preventDefault(); focusOption(event.key === "ArrowDown" ? 1 : -1); }
            if (event.key === "Enter") { event.preventDefault(); listRef.current?.querySelector<HTMLButtonElement>('[role="option"][data-dictionary]:not(:disabled)')?.click(); }
          }} />
      </div>
      <div ref={listRef} id={listId} role="listbox" aria-label="可选字典" className="min-h-0 max-h-[340px] flex-1 overflow-y-auto p-1"
        onKeyDown={event => {
          if (["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
            event.preventDefault(); focusOption(event.key === "ArrowUp" ? -1 : 1, event.key === "Home" ? "first" : event.key === "End" ? "last" : undefined);
          }
        }}>
        <button type="button" role="option" aria-selected={!value} className="flex w-full items-center gap-2 rounded px-2 py-2 text-left text-xs hover:bg-accent focus:bg-accent focus:outline-none" onClick={() => select("")}>
          <Check className={cn("h-3.5 w-3.5", value && "invisible")} />不绑定
        </button>
        {groups.map(group => group.dictionaries.length > 0 && <div key={group.name} role="group" aria-label={group.name}>
          <div className="px-2 pb-1 pt-2 text-[11px] font-medium text-muted-foreground">{group.name} <span className="font-normal">{group.dictionaries.length}</span></div>
          {group.dictionaries.map(dictionary => <button key={dictionary.id} type="button" role="option" data-dictionary={dictionary.key} aria-selected={!group.disabled && value === dictionary.key} disabled={group.disabled}
            className="flex w-full items-start gap-2 rounded px-2 py-2 text-left hover:bg-accent focus:bg-accent focus:outline-none disabled:cursor-not-allowed disabled:opacity-60" onClick={() => select(dictionary.key)}>
            <Check className={cn("mt-0.5 h-3.5 w-3.5 shrink-0", (group.disabled || value !== dictionary.key) && "invisible")} />
            <span className="min-w-0 flex-1">
              <span className="flex items-baseline justify-between gap-2"><span className="break-words text-xs font-medium">{dictionary.name}</span><span className="shrink-0 text-[11px] text-muted-foreground">{dictionary.byte_len} B · {dictionary.items.length} 项</span></span>
              <span className="mt-1 block truncate text-[11px] text-muted-foreground" title={dictionary.items.map(item => `${item.value} ${item.label}`).join(" · ")}>{dictionary.items.length ? dictionary.items.slice(0, 3).map(item => `${item.value} ${item.label}`).join(" · ") : "暂无条目"}</span>
              {(duplicateNames.get(dictionary.name) ?? 0) > 1 && <span className="mt-0.5 block truncate font-mono text-[10px] text-muted-foreground" title={dictionary.key}>{dictionary.key}</span>}
              {group.disabled && <span className="mt-1 block text-[11px]">仅限 {scopeName(dictionary)}</span>}
            </span>
          </button>)}
        </div>)}
        {groups.every(group => !group.dictionaries.length) && <p className="px-3 py-5 text-center text-xs text-muted-foreground">{query.trim() ? "没有匹配的字典" : "暂无可用字典"}</p>}
      </div>
      {unavailable.length > 0 && !query.trim() && <button type="button" className="flex shrink-0 items-center gap-1 border-t px-3 py-2 text-left text-xs text-muted-foreground hover:bg-accent" aria-expanded={showOther} onClick={() => setShowOther(!showOther)}>
        <ChevronRight className={cn("h-3.5 w-3.5", showOther && "rotate-90")} />其他分组（{unavailable.length}）
      </button>}
    </PopoverContent>
  </Popover>;
}
