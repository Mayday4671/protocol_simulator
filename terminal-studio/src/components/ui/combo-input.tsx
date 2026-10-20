/**
 * @file combo-input.tsx
 * @description 可输入 + 可下拉选择的组合输入框, 以及字段取值建议生成
 * @module 通用 UI
 */
import * as React from "react";
import { Check, ChevronDown } from "lucide-react";

import { cn } from "@/lib/utils";
import {
  Popover,
  PopoverAnchor,
  PopoverContent,
} from "@/components/ui/popover";
import { WorkspaceActivityContext } from "@/components/layout/WorkspaceActivity";

/**
 * 组合输入框: 可手动输入, 也可点击右侧箭头从建议列表中选择。
 * 与选择框使用同类 Portal 弹层，避免被滚动容器裁切；支持键盘选择。
 *
 * @param suggestions - 下拉建议列表; 字符串原样显示, 也可传 {value,label}
 *   显示 label、选中时填入 value（如「温湿度传感器 · 0A 01」选中填 0A 01）
 */
export function ComboInput({
  suggestions = [],
  className,
  onChange,
  ...props
}: Omit<React.ComponentProps<"input">, "list"> & {
  suggestions?: Array<string | { value: string; label: string }>;
}) {
  const items = suggestions.map((s) =>
    typeof s === "string" ? { value: s, label: s } : s,
  );
  const [open, setOpen] = React.useState(false);
  const [highlighted, setHighlighted] = React.useState(-1);
  const wrapRef = React.useRef<HTMLDivElement>(null);
  const inputRef = React.useRef<HTMLInputElement>(null);
  const optionRefs = React.useRef<Array<HTMLLIElement | null>>([]);
  const listId = React.useId();
  const { active } = React.useContext(WorkspaceActivityContext);
  const enabled =
    active && !props.disabled && !props.readOnly && items.length > 0;
  const visible = open && enabled;

  const choose = (index: number) => {
    const item = items[index];
    if (!item || !enabled) return;
    const target = { ...inputRef.current, value: item.value };
    onChange?.({
      target,
      currentTarget: target,
    } as React.ChangeEvent<HTMLInputElement>);
    setOpen(false);
    setHighlighted(-1);
  };

  React.useEffect(() => {
    if (!enabled) setOpen(false);
  }, [enabled]);
  React.useEffect(() => {
    if (visible && highlighted >= 0) {
      optionRefs.current[highlighted]?.scrollIntoView({ block: "nearest" });
    }
  }, [visible, highlighted]);

  return (
    <Popover open={visible} onOpenChange={(next) => setOpen(next && enabled)}>
      <PopoverAnchor asChild>
        <div ref={wrapRef} className="relative w-full">
          <input
            ref={inputRef}
            autoComplete="off"
            spellCheck={false}
            {...props}
            role={items.length ? "combobox" : undefined}
            aria-autocomplete={items.length ? "list" : undefined}
            aria-expanded={items.length ? visible : undefined}
            aria-controls={visible ? listId : undefined}
            aria-activedescendant={
              visible && highlighted >= 0
                ? `${listId}-${highlighted}`
                : undefined
            }
            onChange={onChange}
            onFocus={(e) => {
              props.onFocus?.(e);
              if (enabled) {
                setHighlighted(-1);
                setOpen(true);
              }
            }}
            onKeyDown={(e) => {
              props.onKeyDown?.(e);
              if (e.defaultPrevented || !enabled) return;
              if (e.key === "ArrowDown" || e.key === "ArrowUp") {
                e.preventDefault();
                setOpen(true);
                setHighlighted((index) =>
                  !visible || index < 0
                    ? e.key === "ArrowDown"
                      ? 0
                      : items.length - 1
                    : (index +
                        (e.key === "ArrowDown" ? 1 : -1) +
                        items.length) %
                      items.length,
                );
              } else if (visible && e.key === "Enter" && highlighted >= 0) {
                e.preventDefault();
                choose(highlighted);
              } else if (visible && e.key === "Escape") {
                e.preventDefault();
                e.stopPropagation();
                setOpen(false);
              } else if (e.key === "Tab") {
                setOpen(false);
              }
            }}
            className={cn(
              "flex h-8 w-full rounded-md border border-input bg-transparent px-2 py-1 pr-7 font-mono text-xs shadow-sm transition-colors placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50",
              className,
            )}
          />
          {items.length > 0 && (
            <button
              type="button"
              tabIndex={-1}
              title="选择建议值"
              aria-label="选择建议值"
              disabled={!enabled}
              className="absolute right-1 top-1/2 -translate-y-1/2 rounded p-0.5 text-muted-foreground hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50"
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => {
                inputRef.current?.focus();
                setHighlighted(-1);
                setOpen(!visible);
              }}
            >
              <ChevronDown className="h-3.5 w-3.5" />
            </button>
          )}
        </div>
      </PopoverAnchor>
      <PopoverContent
        align="start"
        className="z-[100] w-[var(--radix-popover-trigger-width)] max-w-[calc(100vw-16px)] p-1"
        onOpenAutoFocus={(e) => e.preventDefault()}
        onCloseAutoFocus={(e) => e.preventDefault()}
        onInteractOutside={(e) => {
          if (wrapRef.current?.contains(e.target as Node)) e.preventDefault();
        }}
      >
        <ul
          id={listId}
          role="listbox"
          aria-label="建议值"
          className="max-h-[min(16rem,var(--radix-popover-content-available-height))] overflow-y-auto"
        >
          {items.map((it, index) => (
            <li
              id={`${listId}-${index}`}
              key={it.value + it.label}
              ref={(element) => {
                optionRefs.current[index] = element;
              }}
              role="option"
              aria-selected={String(props.value ?? "") === it.value}
              className={cn(
                "relative cursor-default select-none rounded-sm py-1.5 pl-2 pr-8 text-sm outline-none break-words",
                highlighted === index && "bg-accent text-accent-foreground",
              )}
              onMouseDown={(e) => e.preventDefault()}
              onMouseMove={() => setHighlighted(index)}
              onClick={() => choose(index)}
            >
              {it.label}
              {String(props.value ?? "") === it.value && (
                <Check className="absolute right-2 top-2 h-3.5 w-3.5" />
              )}
            </li>
          ))}
        </ul>
      </PopoverContent>
    </Popover>
  );
}

/**
 * 按字段编码给出取值建议: 内置变量 + 字典引用
 *
 * 字典引用写法是 `{dict:字典Key}`（取默认条目）, 与内置变量、发送参数分属
 * 不同命名空间, 不会互相覆盖。三种编码都支持变量。
 *
 * 注: 字段若已在「字典」列绑定字典, 取值改为条目下拉, 不再走这里。
 *
 * @param enc - 字段编码: hex / uint / ascii
 * @param dictKeys - 当前作用域可用的字典 Key 列表
 * @returns 供 datalist 使用的建议字符串数组
 */
export function valueSuggestions(
  enc: string,
  dictKeys: string[] = [],
): string[] {
  const dictRefs = dictKeys.map((k) => `{dict:${k}}`);
  switch (enc) {
    case "uint":
      return [
        ...dictRefs,
        "{node_id}",
        "{timestamp}",
        "{timestamp_ms}",
        "{seq}",
        "{random}",
      ];
    case "ascii":
      return [...dictRefs, "{node_id}", "{datetime}"];
    default:
      // 十六进制: 字典引用 + 内置变量（按字段长度渲染）
      return [
        ...dictRefs,
        "{node_id}",
        "{session}",
        "{timestamp}",
        "{seq}",
        "{random}",
      ];
  }
}

/**
 * 字典条目在下拉里的展示文本, 如 `01 · 温度传感器`
 *
 * @param value - 条目十六进制值
 * @param label - 条目含义
 */
export function dictItemLabel(value: string, label: string): string {
  return `${value} · ${label}`;
}
