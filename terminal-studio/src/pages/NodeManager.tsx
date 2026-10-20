import { useCallback, useEffect, useMemo, useState } from "react";
import { toast } from "sonner";
import { CircleStop, Pencil, Play, Plus, Radio, Trash2 } from "lucide-react";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import {
  Dialog,
  DialogContent,
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
import { Switch } from "@/components/ui/switch";
import {
  Table,
  TablePanel,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  createNode,
  deleteNode as apiDeleteNode,
  listGroups,
  listNodes,
  listSystems,
  startUdpServer,
  stopUdpServer,
  updateNode as apiUpdateNode,
} from "@/lib/api";
import { useUdpLogs } from "@/hooks/useUdpLogs";
import type {
  Node,
  NodePayload,
  NodeRole,
  ProtocolSystem,
  ProtocolGroup,
} from "@/types";

interface FormState {
  name: string;
  role: NodeRole;
  local_ip: string;
  local_port: string;
  remote_ip: string;
  remote_port: string;
  enabled: boolean;
  /** 绑定的协议分组 id */
  binds: number[];
}

const emptyForm: FormState = {
  name: "",
  role: "simulator",
  local_ip: "0.0.0.0",
  local_port: "",
  remote_ip: "127.0.0.1",
  remote_port: "",
  enabled: true,
  binds: [],
};

function toForm(n: Node): FormState {
  return {
    name: n.name,
    role: n.role,
    local_ip: n.local_ip,
    local_port: String(n.local_port),
    remote_ip: n.remote_ip,
    remote_port: String(n.remote_port),
    enabled: n.enabled,
    binds: (() => {
      try {
        return JSON.parse(n.group_bind_json ?? "[]") as number[];
      } catch {
        return [];
      }
    })(),
  };
}

function toPayload(f: FormState, editing: Node | null): NodePayload {
  return {
    name: f.name.trim(),
    role: f.role,
    local_ip: f.local_ip.trim(),
    local_port: Number(f.local_port),
    remote_ip: f.remote_ip.trim(),
    remote_port: Number(f.remote_port),
    // 终端编号已不属于通道概念(终端标识随设备/帧走), 编辑老通道时原样保留
    node_id_hex: editing?.node_id_hex ?? null,
    enabled: f.enabled,
    group_binds: f.binds.length ? f.binds : null,
  };
}

function validate(f: FormState): string | null {
  if (!f.name.trim()) return "请填写通道名称";
  const lp = Number(f.local_port);
  const rp = Number(f.remote_port);
  if (!Number.isInteger(lp) || lp < 1 || lp > 65535)
    return "本地端口必须是 1-65535 的整数";
  if (!Number.isInteger(rp) || rp < 1 || rp > 65535)
    return "目标端口必须是 1-65535 的整数";
  if (!f.local_ip.trim()) return "请填写本地绑定 IP";
  if (!f.remote_ip.trim()) return "请填写目标 IP";
  return null;
}

export function NodeManager() {
  const [nodes, setNodes] = useState<Node[]>([]);
  const [systems, setSystems] = useState<ProtocolSystem[]>([]);
  const [groups, setGroups] = useState<ProtocolGroup[]>([]);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [editing, setEditing] = useState<Node | null>(null);
  const [form, setForm] = useState<FormState>(emptyForm);
  const [saving, setSaving] = useState(false);
  const { engine, refreshEngine } = useUdpLogs();

  const refresh = useCallback(async () => {
    try {
      setNodes(await listNodes());
      setSystems(await listSystems());
      setGroups(await listGroups());
    } catch (e) {
      toast.error("加载通道失败", { description: String(e) });
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const listeningSet = useMemo(
    () => new Set(engine?.listening_nodes ?? []),
    [engine],
  );

  const openCreate = () => {
    setEditing(null);
    setForm(emptyForm);
    setDialogOpen(true);
  };

  const openEdit = (n: Node) => {
    setEditing(n);
    setForm(toForm(n));
    setDialogOpen(true);
  };

  const handleSave = async () => {
    const err = validate(form);
    if (err) {
      toast.error(err);
      return;
    }
    setSaving(true);
    try {
      const payload = toPayload(form, editing);
      if (editing) {
        await apiUpdateNode(editing.id, payload);
        toast.success("通道已更新");
      } else {
        await createNode(payload);
        toast.success("通道已创建");
      }
      setDialogOpen(false);
      await refresh();
    } catch (e) {
      toast.error("保存失败", { description: String(e) });
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async (n: Node) => {
    if (!window.confirm(`确定删除通道「${n.name}」? 关联心跳绑定将一并删除`))
      return;
    try {
      await apiDeleteNode(n.id);
      toast.success("通道已删除");
      await refresh();
      await refreshEngine();
    } catch (e) {
      toast.error("删除失败", { description: String(e) });
    }
  };

  const toggleListen = async (n: Node, start: boolean) => {
    try {
      if (start) {
        await startUdpServer(n.id);
        toast.success(
          `通道「${n.name}」已开始监听 ${n.local_ip}:${n.local_port}`,
        );
      } else {
        await stopUdpServer(n.id);
        toast.info(`通道「${n.name}」已停止监听`);
      }
      await refreshEngine();
    } catch (e) {
      toast.error("操作失败", { description: String(e) });
    }
  };

  return (
    <div className="flex h-full min-h-0 flex-col p-6">
      <div className="mb-3 flex shrink-0 items-center justify-end">
        <Button onClick={openCreate}>
          <Plus /> 新增通道
        </Button>
      </div>

      <TablePanel className="min-h-0">
        <Table aria-label="通道列表" className="min-w-[960px]" containerClassName="min-h-0">
          <TableHeader>
            <TableRow>
              <TableHead className="w-12">ID</TableHead>
              <TableHead>名称</TableHead>
              <TableHead>角色</TableHead>
              <TableHead className="whitespace-nowrap">本地监听</TableHead>
              <TableHead className="whitespace-nowrap">目标地址</TableHead>
              <TableHead className="whitespace-nowrap">绑定分组</TableHead>
              <TableHead>状态</TableHead>
              <TableHead className="sticky right-0 w-64 whitespace-nowrap bg-muted text-right">
                操作
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {nodes.length === 0 && (
              <TableRow>
                <TableCell
                  colSpan={8}
                  className="h-24 text-center text-muted-foreground"
                >
                  暂无通道
                </TableCell>
              </TableRow>
            )}
            {nodes.map((n) => (
              <TableRow key={n.id}>
                <TableCell className="text-muted-foreground">{n.id}</TableCell>
                <TableCell className="font-medium">{n.name}</TableCell>
                <TableCell>
                  <Badge
                    variant={n.role === "simulator" ? "default" : "secondary"}
                  >
                    {n.role === "simulator" ? "模拟端" : "目标端"}
                  </Badge>
                </TableCell>
                <TableCell className="whitespace-nowrap font-mono text-xs">
                  {n.local_ip}:{n.local_port}
                </TableCell>
                <TableCell className="whitespace-nowrap font-mono text-xs">
                  {n.remote_ip}:{n.remote_port}
                </TableCell>
                <TableCell className="whitespace-nowrap text-xs">
                  {(() => {
                    try {
                      const ids = JSON.parse(
                        n.group_bind_json ?? "[]",
                      ) as number[];
                      if (!ids.length) return "-";
                      const bySystem = new Map<
                        number | null,
                        { label: string; names: string[] }
                      >();
                      for (const id of ids) {
                        const group = groups.find((g) => g.id === id);
                        if (!group) continue;
                        const entry = bySystem.get(group.system_ref) ?? {
                          label:
                            systems.find((s) => s.id === group.system_ref)
                              ?.name ?? "系统信息缺失",
                          names: [],
                        };
                        entry.names.push(group.name);
                        bySystem.set(group.system_ref, entry);
                      }
                      const detail = [...bySystem.values()]
                        .map(
                          ({ label, names }) =>
                            `${label}：${names.join("、")}`,
                        )
                        .join("\n");
                      return (
                        <span title={detail} className="text-muted-foreground">
                          {ids.length} 个分组
                        </span>
                      );
                    } catch {
                      return "-";
                    }
                  })()}
                </TableCell>
                <TableCell>
                  {listeningSet.has(n.id) ? (
                    <Badge className="gap-1 bg-primary text-primary-foreground">
                      <Radio className="h-3 w-3" /> 监听中
                    </Badge>
                  ) : (
                    <Badge variant="outline">未启动</Badge>
                  )}
                </TableCell>
                <TableCell className="sticky right-0 whitespace-nowrap bg-card text-right">
                  <div className="flex justify-end gap-1">
                    {listeningSet.has(n.id) ? (
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => toggleListen(n, false)}
                      >
                        <CircleStop /> 停止
                      </Button>
                    ) : (
                      <Button
                        variant="outline"
                        size="sm"
                        onClick={() => toggleListen(n, true)}
                      >
                        <Play /> 启动
                      </Button>
                    )}
                    <Button
                      variant="ghost"
                      size="icon"
                      onClick={() => openEdit(n)}
                      title="编辑"
                    >
                      <Pencil />
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="text-destructive hover:text-destructive"
                      onClick={() => handleDelete(n)}
                      title="删除"
                    >
                      <Trash2 />
                    </Button>
                  </div>
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </TablePanel>

      <Dialog open={dialogOpen} onOpenChange={setDialogOpen}>
        <DialogContent className="max-w-xl" aria-describedby={undefined}>
          <DialogHeader>
            <DialogTitle>{editing ? "编辑通道" : "新增通道"}</DialogTitle>
          </DialogHeader>
          <div className="grid grid-cols-1 gap-4 sm:grid-cols-2">
            <div className="col-span-2 grid gap-2">
              <Label htmlFor="n-name">通道名称 *</Label>
              <Input
                id="n-name"
                value={form.name}
                placeholder="如: 1号模拟终端"
                onChange={(e) => setForm({ ...form, name: e.target.value })}
              />
            </div>
            <div className="grid gap-2">
              <Label htmlFor="n-role">角色</Label>
              <Select
                value={form.role}
                onValueChange={(v) => setForm({ ...form, role: v as NodeRole })}
              >
                <SelectTrigger id="n-role">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="simulator">模拟端 (主动收发)</SelectItem>
                  <SelectItem value="target">目标端 (被动应答)</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div className="grid gap-2">
              <Label htmlFor="n-lip">本地绑定 IP *</Label>
              <Input
                id="n-lip"
                value={form.local_ip}
                onChange={(e) => setForm({ ...form, local_ip: e.target.value })}
              />
            </div>
            <div className="grid gap-2">
              <Label htmlFor="n-lport">本地端口 *</Label>
              <Input
                id="n-lport"
                type="number"
                value={form.local_port}
                placeholder="1-65535"
                onChange={(e) =>
                  setForm({ ...form, local_port: e.target.value })
                }
              />
            </div>
            <div className="grid gap-2">
              <Label htmlFor="n-rip">目标 IP *</Label>
              <Input
                id="n-rip"
                value={form.remote_ip}
                onChange={(e) =>
                  setForm({ ...form, remote_ip: e.target.value })
                }
              />
            </div>
            <div className="grid gap-2">
              <Label htmlFor="n-rport">目标端口 *</Label>
              <Input
                id="n-rport"
                type="number"
                value={form.remote_port}
                placeholder="1-65535"
                onChange={(e) =>
                  setForm({ ...form, remote_port: e.target.value })
                }
              />
            </div>
            <div className="col-span-2 flex items-center gap-3">
              <Switch
                id="n-enabled"
                checked={form.enabled}
                onCheckedChange={(v) => setForm({ ...form, enabled: v })}
              />
              <Label htmlFor="n-enabled">启用通道</Label>
            </div>
            <div className="col-span-2 grid gap-2">
              <Label>绑定协议分组</Label>
              <div className="max-h-36 overflow-y-auto rounded-md border p-2">
                {systems.length === 0 && (
                  <p className="py-2 text-center text-xs text-muted-foreground">
                    暂无协议系统
                  </p>
                )}
                {systems.map((sy) => (
                  <div key={sy.id} className="mb-1.5">
                    <p className="mb-1 text-xs font-medium text-muted-foreground">
                      {sy.name}
                    </p>
                    <div className="flex flex-wrap gap-1.5">
                      {groups
                        .filter((g) => g.system_ref === sy.id)
                        .map((g) => {
                          const on = form.binds.includes(g.id);
                          return (
                            <button
                              key={g.id}
                              type="button"
                              aria-label={`${sy.name} / ${g.name}`}
                              aria-pressed={on}
                              className={cn(
                                "rounded-full border px-3 py-1 text-xs transition-colors",
                                on
                                  ? "border-primary bg-primary text-primary-foreground"
                                  : "text-muted-foreground hover:border-primary/50 hover:text-foreground",
                              )}
                              onClick={() =>
                                setForm({
                                  ...form,
                                  binds: on
                                    ? form.binds.filter((x) => x !== g.id)
                                    : [...form.binds, g.id],
                                })
                              }
                            >
                              {g.name}
                            </button>
                          );
                        })}
                    </div>
                  </div>
                ))}
              </div>
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
    </div>
  );
}
