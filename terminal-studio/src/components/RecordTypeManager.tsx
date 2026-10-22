import { useEffect, useState } from 'react';
import { Plus, Trash2 } from 'lucide-react';
import { toast } from 'sonner';
import { listSystems } from '@/lib/api';
import { listRecordTypes, saveRecordType, deleteRecordType, type RecordType } from '@/lib/businessRules';
import type { ProtocolSystem } from '@/types';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogFooter } from '@/components/ui/dialog';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';

export function RecordTypeManager({ system: fixedSystem, onClose }: { system?: number; onClose: () => void }) {
  const [systems, setSystems] = useState<ProtocolSystem[]>([]);
  const [system, setSystem] = useState(fixedSystem ?? 0);
  const [types, setTypes] = useState<RecordType[]>([]);
  const [draft, setDraft] = useState<RecordType | null>(null);
  const [dirty, setDirty] = useState(false);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [deleting, setDeleting] = useState(false);
  useEffect(() => { let live = true; listSystems().then(rows => { if (live) { setSystems(rows); setSystem(s => s || rows[0]?.id || 0); if (!rows.length) setLoading(false); } }).catch(e => { if (live) { setError(String(e)); setLoading(false); } }); return () => { live = false; }; }, []);
  useEffect(() => { if (!system) return; let live = true; setLoading(true); setDraft(null); setDirty(false); listRecordTypes(system).then(rows => { if (live) { setTypes(rows); setDraft(rows[0] ?? null); setError(''); } }).catch(e => { if (live) setError(String(e)); }).finally(() => { if (live) setLoading(false); }); return () => { live = false; }; }, [system]);
  const change = (patch: Partial<RecordType>) => { setDraft(d => d ? { ...d, ...patch } : null); setDirty(true); };
  const persist = async () => { if (!draft) return; setBusy(true); setError(''); try { await saveRecordType(system, draft); setTypes(await listRecordTypes(system)); setDirty(false); toast.success('资料类型已保存'); } catch (e) { setError(String(e)); } finally { setBusy(false); } };
  const remove = async () => { if (!draft) return; setBusy(true); setError(''); try { await deleteRecordType(system, draft.kind); const rows = await listRecordTypes(system); setTypes(rows); setDraft(rows[0] ?? null); setDeleting(false); setDirty(false); } catch (e) { setError(String(e)); setDeleting(false); } finally { setBusy(false); } };
  return <Dialog open onOpenChange={open => { if (!open && !busy) { if (dirty) toast.info('请先保存类型或撤销修改'); else onClose(); } }}><DialogContent className="max-w-3xl"><DialogHeader><DialogTitle>资料类型</DialogTitle><DialogDescription>同一系统的协议可共用类型；字段值的编码和长度由协议定义。</DialogDescription></DialogHeader>
    <div className="space-y-4 min-h-0 overflow-auto p-1">
      <div className="flex items-center gap-3"><span className="text-sm shrink-0">所属系统</span><Select value={String(system || '')} disabled={Boolean(fixedSystem) || busy || dirty} onValueChange={v => setSystem(Number(v))}><SelectTrigger aria-label="资料类型所属系统" className="max-w-sm"><SelectValue placeholder="先创建系统" /></SelectTrigger><SelectContent>{systems.map(s => <SelectItem value={String(s.id)} key={s.id}>{s.name}</SelectItem>)}</SelectContent></Select></div>
      <div className="grid grid-cols-[180px_minmax(0,1fr)] gap-4">
        <div className="rounded-lg border p-2 space-y-1 max-h-[430px] overflow-y-auto">{types.map(t => <button key={t.kind} disabled={busy || dirty} className={`w-full text-left rounded-md px-3 py-2 text-sm truncate disabled:opacity-60 ${draft?.kind === t.kind ? 'bg-primary/10 text-primary' : 'hover:bg-muted'}`} onClick={() => { setDraft(t); setError(''); setDeleting(false); }}>{t.name}</button>)}<Button variant="outline" size="sm" className="w-full mt-2" disabled={busy || loading || dirty || !system} onClick={() => { setDraft({ kind: `type_${crypto.randomUUID()}`, name: '', device_required: true, key_scope: 'device', properties: [] }); setDirty(true); setError(''); }}><Plus />新增类型</Button></div>
        {draft && <div className="min-w-0 space-y-4">
          <label className="block space-y-1.5 text-sm">类型名称<Input aria-label="类型名称" value={draft.name} disabled={busy} onChange={e => change({ name: e.target.value })} placeholder="例如：电话、端口、传感器" /></label>
          <label className="flex gap-2 items-center text-sm"><input type="checkbox" aria-label="必须关联设备" checked={draft.device_required} disabled={busy || draft.kind === 'device'} onChange={e => change({ device_required: e.target.checked, key_scope: e.target.checked ? draft.key_scope : 'channel' })} />必须关联设备</label>
          <label className="block space-y-1.5 text-sm"><span>唯一标识范围</span><Select value={draft.key_scope} disabled={busy || draft.kind === 'device'} onValueChange={v => change({ key_scope: v as RecordType['key_scope'], device_required: v === 'device' || draft.device_required })}><SelectTrigger aria-label="唯一标识范围"><SelectValue /></SelectTrigger><SelectContent><SelectItem value="channel">同一系统、通道内唯一</SelectItem><SelectItem value="device">所属设备内唯一</SelectItem></SelectContent></Select></label>
          <div className="space-y-2"><div className="flex justify-between items-center text-sm"><span>资料属性</span><Button variant="outline" size="sm" disabled={busy} onClick={() => change({ properties: [...draft.properties, ''] })}><Plus />添加属性</Button></div><div className="max-h-56 overflow-auto space-y-2 pr-1">{draft.properties.map((p, i) => <div key={i} className="flex gap-2"><Input aria-label={`资料属性 ${i + 1}`} value={p} disabled={busy} onChange={e => change({ properties: draft.properties.map((v, j) => j === i ? e.target.value : v) })} /><Button variant="ghost" size="icon" disabled={busy} aria-label={`移除资料属性 ${i + 1}`} onClick={() => change({ properties: draft.properties.filter((_, j) => j !== i) })}><Trash2 /></Button></div>)}{!draft.properties.length && <p className="text-sm text-muted-foreground py-2">尚未定义属性</p>}</div></div>
          <div className="flex items-center gap-2 pt-2"><Button disabled={busy || !dirty} onClick={() => void persist()}>保存类型</Button>{dirty && <Button variant="outline" disabled={busy} onClick={() => { setDraft(types.find(t => t.kind === draft.kind) ?? types[0] ?? null); setDirty(false); setError(''); }}>撤销修改</Button>}{draft.kind !== 'device' && types.some(t => t.kind === draft.kind) && <Button variant="ghost" disabled={busy || dirty} className="ml-auto text-destructive" onClick={() => setDeleting(true)}><Trash2 />删除类型</Button>}</div>
          {deleting && <div role="alert" className="border rounded-md p-3 space-y-2 text-sm"><p>删除类型“{draft.name}”？仍有规则或资料引用时不能删除。</p><Button variant="destructive" size="sm" disabled={busy} onClick={() => void remove()}>确认删除类型</Button><Button variant="ghost" size="sm" onClick={() => setDeleting(false)}>取消</Button></div>}
        </div>}
      </div>
      {loading && <p className="text-sm text-muted-foreground">加载中…</p>}{error && <p role="alert" className="text-sm text-destructive">{error}</p>}
    </div><DialogFooter>{dirty && <span className="mr-auto text-sm text-muted-foreground">有未保存的修改</span>}<Button variant="outline" disabled={busy || dirty} onClick={onClose}>完成</Button></DialogFooter>
  </DialogContent></Dialog>;
}
