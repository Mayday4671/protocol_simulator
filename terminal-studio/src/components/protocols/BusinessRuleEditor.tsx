import { useEffect, useMemo, useState } from 'react';
import { Plus, Trash2 } from 'lucide-react';
import { toast } from 'sonner';
import type { Protocol, ProtocolField, ProtocolGroup } from '@/types';
import { protocolFrame } from '@/lib/deviceProfiles';
import { listSaveRules, saveBusinessRule, listRecordTypes, type RecordType, type SaveRule } from '@/lib/businessRules';
import { RecordTypeManager } from '@/components/RecordTypeManager';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Dialog, DialogContent, DialogHeader, DialogTitle, DialogDescription, DialogFooter } from '@/components/ui/dialog';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';

type Option = { value: string; label: string };
function Choice({ label, value, options, onChange, empty = '请选择' }: { label: string; value: string; options: Option[]; onChange: (value: string) => void; empty?: string }) {
  return <label className="min-w-0 space-y-1.5 text-sm"><span>{label}</span><Select value={value || '__none'} onValueChange={v => onChange(v === '__none' ? '' : v)}><SelectTrigger aria-label={label}><SelectValue /></SelectTrigger><SelectContent><SelectItem value="__none">{empty}</SelectItem>{options.map(o => <SelectItem key={o.value} value={o.value}>{o.label}</SelectItem>)}</SelectContent></Select></label>;
}
function fieldsOptions(fields: ProtocolField[], array: string): Option[] {
  return [
    ...fields.filter(f => !f.unit_fields?.length && !['device_list', 'resource_list'].includes(f.role ?? '') && !f.value?.includes('{vlen')).map(f => ({ value: `frame.${f.name}`, label: `${f.name} · ${f.len} B` })),
    ...(fields.find(f => f.name === array)?.unit_fields ?? []).map(f => ({ value: `item.${f.name}`, label: `条目 / ${f.name} · ${f.len} B` })),
  ];
}
export function BusinessRuleEditor({ protocol, protocols, groups, onClose }: { protocol: Protocol; protocols: Protocol[]; groups: ProtocolGroup[]; onClose: () => void }) {
  const [rule, setRule] = useState<SaveRule>({ request: protocol.protocol_id, response: '', kind: 'device', operation: 'upsert', key_scope: 'channel', request_array: '', response_array: '', pairing: 'index', request_match: '', response_match: '', request_sequence: '', response_sequence: '', serial: false, timeout_ms: 5000, success_field: '', success_hex: '', identity: '', name_field: '', device_field: '', mappings: [] });
  const [enabled, setEnabled] = useState(false);
  const [loading, setLoading] = useState(true);
  const [ready, setReady] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState('');
  const [types, setTypes] = useState<RecordType[]>([]);
  const [manageTypes, setManageTypes] = useState(false);
  const selectedType = types.find(t => t.kind === rule.kind);
  useEffect(() => { let live = true; Promise.all([listSaveRules(), listRecordTypes(protocol.system_ref!)]).then(([rules, definitions]) => { if (!live) return; setTypes(definitions); setReady(true); const saved = rules.find(r => r.request === protocol.protocol_id); if (saved) { setRule({ ...saved, operation: saved.operation ?? 'upsert', key_scope: saved.key_scope ?? 'channel' }); setEnabled(true); } }).catch(e => { if (live) setError(String(e)); }).finally(() => { if (live) setLoading(false); }); return () => { live = false; }; }, [protocol.protocol_id, protocol.system_ref]);
  const set = <K extends keyof SaveRule>(key: K, value: SaveRule[K]) => setRule(r => ({ ...r, [key]: value }));
  const responses = protocols.filter(p => p.system_ref === protocol.system_ref && p.msg_type === 'Response' && p.protocol_id !== protocol.protocol_id);
  const requestFrame = useMemo(() => protocolFrame(protocol, groups.find(g => g.id === protocol.group_ref)), [protocol, groups]);
  const response = responses.find(p => p.protocol_id === rule.response);
  const responseFrame = useMemo(() => protocolFrame(response, groups.find(g => g.id === response?.group_ref)), [response, groups]);
  const req = fieldsOptions(requestFrame.fields, rule.request_array), resp = fieldsOptions(responseFrame.fields, rule.response_array);
  const arrays = (fields: ProtocolField[]) => fields.filter(f => f.unit_fields?.length).map(f => ({ value: f.name, label: f.name }));
  const persist = async () => { setSaving(true); setError(''); try { if (enabled && !selectedType) throw new Error('请先配置资料类型'); await saveBusinessRule(protocol.protocol_id, enabled ? { ...rule, key_scope: selectedType!.key_scope, ...(rule.operation === 'delete' ? { mappings: [], name_field: '' } : {}) } : null); toast.success(enabled ? '应答保存规则已保存' : '已关闭应答保存'); onClose(); } catch (e) { setError(String(e)); } finally { setSaving(false); } };
  return <><Dialog open onOpenChange={open => { if (!open && !saving && !manageTypes) onClose(); }}><DialogContent className="max-w-5xl"><DialogHeader><DialogTitle>应答保存规则 · {protocol.name}</DialogTitle><DialogDescription>仅在本次请求匹配到指定成功应答后执行资料操作。</DialogDescription></DialogHeader>
    <div className="min-h-0 overflow-y-auto space-y-5 pr-2">
      <label className="flex items-center gap-2 text-sm"><input type="checkbox" checked={enabled} disabled={loading} onChange={e => setEnabled(e.target.checked)} />成功应答后处理资料</label>
      {enabled && <>
        <div className="grid grid-cols-2 lg:grid-cols-4 gap-3">
          <Choice label="应答协议" value={rule.response} options={responses.map(p => ({ value: p.protocol_id, label: p.name }))} onChange={v => setRule(r => ({ ...r, response: v, response_array: '', success_field: '', response_sequence: '', response_match: '', mappings: r.mappings.filter(m => m.source === 'request') }))} />
          <div className="space-y-1.5"><Choice label="保存到" value={rule.kind} options={types.map(t => ({ value: t.kind, label: t.name }))} onChange={v => setRule(r => ({ ...r, kind: v, key_scope: types.find(t => t.kind === v)?.key_scope ?? 'channel', mappings: [], device_field: v === 'device' ? '' : r.device_field }))} /><button type="button" className="text-xs underline text-muted-foreground" onClick={() => setManageTypes(true)}>管理资料类型</button></div>
          <Choice label="成功后操作" value={rule.operation} options={[{ value: 'upsert', label: '新增或更新资料' }, { value: 'delete', label: '删除对应资料' }]} onChange={v => set('operation', v as SaveRule['operation'])} />
          <label className="space-y-1.5 text-sm">等待应答（毫秒）<Input aria-label="等待应答" type="number" min={100} max={300000} value={rule.timeout_ms} onChange={e => set('timeout_ms', Number(e.target.value))} /></label>
        </div>
        <section className="rounded-lg border p-4 space-y-3"><h3 className="font-medium text-sm">请求与应答对应</h3>
          <Choice label="对应方式" value={rule.serial ? 'serial' : 'sequence'} options={[{ value: 'sequence', label: '流水号相等' }, { value: 'serial', label: '无流水号 · 单请求模式' }]} onChange={v => set('serial', v === 'serial')} />
          {rule.serial ? <p className="text-sm text-amber-600 dark:text-amber-400">同一通道、对端及应答协议只能等待一笔请求。协议没有流水号时，无法区分上一笔迟到的重复应答；超时后需重启监听。</p> : <div className="grid grid-cols-2 gap-3"><Choice label="请求流水号" value={rule.request_sequence} options={req.filter(o => o.value.startsWith('frame.'))} onChange={v => set('request_sequence', v)} /><Choice label="应答流水号" value={rule.response_sequence} options={resp.filter(o => o.value.startsWith('frame.'))} onChange={v => set('response_sequence', v)} /></div>}
          <div className="grid grid-cols-2 gap-3"><Choice label="请求资料" empty="整条报文 · 一条资料" value={rule.request_array} options={arrays(requestFrame.fields)} onChange={v => set('request_array', v)} /><Choice label="应答结果" empty="整体结果" value={rule.response_array} options={arrays(responseFrame.fields)} onChange={v => set('response_array', v)} /></div>
          {rule.response_array && <><Choice label="条目对应" value={rule.pairing} options={[{ value: 'index', label: '按顺序（数量须一致）' }, { value: 'key', label: '按条目标识' }]} onChange={v => set('pairing', v as 'index' | 'key')} />{rule.pairing === 'key' && <div className="grid grid-cols-2 gap-3"><Choice label="请求条目标识" value={rule.request_match} options={req} onChange={v => set('request_match', v)} /><Choice label="应答条目标识" value={rule.response_match} options={resp} onChange={v => set('response_match', v)} /></div>}</>}
          <div className="grid grid-cols-2 gap-3"><Choice label="成功结果字段" value={rule.success_field} options={resp} onChange={v => set('success_field', v)} /><label className="space-y-1.5 text-sm">成功值（HEX）<Input aria-label="成功值" value={rule.success_hex} onChange={e => set('success_hex', e.target.value)} placeholder="按协议填写" /></label></div>
        </section>
        <section className="rounded-lg border p-4 space-y-3"><h3 className="font-medium text-sm">资料标识</h3><div className="grid grid-cols-2 lg:grid-cols-3 gap-3"><Choice label="唯一标识（请求字段）" value={rule.identity} options={req} onChange={v => set('identity', v)} />{rule.operation !== 'delete' && <Choice label="显示名称" empty="使用唯一标识" value={rule.name_field} options={req} onChange={v => set('name_field', v)} />}{rule.kind !== 'device' && <Choice label={selectedType?.device_required ? '所属设备标识（必选）' : '所属设备标识'} empty={selectedType?.device_required ? '请选择' : '不关联设备'} value={rule.device_field} options={req} onChange={v => set('device_field', v)} />}</div><p className="text-xs text-muted-foreground">{selectedType?.key_scope === 'device' ? '同一设备内按标识定位资料，不同设备可使用相同编号。' : '同一系统、通道和类型内按标识定位资料。'} 所属设备须已存在。</p></section>
        {rule.operation !== 'delete' ? <section className="rounded-lg border p-4 space-y-3"><div className="flex justify-between items-center"><h3 className="font-medium text-sm">保存属性</h3><Button size="sm" variant="outline" disabled={!selectedType?.properties.length} onClick={() => set('mappings', [...rule.mappings, { property: '', source: 'request', field: '' }])}><Plus />添加属性</Button></div>
          {!selectedType?.properties.length && <Button variant="outline" size="sm" onClick={() => setManageTypes(true)}>为此类型定义属性</Button>}
          {rule.mappings.map((m, i) => <div key={i} className="grid grid-cols-[1fr_120px_1fr_36px] gap-2 items-end"><Choice label={`属性名称 ${i + 1}`} value={m.property} options={[...new Set([...(selectedType?.properties ?? []), ...(m.property ? [m.property] : [])])].map(p => ({ value: p, label: p }))} onChange={v => set('mappings', rule.mappings.map((x,j) => j === i ? { ...x, property: v } : x))} /><Choice label={`来源 ${i + 1}`} value={m.source} options={[{ value: 'request', label: '请求' }, { value: 'response', label: '应答' }]} onChange={v => set('mappings', rule.mappings.map((x,j) => j === i ? { ...x, source: v as 'request' | 'response', field: '' } : x))} /><Choice label={`字段 ${i + 1}`} value={m.field} options={m.source === 'request' ? req : resp} onChange={v => set('mappings', rule.mappings.map((x,j) => j === i ? { ...x, field: v } : x))} /><Button variant="ghost" size="icon" aria-label={`删除属性 ${i + 1}`} onClick={() => set('mappings', rule.mappings.filter((_,j) => j !== i))}><Trash2 /></Button></div>)}
        </section> : <p className="text-sm text-muted-foreground">仅删除成功条目对应的资料；不会删除所属设备。删除设备档案时须先处理其关联资料。</p>}
      </>}
      {(error || requestFrame.error || responseFrame.error) && <p role="alert" className="text-sm text-destructive">{error || requestFrame.error || responseFrame.error}</p>}
    </div><DialogFooter><Button variant="outline" onClick={onClose} disabled={saving}>取消</Button><Button onClick={persist} disabled={loading || saving || !ready}>{saving ? '保存中…' : '保存规则'}</Button></DialogFooter>
  </DialogContent></Dialog>{manageTypes && <RecordTypeManager system={protocol.system_ref!} onClose={() => { setManageTypes(false); void listRecordTypes(protocol.system_ref!).then(setTypes).catch(e => setError(String(e))); }} />}</>;
}
