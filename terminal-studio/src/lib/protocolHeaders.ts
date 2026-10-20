import type { HeaderDefinition, ProtocolField, ProtocolGroup } from '@/types';

type HeaderSelection = { header_refs_json?: string | null };

/** Display labels must not discard unsent values; wire-schema changes must. */
export function packetSchemaKey(fields: ProtocolField[]): string {
  const wireField = (field: ProtocolField): unknown => {
    const { section: _section, section_name: _sectionName, unit_fields, ...wire } = field;
    return { ...wire, ...(unit_fields ? { unit_fields: unit_fields.map(wireField) } : {}) };
  };
  return JSON.stringify(fields.map(wireField));
}

function jsonArray<T>(json: string, label: string): T[] {
  let value: unknown;
  try { value = JSON.parse(json); } catch { throw new Error(`${label}数据无效`); }
  if (!Array.isArray(value)) throw new Error(`${label}必须是列表`);
  return value as T[];
}

export function groupHeaderDefinitions(group?: ProtocolGroup): HeaderDefinition[] {
  if (!group) return [];
  if (group.header_profiles_json != null) {
    const definitions = jsonArray<HeaderDefinition>(group.header_profiles_json, '公共头');
    const ids = new Set<string>();
    for (const definition of definitions) {
      if (!definition || !definition.id || !definition.name || !Array.isArray(definition.fields) || ids.has(definition.id))
        throw new Error('公共头定义无效或标识重复');
      ids.add(definition.id);
    }
    return definitions;
  }
  const fields = jsonArray<ProtocolField>(group.header_fields_json ?? '[]', '公共头字段');
  return fields.length ? [{ id: 'legacy-default', name: `${group.name} 公共头`, fields }] : [];
}

export function protocolHeaderRefs(protocol?: HeaderSelection, group?: ProtocolGroup): string[] {
  if (protocol?.header_refs_json != null) {
    const refs = jsonArray<string>(protocol.header_refs_json, '公共头引用');
    if (refs.some(ref => typeof ref !== 'string' || !ref.trim()) || new Set(refs).size !== refs.length)
      throw new Error('公共头引用无效或重复');
    return refs;
  }
  return group?.header_enabled && groupHeaderDefinitions(group).some(header => header.id === 'legacy-default')
    ? ['legacy-default'] : [];
}

export function resolveProtocolHeaders(protocol?: HeaderSelection, group?: ProtocolGroup): ProtocolField[] {
  const definitions = groupHeaderDefinitions(group);
  return protocolHeaderRefs(protocol, group).flatMap(id => {
    const definition = definitions.find(header => header.id === id);
    if (!definition) throw new Error(`公共头 ${id} 不存在于当前分组`);
    const reserved = new Set(definition.fields.map(field => field.name));
    const used = new Set<string>();
    return definition.fields.map(field => {
      let localName = field.name;
      if (used.has(localName)) {
        let suffix = 2;
        while (reserved.has(`${field.name}#${suffix}`) || used.has(`${field.name}#${suffix}`)) suffix++;
        localName = `${field.name}#${suffix}`;
      }
      used.add(localName);
      return {
        ...field,
        header_key: `${definition.id}::${localName}`,
        section: field.section ?? 'header',
        section_name: field.section_name ?? definition.name,
      };
    });
  });
}

/** Resolve explicit overrides before assigning unique whole-frame field names. */
export function mergeResolvedHeaderFields(header: ProtocolField[], body: ProtocolField[]) {
  const head = header.map(field => ({ ...field }));
  const bodyOnly: ProtocolField[] = [];
  const overridden = new Set<number>();
  for (const field of body) {
    const ref = field.hdr_ref?.trim();
    if (!ref) { bodyOnly.push(field); continue; }
    let matches = header.flatMap((candidate, index) => candidate.header_key === ref ? [index] : []);
    if (!matches.length) matches = header.flatMap((candidate, index) => candidate.name === ref ? [index] : []);
    if (matches.length > 1 && matches.every(index => !header[index].header_key || header[index].header_key!.startsWith('legacy-default::'))) matches = [matches[0]];
    if (matches.length !== 1) throw new Error(matches.length
      ? `字段“${field.name}”的覆盖目标“${ref}”重名，请选择所属公共头`
      : `字段“${field.name}”引用的公共头字段“${ref}”未选用或不存在`);
    const index = matches[0];
    if (overridden.has(index)) throw new Error(`公共头字段“${ref}”被重复覆盖`);
    overridden.add(index);
    const target = head[index];
    head[index] = {
      ...target,
      value: field.value?.trim() ? field.value : target.value,
      dict: field.dict?.trim() ? field.dict : target.dict,
      role: field.role && field.role !== 'none' ? field.role : target.role,
      profile_key: field.profile_key ?? target.profile_key,
      section: field.section ?? target.section,
      section_name: field.section_name ?? target.section_name,
    };
  }
  const frame = [...head, ...bodyOnly];
  const reservedNames = new Set(frame.map(field => field.name));
  const usedNames = new Set<string>();
  const fields = frame.map(field => {
    let name = field.name;
    if (usedNames.has(name)) {
      let suffix = 2;
      while (reservedNames.has(`${name}#${suffix}`) || usedNames.has(`${name}#${suffix}`)) suffix++;
      name = `${name}#${suffix}`;
    }
    usedNames.add(name);
    return name === field.name ? field : { ...field, name };
  });
  return { fields, head: fields.slice(0, head.length), body: fields.slice(head.length) };
}
