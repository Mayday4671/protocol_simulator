import { useCallback, useEffect, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { RefreshCw, Trash2, Settings2 } from 'lucide-react';
import { toast } from 'sonner';
import { listBusinessRecords, deleteBusinessRecord, kindName, listRecordTypes, type RecordType, type BusinessRecord, type BusinessCell } from '@/lib/businessRules';
import { RecordTypeManager } from '@/components/RecordTypeManager';
import { listNodes, listSystems } from '@/lib/api';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogFooter } from '@/components/ui/dialog';

export function BusinessRecords() {
  const [records, setRecords] = useState<BusinessRecord[]>([]);
  const [names, setNames] = useState<Record<string, string>>({});
  const [query, setQuery] = useState('');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState('');
  const [deleting, setDeleting] = useState<BusinessRecord | null>(null);
  const [manage, setManage] = useState(false);
  const [types, setTypes] = useState<(RecordType & { system: number })[]>([]);
  const [system, setSystem] = useState('all');
  const [kind, setKind] = useState('all');
  const [device, setDevice] = useState('all');
  const refresh = useCallback(async () => { setLoading(true); try { const [rows, nodes, systems] = await Promise.all([listBusinessRecords(), listNodes(), listSystems()]); const definitions = await Promise.all(systems.map(async s => (await listRecordTypes(s.id)).map(t => ({ ...t, system: s.id })))); setTypes(definitions.flat()); setRecords(rows); setNames(Object.fromEntries([...nodes.map(n => [`n${n.id}`, n.name]), ...systems.map(s => [`s${s.id}`, s.name])])); setError(''); } catch (e) { setError(String(e)); } finally { setLoading(false); } }, []);
  useEffect(() => { void refresh(); const listener = listen('business-records-updated', () => void refresh()); return () => { void listener.then(f => f()); }; }, [refresh]);
  const title = (r: BusinessRecord) => types.find(t => t.system === r.system_ref && t.kind === r.kind)?.name ?? kindName(r.kind);
  const visible = records.filter(r => (system === 'all' || r.system_ref === Number(system)) && (kind === 'all' || `${r.system_ref}:${r.kind}` === kind) && (device === 'all' || String(r.device_ref) === device) && `${title(r)} ${r.record_key} ${r.name} ${r.device_name ?? ''}`.toLowerCase().includes(query.toLowerCase()));
  const devices = [...new Map(records.filter(r => r.device_ref && (system === 'all' || r.system_ref === Number(system))).map(r => [r.device_ref!, r])).values()];
  return <div className="flex h-full min-h-0 flex-col gap-3"><div className="flex items-center gap-2 flex-wrap"><Input aria-label="搜索业务资料" className="w-60" placeholder="搜索标识、名称或设备" value={query} onChange={e => setQuery(e.target.value)} /><Select value={system} onValueChange={v => { setSystem(v); setKind('all'); setDevice('all'); }}><SelectTrigger className="w-44" aria-label="资料系统"><SelectValue /></SelectTrigger><SelectContent><SelectItem value="all">全部系统</SelectItem>{Object.entries(names).filter(([k]) => k.startsWith('s')).map(([k, n]) => <SelectItem key={k} value={k.slice(1)}>{n}</SelectItem>)}</SelectContent></Select><Select value={kind} onValueChange={setKind}><SelectTrigger className="w-44" aria-label="资料类型筛选"><SelectValue /></SelectTrigger><SelectContent><SelectItem value="all">全部类型</SelectItem>{types.filter(t => system === 'all' || t.system === Number(system)).map(t => <SelectItem key={`${t.system}:${t.kind}`} value={`${t.system}:${t.kind}`}>{t.name}{system === 'all' ? ` · ${names[`s${t.system}`]}` : ''}</SelectItem>)}</SelectContent></Select><Select value={device} onValueChange={setDevice}><SelectTrigger className="w-44" aria-label="所属设备筛选"><SelectValue /></SelectTrigger><SelectContent><SelectItem value="all">全部设备</SelectItem>{devices.map(r => <SelectItem key={r.device_ref} value={String(r.device_ref)}>{r.device_name} · {names[`n${r.node_ref}`]}</SelectItem>)}</SelectContent></Select><Button variant="outline" disabled={loading} onClick={() => void refresh()}><RefreshCw className={loading ? 'animate-spin' : ''} />刷新</Button><Button variant="outline" onClick={() => setManage(true)}><Settings2 />资料类型</Button><span className="text-sm text-muted-foreground">{visible.length} 条</span></div>
    {error && <p className="text-destructive text-sm" role="alert">{error}</p>}
    <div className="min-h-0 flex-1 overflow-auto rounded-lg border bg-card divide-y">
      {visible.map(record => {
        let properties: Record<string, BusinessCell> = {}; try { properties = JSON.parse(record.properties_json); } catch { /* Show the record even if its properties are damaged. */ }
        return <details key={record.id} className="group"><summary className="cursor-pointer px-4 py-3 flex flex-wrap items-center gap-x-4 gap-y-1 hover:bg-muted/40"><span className="text-xs rounded bg-primary/10 text-primary px-2 py-1">{title(record)}</span><strong className="text-sm">{record.name}</strong><code className="text-xs text-muted-foreground">{record.record_key}</code>{record.device_name && <span className="text-sm">所属设备：{record.device_name}</span>}<span className="ml-auto text-xs text-muted-foreground">{names[`s${record.system_ref}`]} · {names[`n${record.node_ref}`]} · {record.updated_at}</span>{record.kind !== 'device' && <Button variant="ghost" size="icon" aria-label={`删除资料 ${record.name}`} onClick={e => { e.preventDefault(); setDeleting(record); }}><Trash2 className="text-destructive" /></Button>}</summary><div className="px-4 pb-4 grid grid-cols-[repeat(auto-fit,minmax(180px,1fr))] gap-2">{Object.entries(properties).map(([key, value]) => <div key={key} className="rounded-md border px-3 py-2 min-w-0"><div className="text-xs text-muted-foreground mb-1">{key} · {value.bytes} B</div><div className="font-mono text-sm break-all">{value.value}</div>{value.enc !== 'hex' && <div className="text-xs text-muted-foreground break-all mt-1">HEX {value.hex}</div>}</div>)}</div></details>;
      })}
      {!visible.length && <p className="py-12 text-center text-sm text-muted-foreground">{records.length ? '没有符合筛选条件的资料' : '暂无资料。在请求协议的“应答保存规则”中配置后，成功应答会保存到这里。'}</p>}
    </div>
    <Dialog open={Boolean(deleting)} onOpenChange={open => { if (!open) setDeleting(null); }}><DialogContent><DialogHeader><DialogTitle>删除资料</DialogTitle><DialogDescription>删除“{deleting?.name}”？所属设备不会被删除。</DialogDescription></DialogHeader><DialogFooter><Button variant="outline" onClick={() => setDeleting(null)}>取消</Button><Button variant="destructive" onClick={async () => { if (!deleting) return; try { await deleteBusinessRecord(deleting.id); setDeleting(null); await refresh(); } catch (e) { toast.error(String(e)); } }}>删除</Button></DialogFooter></DialogContent></Dialog>
    {manage && <RecordTypeManager system={system === 'all' ? undefined : Number(system)} onClose={() => { setManage(false); void refresh(); }} />}
  </div>;
}
