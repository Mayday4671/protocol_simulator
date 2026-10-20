import type { DictSetView, Protocol, ProtocolField, ProtocolGroup } from '@/types';
import { encodeValue } from '@/lib/protocolValues';
import { mergeResolvedHeaderFields, resolveProtocolHeaders } from '@/lib/protocolHeaders';

export function parseFieldSchema(json?: string | null): ProtocolField[] {
  try { const value: unknown = JSON.parse(json ?? '[]'); return Array.isArray(value) ? value : []; }
  catch { return []; }
}

export function parseProperties(json?: string | null): Record<string, string> {
  try {
    const value: unknown = JSON.parse(json ?? '{}');
    if (!value || typeof value !== 'object' || Array.isArray(value)) return {};
    return Object.fromEntries(Object.entries(value).filter((entry): entry is [string, string] => typeof entry[1] === 'string'));
  } catch { return {}; }
}

/** Same explicit header override and whole-frame name deduplication as the sender. */
export function protocolFrame(protocol?: Protocol, group?: ProtocolGroup) {
  try {
    const header = resolveProtocolHeaders(protocol, group);
    const body = protocol?.def_mode === 'fields' ? parseFieldSchema(protocol.fields_json) : [];
    return { ...mergeResolvedHeaderFields(header, body), error: '' };
  } catch (error) {
    return { fields: [] as ProtocolField[], head: [] as ProtocolField[], body: [] as ProtocolField[], error: String(error) };
  }
}

export function dictionariesFor(dicts: DictSetView[], groupId?: number | null) {
  const byKey = new Map<string, DictSetView>();
  for (const d of dicts) {
    if (d.group_ref != null && d.group_ref !== groupId) continue;
    if (!byKey.has(d.key) || d.group_ref != null) byKey.set(d.key, d);
  }
  return [...byKey.values()];
}

export function defaultFieldValue(field: ProtocolField, dicts: DictSetView[]): string {
  if (field.value && !field.value.includes('{')) return field.value;
  if (field.dict) return dicts.find(d => d.key === field.dict)?.items[0]?.value ?? '';
  return '';
}

/** Unmapped packet fields can never pick up values from a device profile. */
export function mappedFieldValues(fields: ProtocolField[], properties: Record<string, string>) {
  return Object.fromEntries(fields.filter(f => f.profile_key && properties[f.profile_key] !== undefined).map(f => [f.name, properties[f.profile_key!] ]));
}

export function fieldIsActive(field: ProtocolField, fields: ProtocolField[], values: Record<string, string>, dicts: DictSetView[]): boolean {
  if (!field.when) return true;
  const controller = fields.find(f => f.name === field.when!.field);
  if (!controller || fields.indexOf(controller) >= fields.indexOf(field) || !fieldIsActive(controller, fields, values, dicts)) return false;
  const current = values[controller.name] ?? defaultFieldValue(controller, dicts);
  const expected = field.when.value;
  if (controller.enc === 'ascii' && !controller.dict) return current === expected;
  if (controller.enc === 'uint' && !controller.dict) {
    try { return BigInt(current) === BigInt(expected); } catch { return false; }
  }
  const compact = (v: string) => v.replace(/\s+/g, '').toUpperCase();
  const dictionary = dicts.find(d => d.key === controller.dict);
  const resolved = (v: string) => dictionary?.items.find(item => item.label === v || compact(item.value) === compact(v))?.value ?? v;
  return compact(resolved(current)) === compact(resolved(expected));
}

export function deviceFormValues(fields: ProtocolField[], properties: Record<string, string>, dicts: DictSetView[]) {
  return Object.fromEntries(fields.filter(f => f.profile_key).map(f => [f.name, properties[f.profile_key!] ?? defaultFieldValue(f, dicts)]));
}

export function deviceFormProperties(fields: ProtocolField[], values: Record<string, string>, dicts: DictSetView[]) {
  const properties: Record<string, string> = {};
  for (const f of fields.filter(f => f.profile_key && fieldIsActive(f, fields, values, dicts))) {
    const value = values[f.name] ?? defaultFieldValue(f, dicts);
    if (values[`__invalid_list_${f.name}`]) throw new Error(values[`__invalid_list_${f.name}`]);
    if (value.includes('{')) throw new Error(`${f.name} 不能保存运行时变量`);
    const stored = normalizePropertyValue(f, value, dicts);
    if (f.role === 'device_list') { properties[f.profile_key!] = stored; continue; }
    const dictionary = dicts.find(d => d.key === f.dict);
    if (f.dict && !dictionary?.items.some(item => item.value === stored)) throw new Error(`${f.name} 请选择有效的字典项`);
    properties[f.profile_key!] = stored;
  }
  return properties;
}

/** 单字段归一为设备属性存储值；编辑过程中输入不完整时保留原文, 保存时整体校验。 */
export function normalizePropertyValue(field: ProtocolField, value: string, dicts: DictSetView[]): string {
  if (field.role === 'device_list') return value.replace(/\s+/g, '').toUpperCase();
  const dictionary = dicts.find(d => d.key === field.dict);
  const normalized = dictionary?.items.find(item => item.label === value || item.value === value.replace(/\s+/g, '').toUpperCase())?.value ?? value;
  try {
    if (field.enc === 'hex' || field.dict) return encodeValue(field, normalized);
    if (field.enc === 'uint') return BigInt(normalized || '0').toString();
  } catch { return normalized; }
  return normalized;
}

/** 设备表单的一个协议分区: 系统内每套声明了设备属性的非应答协议各成一区。 */
export interface DeviceSection {
  protocol: Protocol;
  /** 整帧字段(公共头合并后), 分支条件在其中求值 */
  frame: ProtocolField[];
  /** 绑定了设备属性 Key 的字段 */
  fields: ProtocolField[];
  keys: string[];
  dicts: DictSetView[];
  /** 属性键集合被其他分区完全覆盖时默认折叠(共享键在覆盖它的分区里编辑) */
  collapseByDefault: boolean;
}

export function deviceSections(protocols: Protocol[], systemRef: number, groups: ProtocolGroup[], dicts: DictSetView[]): DeviceSection[] {
  const sections: Omit<DeviceSection, 'collapseByDefault'>[] = [];
  for (const protocol of protocols) {
    if (protocol.system_ref !== systemRef || protocol.msg_type === 'Response' || protocol.def_mode !== 'fields') continue;
    const frame = protocolFrame(protocol, groups.find(g => g.id === protocol.group_ref)).fields;
    const fields = frame.filter(f => f.profile_key?.trim());
    if (!fields.length) continue;
    sections.push({
      protocol,
      frame,
      fields,
      keys: [...new Set(fields.map(f => f.profile_key!.trim()))],
      dicts: dictionariesFor(dicts, protocol.group_ref),
    });
  }
  return sections.map((s, i, all) => ({
    ...s,
    collapseByDefault: all.some((o, j) => {
      if (i === j) return false;
      const covered = s.keys.every(k => o.keys.includes(k));
      const superset = o.keys.some(k => !s.keys.includes(k));
      return covered && (superset || j < i);
    }),
  }));
}

export function decodeRecordHex(units: ProtocolField[], hex: string): Record<string, string>[] {
  const compact = hex.replace(/\s+/g, '').toUpperCase();
  const recordLength = units.reduce((sum, f) => sum + f.len * 2, 0);
  if (!recordLength || !/^[0-9A-F]*$/.test(compact) || compact.length % recordLength) throw new Error('已存列表长度与记录结构不一致');
  const rows: Record<string, string>[] = [];
  for (let start = 0; start < compact.length; start += recordLength) {
    let offset = start;
    const row: Record<string, string> = {};
    for (const f of units) {
      const raw = compact.slice(offset, offset + f.len * 2);
      offset += f.len * 2;
      row[f.name] = f.enc === 'uint' && !f.dict ? BigInt(`0x${raw}`).toString() : f.enc === 'ascii' && !f.dict
        ? new TextDecoder().decode(Uint8Array.from(raw.match(/../g) ?? [], b => parseInt(b, 16))).replace(/\0+$/, '') : raw;
    }
    rows.push(row);
  }
  return rows;
}
