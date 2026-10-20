import {
  Select as UXSelect,
  SelectTrigger as UXSelectTrigger,
  SelectValue as UXSelectValue,
  SelectContent as UXSelectContent,
  SelectItem as UXSelectItem,
} from "@/components/ui/select";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { ChevronDown, Settings2 } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
export interface BranchField {
  name: string;
  value?: string;
  whenField?: string;
  whenValue?: string;
}
type Layout =
  | {
      kind: "field";
      index: number;
    }
  | BranchLayout;
interface BranchLayout {
  kind: "branch";
  key: string;
  field: string;
  choices: {
    value: string;
    items: Layout[];
  }[];
}
/** 分支条件值的规范形: 纯十六进制文本(可含空格, 如 "0 1")压缩为大写紧凑形式;
 * 字典含义 / uint / "*" 等非十六进制文本原样保留。引擎按同一口径判定条件等价。 */
export function canonicalWhenValue(value: string): string {
  const v = value ?? "";
  return /^[0-9a-fA-F\s]+$/.test(v) ? v.replace(/\s+/g, "").toUpperCase() : v;
}
/** Only adjacent fields with the same condition share a visual group.
 * Reusing an earlier group would move later fields ahead of intervening bytes. */
function layoutOf(fields: BranchField[]): Layout[] {
  const root: Layout[] = [];
  fields.forEach((f, index) => {
    if (!f.whenField) {
      root.push({ kind: "field", index });
      return;
    }
    const choiceValue = canonicalWhenValue(f.whenValue ?? "");
    const previous = root[root.length - 1];
    if (previous?.kind === "branch" && previous.field === f.whenField &&
        previous.choices[0].value === choiceValue) {
      previous.choices[0].items.push({ kind: "field", index });
    } else {
      root.push({
        kind: "branch",
        key: `branch-${index}`,
        field: f.whenField,
        choices: [{ value: choiceValue, items: [{ kind: "field", index }] }],
      });
    }
  });
  return root;
}
interface FieldBranchesProps {
  fields: BranchField[];
  /** Upstream fields such as the shared header are valid conditions, not body rows. */
  conditionFields?: BranchField[];
  renderField: (index: number) => ReactNode;
  /** 列表及长内容独占一行，普通运行参数按可用宽度排成多列。 */
  fullWidth?: (index: number) => boolean;
  /** Runtime uses the same condition matcher as the sender; there is no preview selector. */
  activeWhen?: (index: number) => boolean;
  onBranchChange?: (
    indices: number[],
    condition: {
      field: string;
      value: string;
    },
  ) => void;
}
function BranchGroup({
  group,
  fields,
  conditionFields = [],
  renderField,
  activeWhen,
  fullWidth,
  onBranchChange,
}: FieldBranchesProps & {
  group: BranchLayout;
}) {
  const [open, setOpen] = useState(true);
  const [draft, setDraft] = useState<{
    field: string;
    value: string;
  } | null>(null);
  // Multiple written values can match the same wire value (e.g. "01" / "0 1",
  // or a dictionary label / hex value). Keep every matching field in the group.
  const activeChoices = activeWhen
    ? group.choices.filter((c) =>
        c.items.some((item) => item.kind === "field" && activeWhen(item.index)),
      )
    : [];
  const current = activeWhen
    ? activeChoices.length
      ? {
          value: activeChoices[0].value,
          items: activeChoices.flatMap((c) => c.items),
        }
      : undefined
    : group.choices[0];
  const itemCount = current?.items.length ?? 0;
  const previousItemCount = useRef(itemCount);
  useEffect(() => {
    // Adding fields to an existing collapsed branch must reveal the result.
    // Ordinary value edits and runtime condition changes preserve its state.
    if (!activeWhen && itemCount > previousItemCount.current) setOpen(true);
    previousItemCount.current = itemCount;
  }, [itemCount, activeWhen]);
  if (!current) return null;
  const indices = current.items.flatMap((item) =>
    item.kind === "field" ? [item.index] : [],
  );
  const candidates = [
    ...new Map(
      [...conditionFields, ...fields.slice(0, Math.min(...indices))]
        .filter((f) => f.name)
        .map((f) => [f.name, f]),
    ).values(),
  ];
  const changeCondition = (condition: { field: string; value: string }) => {
    onBranchChange?.(indices, condition);
    setDraft(null);
  };
  return (
    <section
      data-branch-group={group.field}
      data-branch-value={current.value}
      className="col-span-full min-w-0 rounded-md border border-amber-500/40"
    >
      <div className="flex items-center gap-3 bg-amber-500/5 px-3 py-2">
        <button
          type="button"
          aria-label={`${open ? "收起" : "展开"}分支组：${group.field} = ${current.value}`}
          aria-expanded={open}
          onClick={() => setOpen(!open)}
          className="flex items-center gap-2 text-sm font-medium"
        >
          <ChevronDown
            className={`h-4 w-4 transition-transform ${open ? "" : "-rotate-90"}`}
          />
          {group.field}
        </button>
        <span className="text-xs">=</span>
        <span className="rounded border px-2 py-1 font-mono text-xs">
          {current.value || "未设置"}
        </span>
        <span className="text-xs text-muted-foreground">
          {current.items.length} 个字段
        </span>
        {onBranchChange && (
          <Button
            variant="ghost"
            size="icon"
            className="ml-auto h-7 w-7"
            aria-label={`编辑分支条件：${group.field} = ${current.value}`}
            onClick={() =>
              setDraft(
                draft ? null : { field: group.field, value: current.value },
              )
            }
          >
            <Settings2 className="h-3.5 w-3.5" />
          </Button>
        )}
      </div>
      {draft && (
        <div className="flex flex-wrap items-center gap-2 border-t p-2">
          <UXSelect
            value={String(draft.field) || "__empty__"}
            onValueChange={(value) =>
              setDraft({ ...draft, field: value === "__empty__" ? "" : value })
            }
          >
            <UXSelectTrigger
              aria-label="分支条件字段"
              className="h-8 w-48 max-w-full text-xs"
            >
              <UXSelectValue />
            </UXSelectTrigger>
            <UXSelectContent>
              {!candidates.some((f) => f.name === draft.field) && (
                <UXSelectItem value={String(draft.field) || "__empty__"}>
                  {draft.field}
                </UXSelectItem>
              )}
              {candidates.map((f, i) => (
                <UXSelectItem key={i} value={String(f.name) || "__empty__"}>
                  {f.name}
                </UXSelectItem>
              ))}
            </UXSelectContent>
          </UXSelect>
          <span>=</span>
          <Input
            aria-label="分支匹配值"
            className="h-8 w-32 font-mono text-xs"
            value={draft.value}
            onChange={(e) => setDraft({ ...draft, value: e.target.value })}
          />
          <Button
            size="sm"
            aria-label="应用分支条件"
            disabled={!draft.field || !draft.value.trim()}
            onClick={() =>
              changeCondition({ ...draft, value: draft.value.trim() })
            }
          >
            应用
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => changeCondition({ field: "", value: "" })}
          >
            移为公共字段
          </Button>
        </div>
      )}
      {(open || activeWhen) && (
        <div
          hidden={!open}
          className={activeWhen ? "runtime-fields p-3" : "space-y-1.5 p-2"}
        >
          <LayoutItems
            items={current.items}
            fields={fields}
            conditionFields={conditionFields}
            renderField={renderField}
            activeWhen={activeWhen}
            fullWidth={fullWidth}
            onBranchChange={onBranchChange}
          />
        </div>
      )}
    </section>
  );
}
function LayoutItems({
  items,
  ...props
}: FieldBranchesProps & {
  items: Layout[];
}) {
  return (
    <>
      {items.map((item) =>
        item.kind === "field" ? (
          (!props.activeWhen || props.activeWhen(item.index)) && (
            <div
              className={
                props.fullWidth?.(item.index)
                  ? "col-span-full min-w-0"
                  : "min-w-0"
              }
              key={`f-${item.index}`}
            >
              {props.renderField(item.index)}
            </div>
          )
        ) : props.activeWhen ? (
          <BranchGroup key={item.key} group={item} {...props} />
        ) : (
          // 协议编辑同时展示所有条件值，新分支不再藏在旧分支的选择器后面。
          item.choices.map((choice) => (
            <BranchGroup
              key={`${item.key}:${choice.value}`}
              group={{ ...item, choices: [choice] }}
              {...props}
            />
          ))
        ),
      )}
    </>
  );
}
export function FieldBranches(props: FieldBranchesProps) {
  const layout = useMemo(() => layoutOf(props.fields), [props.fields]);
  return (
    <div className={props.activeWhen ? "runtime-fields" : "space-y-1.5"}>
      <LayoutItems items={layout} {...props} />
    </div>
  );
}
