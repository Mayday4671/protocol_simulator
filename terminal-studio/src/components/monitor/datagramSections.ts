import type { FieldDetail, DatagramParse } from "@/types";

export type DatagramSectionKind = "header" | "header_extension" | "payload" | "trailer" | "unparsed";
export interface DatagramDisplayField extends FieldDetail {
  id: string;
  displayName: string;
  sectionKind: DatagramSectionKind;
  sectionName: string;
  actualLength: number;
  synthetic: boolean;
}
export interface DatagramRecordGroup {
  id: string;
  recordName: string | null;
  recordIndex: number | null;
  fields: DatagramDisplayField[];
}
export interface DatagramSection {
  id: string;
  kind: DatagramSectionKind;
  name: string;
  start: number;
  end: number;
  byteLength: number;
  fields: DatagramDisplayField[];
  groups: DatagramRecordGroup[];
}

const SECTION_NAMES: Record<DatagramSectionKind, string> = {
  header: "公共头",
  header_extension: "扩展头",
  payload: "净荷",
  trailer: "尾部",
  unparsed: "未解析",
};

/** Byte offsets and section metadata are authoritative; field names never infer layout. */
export function buildDatagramSections(fields: FieldDetail[], rawHex: string): DatagramSection[] {
  const raw = rawHex.replace(/\s/g, "").toUpperCase();
  const byteCount = Math.floor(raw.length / 2);
  const normalized = fields.map((field, index): DatagramDisplayField => {
    const start = Math.max(0, Math.floor(field.start));
    const actualLength = Math.max(0, Math.min(field.len, byteCount - start));
    const kind = field.section ?? (field.is_header ? "header" : "payload");
    return {
      ...field,
      id: `field-${index}`,
      start,
      displayName: field.field_name?.trim() || field.name,
      sectionKind: kind,
      sectionName: field.section_name?.trim() || SECTION_NAMES[kind],
      actualLength,
      hex: actualLength ? `0x${raw.slice(start * 2, (start + actualLength) * 2)}` : "",
      synthetic: false,
    };
  });

  // Sort a copy solely to find uncovered bytes. The rendered fields keep protocol order.
  const intervals = normalized.filter(field => field.actualLength > 0)
    .map(field => [field.start, field.start + field.actualLength] as const)
    .sort((a, b) => a[0] - b[0]);
  const gaps: DatagramDisplayField[] = [];
  const addGap = (start: number, end: number) => {
    if (end <= start) return;
    gaps.push({
      id: `unparsed-${start}`, name: "未解析字节", displayName: "未解析字节",
      meaning: "协议定义未覆盖的字节", start, len: end - start,
      actualLength: end - start, hex: `0x${raw.slice(start * 2, end * 2)}`,
      dec: null, dict_label: null, is_header: false,
      sectionKind: "unparsed", sectionName: SECTION_NAMES.unparsed, synthetic: true,
    });
  };
  let covered = 0;
  for (const [start, end] of intervals) {
    addGap(covered, start);
    covered = Math.max(covered, end);
  }
  addGap(covered, byteCount);

  const ordered: DatagramDisplayField[] = [];
  let gapIndex = 0;
  for (const field of normalized) {
    while (gapIndex < gaps.length && gaps[gapIndex].start + gaps[gapIndex].actualLength <= field.start) {
      ordered.push(gaps[gapIndex++]);
    }
    ordered.push(field);
  }
  ordered.push(...gaps.slice(gapIndex));

  const sections: DatagramSection[] = [];
  for (const field of ordered) {
    let section = sections[sections.length - 1];
    if (!section || section.kind !== field.sectionKind || section.name !== field.sectionName || section.end !== field.start) {
      section = {
        id: `section-${sections.length}`, kind: field.sectionKind, name: field.sectionName,
        start: field.start, end: field.start, byteLength: 0, fields: [], groups: [],
      };
      sections.push(section);
    }
    section.fields.push(field);
    section.end = field.start + field.actualLength;
    section.byteLength += field.actualLength;
    const recordName = field.record_name?.trim() || null;
    const recordIndex = field.record_index ?? null;
    let group = section.groups[section.groups.length - 1];
    if (!group || group.recordName !== recordName || group.recordIndex !== recordIndex) {
      group = { id: `${section.id}-group-${section.groups.length}`, recordName, recordIndex, fields: [] };
      section.groups.push(group);
    }
    group.fields.push(field);
  }
  return sections;
}
export function formatByteRange(start: number, length: number): string {
  if (!length) return `@${start}`;
  return length === 1 ? String(start) : `${start}–${start + length - 1}`;
}
export function datagramEndpoints(data: Pick<DatagramParse, "direction" | "local_addr" | "remote_addr">) {
  return data.direction === "in"
    ? { source: data.remote_addr, target: data.local_addr }
    : { source: data.local_addr, target: data.remote_addr };
}
