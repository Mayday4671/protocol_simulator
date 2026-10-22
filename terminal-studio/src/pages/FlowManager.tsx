import { useCallback, useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import {
  ReactFlow,
  Background,
  BackgroundVariant,
  Controls,
  Handle,
  Position,
  addEdge,
  useEdgesState,
  useNodesState,
  type Connection,
  type Edge,
  type Node,
  type NodeProps,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { GitBranch, Play, Plus, Save, Trash2, Workflow } from "lucide-react";
import { cn } from "@/lib/utils";
import { protocolFrame } from "@/lib/deviceProfiles";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Select as UXSelect,
  SelectTrigger as UXSelectTrigger,
  SelectValue as UXSelectValue,
  SelectContent as UXSelectContent,
  SelectItem as UXSelectItem,
} from "@/components/ui/select";
import {
  listFlows,
  upsertFlow,
  deleteFlow,
  listTerminals,
  listProtocols,
  listSystems,
  listGroups,
  listDictSets,
  type FlowRow,
  type FlowStep,
  type SignalDef,
  type SignalField,
} from "@/lib/api";
import { FIELD_ROLE_OPTIONS } from "@/components/protocols/fieldRoles";
import { FlowTestView } from "@/components/flows/FlowTestView";
import { FlowMatchEditor } from "@/components/flows/FlowMatchEditor";
import type { DictSetView, Protocol, ProtocolGroup, ProtocolSystem, TerminalProfile } from "@/types";

/** 出边句柄: 不同分支不同颜色, 连线即分支 */
const HANDLES: { key: keyof FlowStep; field: string; label: string; color: string }[] = [
  { key: "on_success", field: "on_success", label: "成功", color: "#10b981" },
  { key: "on_failure", field: "on_failure", label: "失败", color: "#ef4444" },
  { key: "on_timeout", field: "on_timeout", label: "超时", color: "#f59e0b" },
  { key: "next", field: "next", label: "下一步", color: "#64748b" },
];

type StepNodeData = {
  step: FlowStep;
  protocolName: string;
  expectName: string;
  status?: string;
  selected?: boolean;
  isStart?: boolean;
  isEnd?: boolean;
};

const STATUS_RING: Record<string, string> = {
  running: "ring-2 ring-sky-400",
  success: "ring-2 ring-emerald-500",
  failure: "ring-2 ring-red-500",
  timeout: "ring-2 ring-amber-500",
};

function StepNodeCard({ data }: NodeProps) {
  const { step, protocolName, expectName, status, selected, isStart, isEnd } = data as StepNodeData;
  return (
    <div
      className={cn(
        "w-64 rounded-lg border bg-card px-3 py-2 text-left shadow-sm",
        selected ? "border-primary" : "border-border",
        STATUS_RING[status ?? ""],
      )}
    >
      <Handle type="target" position={Position.Top} className="!h-2.5 !w-2.5" />
      {isStart && (
        <p className="mb-1 inline-flex items-center gap-1 self-start rounded-full bg-emerald-600 px-2 py-0.5 text-[11px] font-medium text-white">
          ▶ 开始
        </p>
      )}
      <p className="truncate text-sm font-medium">{step.name || "（未命名步骤）"}</p>
      <p className="truncate text-xs text-muted-foreground">{protocolName}</p>
      <p className="truncate text-xs text-muted-foreground">
        {expectName ? `期待应答: ${expectName}` : "只发不等"}
        {step.terminal_mode === "each" ? " · 逐台遍历" : ""}
      </p>
      {/* 结束节点: 无任何出边时显示终点标记(不再画四个无用的出口) */}
      {isEnd ? (
        <p className="mt-1.5 inline-flex items-center gap-1 self-start rounded-full bg-slate-500 px-2 py-0.5 text-[11px] font-medium text-white">
          ■ 结束
        </p>
      ) : (
        <div className="mt-1.5 flex gap-1">
          {HANDLES.filter((h) => (step as unknown as Record<string, unknown>)[h.field]).map((h) => (
            <div
              key={h.field}
              className="relative flex-1 rounded border px-1 py-0.5 text-center text-[11px] font-medium"
              style={{ color: h.color, borderColor: `${h.color}66` }}
            >
              {h.label}
              <Handle
                id={h.field}
                type="source"
                position={Position.Bottom}
                style={{ background: h.color }}
                className="!h-2.5 !w-2.5"
              />
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

const nodeTypes = { step: StepNodeCard };

interface DraftState {
  id: number;
  name: string;
  system_ref: number | null;
  group_ref: number | null;
  description: string;
  steps: FlowStep[];
  trigger_protocols: string[];
  schedule_interval_ms: number;
}

const emptyDraft = (systemId: number | null): DraftState => ({
  id: 0,
  name: "",
  system_ref: systemId,
  group_ref: null,
  description: "",
  steps: [],
  trigger_protocols: [],
  schedule_interval_ms: 0,
});

/** 内嵌信令字段编辑器: 名/长/值/字典/用途 一行一条(完整编辑仍可去协议管理) */
function SignalDefEditor({ def, dicts, onChange }: {
  def: SignalDef;
  dicts: DictSetView[];
  onChange: (d: SignalDef) => void;
}) {
  const patchField = (i: number, patch: Partial<SignalField>) =>
    onChange({ ...def, fields: def.fields.map((f, j) => (j === i ? { ...f, ...patch } : f)) });
  const scopedDicts = dicts.filter((d) => !d.system_ref && !d.group_ref);
  return (
    <div className="space-y-1.5 rounded-md border p-2">
      <Input
        aria-label="信令名称"
        className="h-8"
        placeholder="信令名称(如 开机注册)"
        value={def.name}
        onChange={(e) => onChange({ ...def, name: e.target.value })}
      />
      {def.fields.map((f, i) => (
        <div key={i} className="flex items-center gap-1">
          <Input
            aria-label={`字段名${i + 1}`}
            className="h-7 min-w-0 flex-1"
            placeholder="字段名"
            value={f.name}
            onChange={(e) => patchField(i, { name: e.target.value })}
          />
          <Input
            aria-label={`长度${i + 1}`}
            className="h-7 w-12"
            type="number"
            value={f.len}
            onChange={(e) => patchField(i, { len: Number(e.target.value) || 1 })}
          />
          <Input
            aria-label={`值${i + 1}`}
            className="h-7 min-w-0 flex-1 font-mono"
            placeholder="值/变量"
            value={f.value}
            onChange={(e) => patchField(i, { value: e.target.value })}
          />
          <UXSelect value={f.dict ?? "__none__"} onValueChange={(v) => patchField(i, { dict: v === "__none__" ? null : v })}>
            <UXSelectTrigger aria-label={`字典${i + 1}`} className="h-7 w-24 shrink-0 text-xs">
              <UXSelectValue />
            </UXSelectTrigger>
            <UXSelectContent>
              <UXSelectItem value="__none__">无字典</UXSelectItem>
              {scopedDicts.map((d) => (
                <UXSelectItem key={d.key} value={d.key}>{d.name}</UXSelectItem>
              ))}
            </UXSelectContent>
          </UXSelect>
          <UXSelect value={f.role ?? "none"} onValueChange={(v) => patchField(i, { role: v })}>
            <UXSelectTrigger aria-label={`用途${i + 1}`} className="h-7 w-24 shrink-0 text-xs">
              <UXSelectValue />
            </UXSelectTrigger>
            <UXSelectContent>
              <UXSelectItem value="none">无</UXSelectItem>
              {FIELD_ROLE_OPTIONS.map((o) => (
                <UXSelectItem key={o.value} value={o.value}>{o.label}</UXSelectItem>
              ))}
            </UXSelectContent>
          </UXSelect>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7 shrink-0 text-destructive hover:text-destructive"
            aria-label={`删除字段${i + 1}`}
            onClick={() => onChange({ ...def, fields: def.fields.filter((_, j) => j !== i) })}
          >
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
        </div>
      ))}
      <Button
        variant="outline"
        size="sm"
        className="w-full"
        onClick={() =>
          onChange({ ...def, fields: [...def.fields, { name: `字段${def.fields.length + 1}`, len: 1, enc: "hex", value: "", dict: null, role: "none" }] })
        }
      >
        <Plus /> 添加字段
      </Button>
    </div>
  );
}

let stepSeq = 1;

export function FlowManager() {
  const [flows, setFlows] = useState<FlowRow[]>([]);
  const [groups, setGroups] = useState<ProtocolGroup[]>([]);
  const [dicts, setDicts] = useState<DictSetView[]>([]);
  const [systems, setSystems] = useState<ProtocolSystem[]>([]);
  const [protocols, setProtocols] = useState<Protocol[]>([]);
  const [terminals, setTerminals] = useState<TerminalProfile[]>([]);
  const [draft, setDraft] = useState<DraftState | null>(null);
  const [selectedStepId, setSelectedStepId] = useState<string | null>(null);
  const [testing, setTesting] = useState<FlowRow | null>(null);
  const [saving, setSaving] = useState(false);

  const refreshFlows = useCallback(async () => {
    try {
      setFlows(await listFlows());
      setSystems(await listSystems());
      setProtocols(await listProtocols());
      setGroups(await listGroups());
      setDicts(await listDictSets());
    } catch (e) {
      toast.error("加载业务流程失败", { description: String(e) });
    }
  }, []);

  useEffect(() => {
    void refreshFlows();
  }, [refreshFlows]);

  // 选中流程时载入设备集
  useEffect(() => {
    if (!draft?.system_ref) {
      setTerminals([]);
      return;
    }
    listTerminals()
      .then((rows) => {
        const scoped = rows.filter((t) => t.system_ref === draft.system_ref);
        setTerminals(scoped);
      })
      .catch(() => setTerminals([]));
  }, [draft?.system_ref, draft?.id]);

  const protoName = useCallback(
    (pid?: string | null) =>
      protocols.find((p) => p.protocol_id === pid)?.name ?? pid ?? "",
    [protocols],
  );

  const sendProtocols = useMemo(
    () => protocols.filter((p) => p.system_ref === draft?.system_ref && p.msg_type !== "Response"),
    [protocols, draft?.system_ref],
  );
  const expectProtocols = useMemo(
    () => protocols.filter((p) => p.system_ref === draft?.system_ref && p.msg_type === "Response"),
    [protocols, draft?.system_ref],
  );
  const selectedStep = draft?.steps.find((s) => s.id === selectedStepId) ?? null;

  const [nodes, setNodes, onNodesChange] = useNodesState<Node<StepNodeData>>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([]);

  // 草稿 → 画布
  useEffect(() => {
    if (!draft) {
      setNodes([]);
      setEdges([]);
      return;
    }
    setNodes(
      draft.steps.map((step) => ({
        id: step.id,
        type: "step",
        position: { x: step.x, y: step.y },
        data: {
          step,
          protocolName: protoName(step.protocol_id),
          expectName: protoName(step.expect_protocol),
          selected: step.id === selectedStepId,
          isStart: step.id === draft.steps[0]?.id,
          isEnd: !HANDLES.some((h) => (step as unknown as Record<string, unknown>)[h.field]),
        },
        selected: step.id === selectedStepId,
      })),
    );
    const list: Edge[] = [];
    for (const step of draft.steps) {
      for (const h of HANDLES) {
        const to = (step as unknown as Record<string, unknown>)[h.field] as string | null | undefined;
        if (to) {
          list.push({
            id: `${step.id}:${h.field}:${to}`,
            source: step.id,
            sourceHandle: h.field,
            target: to,
            animated: false,
            style: { stroke: h.color, strokeWidth: 2 },
            label: h.field === "next" ? undefined : h.label,
            labelStyle: { fill: h.color },
          });
        }
      }
      setEdges(list);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [draft, protoName, selectedStepId]);

  const patchStep = useCallback(
    (id: string, patch: Partial<FlowStep>) => {
      setDraft((d) =>
        d ? { ...d, steps: d.steps.map((s) => (s.id === id ? { ...s, ...patch } : s)) } : d,
      );
    },
    [],
  );

  const onConnect = useCallback(
    (c: Connection) => {
      if (!draft || !c.source || !c.target || c.source === c.target) return;
      const handleField = (c.sourceHandle ?? "next") as string;
      patchStep(c.source, { [handleField]: c.target } as Partial<FlowStep>);
      setEdges((eds) =>
        addEdge(
          { ...c, style: { stroke: HANDLES.find((h) => h.field === handleField)?.color } },
          eds,
        ),
      );
    },
    [draft, patchStep, setEdges],
  );

  // 拖动结束保存坐标
  const onNodeDragStop = useCallback(
    (_: unknown, node: Node) => {
      if (draft) patchStep(node.id, { x: Math.round(node.position.x), y: Math.round(node.position.y) });
    },
    [draft, patchStep],
  );

  const addStep = () => {
    if (!draft) return;
    const id = `s${Date.now().toString(36)}${stepSeq++}`;
    setDraft((d) => {
      if (!d) return d;
      return {
        ...d,
        steps: [
          ...d.steps,
          {
                id,
                name: `步骤 ${d.steps.length + 1}`,
                x: 360,
                y: 60 + d.steps.length * 175,
                protocol_id: sendProtocols[0]?.protocol_id ?? "",
                terminal_mode: "single",
                terminal_id: null,
                params: {},
                expect_protocol: expectProtocols[0]?.protocol_id ?? null,
                timeout_ms: 3000,
                delay_ms: 0,
                next: null,
                on_success: null,
                on_failure: null,
                on_timeout: null,
              },
            ],
      };
    });
    setSelectedStepId(id);
  };

  const removeStep = (id: string) => {
    if (!draft) return;
    setDraft({
      ...draft,
      steps: draft.steps
        .filter((s) => s.id !== id)
        .map((s) => {
          const clean: FlowStep = { ...s };
          for (const h of HANDLES) {
            const k = h.field as keyof FlowStep;
            if (clean[k] === id) clean[k] = null as never;
          }
          return clean;
        }),
    });
    if (selectedStepId === id) setSelectedStepId(null);
  };

  const save = async () => {
    if (!draft) return;
    if (!draft.name.trim()) return toast.error("请填写流程名称");
    if (!systems.some((system) => system.id === draft.system_ref)) return toast.error("请选择所属系统");
    if (!draft.steps.length) return toast.error("请至少添加一个步骤");
    setSaving(true);
    try {
      const saved = await upsertFlow({
        id: draft.id,
        name: draft.name,
        system_ref: draft.system_ref,
        group_ref: draft.group_ref,
        description: draft.description,
        steps: draft.steps,
        trigger_protocols: draft.trigger_protocols,
        schedule_interval_ms: draft.schedule_interval_ms,
      });
      setDraft({ ...saved, group_ref: saved.group_ref ?? null, description: saved.description ?? "" });
      toast.success("业务流程已保存");
      await refreshFlows();
    } catch (e) {
      toast.error("保存失败", { description: String(e) });
    } finally {
      setSaving(false);
    }
  };

  const openFlow = (f: FlowRow) => {
    setSelectedStepId(null);
    // XML 导入的流程可能没有画布坐标: 全部堆在 (0,0) 重叠成一张卡, 自动排成链式布局
    const steps =
      f.steps.length && f.steps.every((st) => !st.x && !st.y)
        ? f.steps.map((st, i) => ({ ...st, x: 360, y: 60 + i * 175 }))
        : f.steps;
    setDraft({
      id: f.id,
      name: f.name,
      system_ref: f.system_ref,
      group_ref: f.group_ref ?? null,
      description: f.description ?? "",
      steps,
      trigger_protocols: f.trigger_protocols,
      schedule_interval_ms: f.schedule_interval_ms,
    });
  };

  /** 分流字段候选: 期待应答协议(或内嵌定义)的字段名 */
  const routeFieldOptions = (step: FlowStep) => {
    if (step.expect_def && !step.expect_protocol) return step.expect_def.fields.map((f) => f.name);
    const p = protocols.find((x) => x.protocol_id === step.expect_protocol);
    if (!p) return [] as string[];
    return protocolFrame(p, groups.find(group => group.id === p.group_ref)).fields.map(field => field.name);
  };

  const branchTarget = (field: string) => {
    if (!selectedStep) return "";
    const v = (selectedStep as unknown as Record<string, unknown>)[field] as string | null | undefined;
    return v == null ? "" : v;
  };
  const setBranch = (field: string, value: string) => {
    if (!selectedStep) return;
    patchStep(selectedStep.id, { [field]: value || null } as Partial<FlowStep>);
  };

  if (testing) return <div className="h-full min-h-0 p-4"><FlowTestView key={testing.id} flow={testing} protocols={protocols} groups={groups} onBack={()=>setTesting(null)}/></div>;

  return (
    <div className="flex h-full min-h-0 p-5">
      {/* 左: 流程列表 */}
      <div className="mr-4 flex w-60 flex-none flex-col rounded-xl border bg-card">
        <div className="flex items-center justify-between border-b px-3 py-2.5">
          <span className="text-sm font-medium">业务流程</span>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7"
            title="新建设备流程"
            aria-label="新建设备流程"
            onClick={() => {
              setSelectedStepId(null);
              setDraft(emptyDraft(systems[0]?.id ?? null));
            }}
            disabled={!systems.length}
          >
            <Plus className="h-4 w-4" />
          </Button>
        </div>
        <div className="flex-1 overflow-y-auto p-2 text-sm">
          {!flows.length && (
            <p className="py-4 text-center text-xs text-muted-foreground">
              暂无流程, 点右上 + 新建
            </p>
          )}
          {flows.map((f) => (
            <button
              key={f.id}
              type="button"
              onClick={() => openFlow(f)}
              className={cn(
                "mb-1 flex w-full items-center gap-2 rounded-md px-2 py-2 text-left",
                draft?.id === f.id && draft?.id !== 0
                  ? "bg-secondary"
                  : "hover:bg-accent",
              )}
            >
              <Workflow className="h-4 w-4 shrink-0 text-muted-foreground" />
              <span className="min-w-0 flex-1 truncate">{f.name}</span>
              <span className="shrink-0 text-xs text-muted-foreground">{f.steps.length} 步</span>
            </button>
          ))}
        </div>
      </div>

      {/* 中: 画布 */}
      <div className="flex min-w-0 flex-1 flex-col rounded-xl border bg-card">
        <div className="flex flex-wrap items-center gap-2 border-b px-3 py-2">
          {draft ? (
            <>
              <Input
                aria-label="流程名称"
                className="h-8 w-52"
                placeholder="流程名称"
                value={draft.name}
                onChange={(e) => setDraft({ ...draft, name: e.target.value })}
              />
              <UXSelect
                value={String(draft.system_ref ?? "")}
                onValueChange={(v) => setDraft({ ...draft, system_ref: v ? Number(v) : null, group_ref: null })}
              >
                <UXSelectTrigger aria-label="流程归属系统" className="h-8 w-44">
                  <UXSelectValue />
                </UXSelectTrigger>
                <UXSelectContent>
                  {systems.map((s) => (
                    <UXSelectItem key={s.id} value={String(s.id)}>
                      {s.name}
                    </UXSelectItem>
                  ))}
                </UXSelectContent>
              </UXSelect>
              <UXSelect
                value={String(draft.group_ref ?? "")}
                onValueChange={(v) => setDraft({ ...draft, group_ref: v ? Number(v) : null })}
              >
                <UXSelectTrigger aria-label="流程归属分组" className="h-8 w-36">
                  <UXSelectValue />
                </UXSelectTrigger>
                <UXSelectContent>
                  {groups
                    .filter((g) => g.system_ref === draft.system_ref)
                    .map((g) => (
                      <UXSelectItem key={g.id} value={String(g.id)}>
                        {g.name}
                      </UXSelectItem>
                    ))}
                </UXSelectContent>
              </UXSelect>
              <Button variant="outline" size="sm" onClick={addStep} disabled={!sendProtocols.length}>
                <Plus /> 添加步骤
              </Button>
              <Button size="sm" variant="outline" onClick={save} disabled={saving}>
                <Save /> 保存
              </Button>
              <Button className="ml-auto" size="sm" disabled={!draft.id || saving} onClick={async()=>{
                try { const saved = await upsertFlow({ ...draft }); setDraft({ ...saved, group_ref:saved.group_ref??null, description:saved.description??"" }); setTesting(saved); await refreshFlows(); }
                catch(e){toast.error("保存流程失败",{description:String(e)});}
              }}><Play/>测试流程</Button>
            </>
          ) : (
            <p className="px-1 py-1 text-sm text-muted-foreground">
              从左侧选择或新设一个业务流程, 拖拽节点、连线分支即可编排协议交互链
            </p>
          )}
        </div>
        <div className="min-h-0 flex-1">
          <ReactFlow
            nodes={nodes}
            edges={edges}
            nodeTypes={nodeTypes}
            onNodesChange={onNodesChange}
            onEdgesChange={onEdgesChange}
            onConnect={onConnect}
            onNodeDragStop={onNodeDragStop}
            onNodeClick={(_, node) => setSelectedStepId(node.id)}
            onPaneClick={() => setSelectedStepId(null)}
            fitView
            proOptions={{ hideAttribution: true }}
          >
            <Background variant={BackgroundVariant.Dots} gap={16} size={1} />
            <Controls showInteractive={false} />
          </ReactFlow>
        </div>
      </div>

      {/* 右: 属性面板 + 运行日志 */}
      <div className="ml-4 flex w-80 flex-none flex-col gap-3">
        <div className="flex min-h-0 flex-1 flex-col overflow-y-auto rounded-xl border bg-card p-3">
          {draft && !selectedStepId && (
            <div className="space-y-3">
              <p className="text-sm font-medium">流程设置</p>
              <label className="grid gap-1 text-xs">
                描述
                <Input
                  className="h-8"
                  value={draft.description ?? ""}
                  onChange={(e) => setDraft({ ...draft, description: e.target.value })}
                />
              </label>
              <div>
                <p className="mb-1 text-xs font-medium">收到协议自动运行</p>
                <div className="flex flex-wrap gap-1.5">
                  {protocols
                    .filter((p) => p.system_ref === draft.system_ref && p.msg_type === "Response")
                    .map((p) => {
                      const on = draft.trigger_protocols.includes(p.protocol_id);
                      return (
                        <button
                          key={p.protocol_id}
                          type="button"
                          aria-pressed={on}
                          onClick={() =>
                            setDraft({
                              ...draft,
                              trigger_protocols: on
                                ? draft.trigger_protocols.filter((x) => x !== p.protocol_id)
                                : [...draft.trigger_protocols, p.protocol_id],
                            })
                          }
                          className={cn(
                            "rounded-full border px-2.5 py-1 text-xs transition-colors",
                            on
                              ? "border-primary bg-primary text-primary-foreground"
                              : "text-muted-foreground hover:border-primary/50",
                          )}
                        >
                          {p.name}
                        </button>
                      );
                    })}
                </div>
                <p className="mt-1 text-[11px] leading-4 text-muted-foreground">
                  勾选的应答协议被收到时, 本流程自动运行(已在跑不重复)
                </p>
              </div>
              <label className="grid gap-1 text-xs">
                定时触发间隔(毫秒, 0 = 不定时)
                <Input
                  className="h-8"
                  type="number"
                  value={draft.schedule_interval_ms}
                  onChange={(e) =>
                    setDraft({ ...draft, schedule_interval_ms: Number(e.target.value) || 0 })
                  }
                />
              </label>
              {draft.id > 0 && (
                <Button
                  variant="ghost"
                  size="sm"
                  className="justify-start text-destructive hover:text-destructive"
                  onClick={async () => {
                    if (!window.confirm(`确定删除流程「${draft.name}」?`)) return;
                    try {
                      await deleteFlow(draft.id);
                      setDraft(null);
                      await refreshFlows();
                      toast.success("流程已删除");
                    } catch (e) {
                      toast.error("删除失败", { description: String(e) });
                    }
                  }}
                >
                  <Trash2 /> 删除此流程
                </Button>
              )}
            </div>
          )}
          {selectedStep && (
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <p className="text-sm font-medium">步骤属性</p>
                <Button
                  variant="ghost"
                  size="icon"
                  className="h-7 w-7 text-destructive hover:text-destructive"
                  aria-label="删除步骤"
                  onClick={() => removeStep(selectedStep.id)}
                >
                  <Trash2 className="h-4 w-4" />
                </Button>
              </div>
              <label className="grid gap-1 text-xs">
                步骤名称
                <Input
                  className="h-8"
                  value={selectedStep.name}
                  onChange={(e) => patchStep(selectedStep.id, { name: e.target.value })}
                />
              </label>
              <div className="grid gap-1 text-xs">
                <span>{selectedStep.action === "receive" ? "接收信令" : "发送信令"}{selectedStep.protocol_id ? "" : selectedStep.request_def ? "(内嵌定义, 保存自动入协议库)" : ""}</span>
                {selectedStep.request_def && !selectedStep.protocol_id ? (
                  <div className="space-y-1.5">
                    <SignalDefEditor
                      def={selectedStep.request_def}
                      dicts={dicts}
                      onChange={(d) => patchStep(selectedStep.id, { request_def: d })}
                    />
                    <Button variant="ghost" size="sm" onClick={() => patchStep(selectedStep.id, { request_def: null })}>
                      改用现有协议
                    </Button>
                  </div>
                ) : (
                  <UXSelect
                    value={selectedStep.protocol_id || "__empty__"}
                    onValueChange={(v) =>
                      patchStep(
                        selectedStep.id,
                        v === "__inline__"
                          ? { request_def: { name: selectedStep.name, fields: [] }, protocol_id: "" }
                          : { protocol_id: v === "__empty__" ? "" : v, request_def: null },
                      )
                    }
                  >
                    <UXSelectTrigger aria-label="步骤发送协议" className="h-8">
                      <UXSelectValue />
                    </UXSelectTrigger>
                    <UXSelectContent>
                      <UXSelectItem value="__inline__">＋ 内嵌定义新信令…</UXSelectItem>
                      {protocols.filter(p=>p.system_ref===draft?.system_ref).map((p) => (
                        <UXSelectItem key={p.protocol_id} value={p.protocol_id}>
                          {p.name}
                        </UXSelectItem>
                      ))}
                    </UXSelectContent>
                  </UXSelect>
                )}
              </div>
              <FlowMatchEditor step={selectedStep} protocols={protocols} groups={groups} patch={p=>patchStep(selectedStep.id,p)}/>
              <div className="grid grid-cols-2 gap-2">
                <label className="grid gap-1 text-xs">
                  设备模式
                  <UXSelect
                    value={selectedStep.terminal_mode || "single"}
                    onValueChange={(v) => patchStep(selectedStep.id, { terminal_mode: v })}
                  >
                    <UXSelectTrigger aria-label="步骤设备模式" className="h-8">
                      <UXSelectValue />
                    </UXSelectTrigger>
                    <UXSelectContent>
                      <UXSelectItem value="single">单台设备</UXSelectItem>
                      <UXSelectItem value="each">逐台遍历</UXSelectItem>
                    </UXSelectContent>
                  </UXSelect>
                </label>
                <label className="grid gap-1 text-xs">
                  固定设备
                  <UXSelect
                    value={String(selectedStep.terminal_id ?? "") || "__empty__"}
                    onValueChange={(v) =>
                      patchStep(selectedStep.id, {
                        terminal_id: v === "__empty__" ? null : Number(v),
                      })
                    }
                    disabled={selectedStep.terminal_mode === "each"}
                  >
                    <UXSelectTrigger aria-label="步骤固定设备" className="h-8">
                      <UXSelectValue />
                    </UXSelectTrigger>
                    <UXSelectContent>
                      <UXSelectItem value="__empty__">运行时第一台</UXSelectItem>
                      {terminals.map((t) => (
                        <UXSelectItem key={t.id} value={String(t.id)}>
                          {t.name}
                        </UXSelectItem>
                      ))}
                    </UXSelectContent>
                  </UXSelect>
                </label>
              </div>
              <div className="grid gap-1 text-xs">
                <span>期待应答{selectedStep.expect_def && !selectedStep.expect_protocol ? "(内嵌定义, 保存自动入协议库)" : ""}</span>
                {selectedStep.expect_def && !selectedStep.expect_protocol ? (
                  <div className="space-y-1.5">
                    <SignalDefEditor
                      def={selectedStep.expect_def}
                      dicts={dicts}
                      onChange={(d) => patchStep(selectedStep.id, { expect_def: d })}
                    />
                    <Button variant="ghost" size="sm" onClick={() => patchStep(selectedStep.id, { expect_def: null })}>
                      改用现有应答协议
                    </Button>
                  </div>
                ) : (
                  <UXSelect
                    value={selectedStep.expect_protocol || "__empty__"}
                    onValueChange={(v) =>
                      patchStep(
                        selectedStep.id,
                        v === "__inline__"
                          ? { expect_def: { name: `${selectedStep.name}应答`, fields: [] }, expect_protocol: null }
                          : { expect_protocol: v === "__empty__" ? null : v, expect_def: null },
                      )
                    }
                  >
                    <UXSelectTrigger aria-label="步骤期待应答" className="h-8">
                      <UXSelectValue />
                    </UXSelectTrigger>
                    <UXSelectContent>
                      <UXSelectItem value="__empty__">只发送, 不等应答</UXSelectItem>
                      <UXSelectItem value="__inline__">＋ 内嵌定义应答信令…</UXSelectItem>
                      {expectProtocols.map((p) => (
                        <UXSelectItem key={p.protocol_id} value={p.protocol_id}>
                          {p.name}
                        </UXSelectItem>
                      ))}
                    </UXSelectContent>
                  </UXSelect>
                )}
              </div>
              <div className="grid grid-cols-2 gap-2">
                <label className="grid gap-1 text-xs">
                  超时(毫秒)
                  <Input
                    className="h-8"
                    type="number"
                    value={selectedStep.timeout_ms}
                    onChange={(e) =>
                      patchStep(selectedStep.id, { timeout_ms: Number(e.target.value) || 3000 })
                    }
                  />
                </label>
                <label className="grid gap-1 text-xs">
                  前置延时(毫秒)
                  <Input
                    className="h-8"
                    type="number"
                    value={selectedStep.delay_ms}
                    onChange={(e) =>
                      patchStep(selectedStep.id, { delay_ms: Number(e.target.value) || 0 })
                    }
                  />
                </label>
              </div>
              <label className="grid gap-1 text-xs">
                字段覆盖(每行一条: 字段名=值)
                <textarea
                  aria-label="步骤字段覆盖"
                  className="h-20 w-full rounded-md border bg-transparent px-2 py-1 font-mono text-xs"
                  placeholder={"采集周期=00 3C\n告警阈值=01 90"}
                  value={Object.entries(selectedStep.params)
                    .map(([k, v]) => `${k}=${v}`)
                    .join("\n")}
                  onChange={(e) => {
                    const params: Record<string, string> = {};
                    for (const line of e.target.value.split("\n")) {
                      const i = line.indexOf("=");
                      if (i > 0) params[line.slice(0, i).trim()] = line.slice(i + 1).trim();
                    }
                    patchStep(selectedStep.id, { params });
                  }}
                />
              </label>
              <div className="space-y-2 rounded-md border p-2">
                <p className="flex items-center gap-1 text-xs font-medium">
                  <GitBranch className="h-3.5 w-3.5" /> 分支去向
                </p>
                {(selectedStep.routes ?? []).map((r, i) => (
                  <div key={`route-${i}`} className="flex items-center gap-1" aria-label={`分流规则${i + 1}`}>
                    <span className="shrink-0 text-[11px] text-emerald-600">接收</span>
                    <UXSelect
                      value={r.field}
                      onValueChange={(v) => {
                        const routes = [...(selectedStep.routes ?? [])];
                        routes[i] = { ...r, field: v };
                        patchStep(selectedStep.id, { routes });
                      }}
                    >
                      <UXSelectTrigger aria-label={`分流字段${i + 1}`} className="h-7 w-24 shrink-0 text-xs">
                        <UXSelectValue />
                      </UXSelectTrigger>
                      <UXSelectContent>
                        {(routeFieldOptions(selectedStep)).map((n) => (
                          <UXSelectItem key={n} value={n}>{n}</UXSelectItem>
                        ))}
                      </UXSelectContent>
                    </UXSelect>
                    <Input
                      aria-label={`分流值${i + 1}`}
                      className="h-7 w-20 min-w-0 font-mono text-xs"
                      placeholder="值/含义"
                      value={r.value}
                      onChange={(e) => {
                        const routes = [...(selectedStep.routes ?? [])];
                        routes[i] = { ...r, value: e.target.value };
                        patchStep(selectedStep.id, { routes });
                      }}
                    />
                    <span className="shrink-0 text-[11px] text-muted-foreground">→</span>
                    <UXSelect
                      value={r.to}
                      onValueChange={(v) => {
                        const routes = [...(selectedStep.routes ?? [])];
                        routes[i] = { ...r, to: v };
                        patchStep(selectedStep.id, { routes });
                      }}
                    >
                      <UXSelectTrigger aria-label={`分流去向${i + 1}`} className="h-7 w-28 shrink-0 text-xs">
                        <UXSelectValue />
                      </UXSelectTrigger>
                      <UXSelectContent>
                        {draft?.steps.filter((x) => x.id !== selectedStep.id).map((x) => (
                          <UXSelectItem key={x.id} value={x.id}>{x.name}</UXSelectItem>
                        ))}
                      </UXSelectContent>
                    </UXSelect>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-7 w-7 shrink-0 text-destructive hover:text-destructive"
                      aria-label={`删除分流规则${i + 1}`}
                      onClick={() => patchStep(selectedStep.id, { routes: (selectedStep.routes ?? []).filter((_, j) => j !== i) })}
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </Button>
                  </div>
                ))}
                <Button
                  variant="outline"
                  size="sm"
                  className="w-full"
                  disabled={!routeFieldOptions(selectedStep).length}
                  onClick={() =>
                    patchStep(selectedStep.id, {
                      routes: [
                        ...(selectedStep.routes ?? []),
                        { field: routeFieldOptions(selectedStep)[0] ?? "", value: "", to: draft?.steps.find((x) => x.id !== selectedStep.id)?.id ?? "" },
                      ],
                    })
                  }
                >
                  <Plus /> 按接收内容分流
                </Button>
                {HANDLES.map((h) => (
                  <label key={h.field} className="grid gap-1 text-xs">
                    <span style={{ color: h.color }}>
                      {h.label}
                      {h.field === "next" ? "(无期待应答时)" : ""}
                    </span>
                    <UXSelect
                      value={branchTarget(h.field)}
                      onValueChange={(v) => setBranch(h.field, v)}
                    >
                      <UXSelectTrigger aria-label={`${h.label}去向`} className="h-8">
                        <UXSelectValue />
                      </UXSelectTrigger>
                      <UXSelectContent>
                        <UXSelectItem value="__empty__">流程结束</UXSelectItem>
                        {draft?.steps
                          .filter((s) => s.id !== selectedStep.id)
                          .map((s) => (
                            <UXSelectItem key={s.id} value={s.id}>
                              {s.name}
                            </UXSelectItem>
                          ))}
                      </UXSelectContent>
                    </UXSelect>
                  </label>
                ))}
              </div>
            </div>
          )}
        </div>

      </div>
    </div>
  );
}
