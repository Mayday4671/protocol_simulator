import { listBusinessRecords, listRecordTypes, type BusinessCell } from "@/lib/businessRules";
import { useState } from "react";
import { Monitor } from "lucide-react";
import { listGroups, listProtocols, listSystems, listTerminals } from "@/lib/api";
import { parseProperties, protocolFrame } from "@/lib/deviceProfiles";

import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { Input } from "@/components/ui/input";

type Choice = { key: string; label: string; value?: string; error?: string };
type Device = { id: number; name: string; system: string; address: string; choices: Choice[] };

import { devicePropertyValue } from "@/lib/deviceSelection";
export { devicePropertyValue } from "@/lib/deviceSelection";

export function DeviceValuePicker({ enc, bytes, onChange, label, replacesDictionary }: {
  enc: string; bytes: number; onChange: (value: string) => void; label: string; replacesDictionary?: boolean;
}) {
  const [open, setOpen] = useState(false);
  const [devices, setDevices] = useState<Device[]>([]);
  const [selected, setSelected] = useState<number>();
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  async function refresh() {
    setLoading(true); setError(""); setSelected(undefined); setQuery("");
    try {
      const [profiles, protocols, groups, systems, records] = await Promise.all([
        listTerminals(), listProtocols(), listGroups(), listSystems(), listBusinessRecords(),
      ]);
      const definitions = (await Promise.all([...new Set(records.filter(r => r.device_ref !== null).map(r => r.system_ref))].map(async system => (await listRecordTypes(system)).map(t => ({ ...t, system }))))).flat();
      setDevices(profiles.map(profile => {
        const schemas = protocols.filter(p => p.system_ref === profile.system_ref)
          .sort((a, b) => Number(b.protocol_id === profile.protocol_ref) - Number(a.protocol_id === profile.protocol_ref))
          .flatMap(p => protocolFrame(p, groups.find(g => g.id === p.group_ref)).fields);
        const record = records.find(r => r.kind === 'device' && r.node_ref === profile.node_ref && r.system_ref === profile.system_ref && r.record_key === profile.device_key);
        const properties: Record<string, BusinessCell> = record ? JSON.parse(record.properties_json) : {};
        return {
          id: profile.id, name: profile.name, address: profile.device_key,
          system: systems.find(s => s.id === profile.system_ref)?.name ?? `系统 #${profile.system_ref}`,
          choices: [...Object.entries(parseProperties(profile.fields_json)).map(([key, raw]) => {
            const schema = schemas.find(f => f.profile_key === key);
            const stored = properties[key];
            const base = { key, label: `${schema?.name ?? key} · ${raw}` };
            try { return { ...base, value: devicePropertyValue(raw, stored ? { enc: stored.enc } : schema, enc, bytes) }; }
            catch (e) { return { ...base, error: (e as Error).message }; }
          }), ...records.filter(r => r.kind !== 'device' && r.device_ref === profile.id).flatMap(r => {
            const values: Record<string, BusinessCell> = JSON.parse(r.properties_json);
            const typeName = definitions.find(t => t.system === r.system_ref && t.kind === r.kind)?.name ?? r.kind;
            return Object.entries(values).map(([key, stored]) => {
              const base = { key: `record:${r.id}:${key}`, label: `${typeName} / ${r.name} / ${key} · ${stored.value}` };
              try { return { ...base, value: devicePropertyValue(stored.value, { enc: stored.enc }, enc, bytes) }; }
              catch (e) { return { ...base, error: (e as Error).message }; }
            });
          })],
        };
      }));
    } catch (e) { setError(`设备读取失败：${String(e)}`); }
    finally { setLoading(false); }
  }
  const device = devices.find(d => d.id === selected);
  return <Popover open={open} onOpenChange={next => { setOpen(next); if (next) void refresh(); }}>
    <PopoverTrigger asChild>
      <button type="button" aria-label={`从设备取值：${label}`} title="从设备取值"
        className="flex h-8 w-7 shrink-0 items-center justify-center rounded-md border border-input hover:bg-accent">
        <Monitor className="h-3.5 w-3.5" />
      </button>
    </PopoverTrigger>
    <PopoverContent align="end" className="w-80 p-2">
      {replacesDictionary && <p className="mb-2 text-xs text-muted-foreground">选择设备属性后，此字段改用固定值并解除字典绑定。</p>}
      {device ? <>
        <button type="button" className="mb-2 text-xs text-muted-foreground hover:text-foreground" onClick={() => setSelected(undefined)}>← 全部设备</button>
        <div className="mb-2 text-sm font-medium">{device.name}</div>
        <div className="max-h-64 space-y-1 overflow-auto">
          {device.choices.map(choice => <button type="button" key={choice.key} disabled={choice.value === undefined}
            className="block w-full rounded px-2 py-2 text-left text-xs hover:bg-accent disabled:opacity-50"
            onClick={() => { onChange(choice.value!); setOpen(false); }}>
            <span className="block break-all">{choice.label}</span>
            <span className="text-muted-foreground">{choice.error ?? choice.key}</span>
          </button>)}
          {!device.choices.length && <p className="p-2 text-xs text-muted-foreground">该设备尚未配置属性</p>}
        </div>
      </> : <>
        <Input aria-label="搜索设备" placeholder="搜索设备、地址、系统" className="mb-2 h-8" value={query} onChange={e => setQuery(e.target.value)} />
        <div className="max-h-64 space-y-1 overflow-auto">
          {loading ? <p className="p-2 text-xs">读取设备…</p> : error ? <p role="alert" className="p-2 text-xs text-destructive">{error}</p> :
            devices.filter(d => `${d.name} ${d.system} ${d.address}`.toLowerCase().includes(query.toLowerCase())).map(d =>
              <button type="button" key={d.id} className="block w-full rounded px-2 py-2 text-left hover:bg-accent" onClick={() => setSelected(d.id)}>
                <span className="block text-sm">{d.name}</span>
                <span className="block break-all text-xs text-muted-foreground">{d.system} · {d.address}</span>
              </button>)}
          {!loading && !error && !devices.length && <p className="p-2 text-xs text-muted-foreground">请先在设备管理中添加设备</p>}
          {!loading && !error && devices.length > 0 && !devices.some(d => `${d.name} ${d.system} ${d.address}`.toLowerCase().includes(query.toLowerCase())) && <p className="p-2 text-xs text-muted-foreground">没有匹配的设备</p>}
        </div>
      </>}
    </PopoverContent>
  </Popover>;
}
