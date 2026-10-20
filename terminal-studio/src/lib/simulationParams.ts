import type { DictSetView, ProtocolField } from "@/types";
import { fieldIsActive, defaultFieldValue } from "@/lib/deviceProfiles";
import {
  encodeListEntries,
  initialListEntries,
  visibleListEntries,
  type ListEntry,
} from "@/lib/protocolLists";

export function computedFieldLabel(value: string): string | null {
  const token = value.trim();
  if (token === "{tlen}") return "报文总长（自动计算）";
  if (token === "{crc16_ccitt_false}") return "CRC16-CCITT-FALSE（自动计算）";
  const range = token.match(/^\{byte_length:([^:{}]+):([^:{}]+)\}$/);
  return range ? `字节长度：${range[1]} → ${range[2]}（自动计算）` : null;
}

/** Shared by single and periodic sends; never submit inactive or computed values. */
export function buildSimulationParams(
  fields: ProtocolField[],
  values: Record<string, string>,
  overrides: Record<string, string>,
  dicts: DictSetView[],
  listDrafts: Record<string, ListEntry[]>,
): Record<string, string> {
  const params: Record<string, string> = {};
  const active = (field: ProtocolField) => fieldIsActive(field, fields, values, dicts);
  for (const [index, field] of fields.entries()) {
    if (!active(field) || computedFieldLabel(field.value)) continue;
    if (field.role === "device_list" && field.unit_fields?.length) {
      const error = overrides[`__invalid_list_${field.name}`];
      if (error) throw new Error(error);
      const count = fields.slice(0, index).reverse().find(
        (candidate) => candidate.role === "device_count" && active(candidate),
      );
      const rows = listDrafts[field.name] ?? initialListEntries(
        field, values[field.name] ?? defaultFieldValue(field, dicts),
      );
      const visibleRows = visibleListEntries(
        field,
        count,
        count ? (values[count.name] ?? defaultFieldValue(count, dicts)) : "",
        dicts,
        rows,
      );
      params[field.name] = encodeListEntries(field, visibleRows, dicts);
    } else if (overrides[field.name] !== undefined) {
      params[field.name] = overrides[field.name];
    }
  }
  return params;
}
