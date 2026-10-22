import type { DictSetView, ProtocolField } from "@/types";
import { decodeRecordHex, defaultFieldValue } from "@/lib/deviceProfiles";
import { encodeValue, entryCount } from "@/lib/protocolValues";

export type ListEntry = Record<string, string>;

export function initialListEntries(
  field: ProtocolField,
  value?: string,
): ListEntry[] {
  return value == null || value.includes("{")
    ? []
    : decodeRecordHex(field.unit_fields ?? [], value);
}

/** Only the visible prefix is encoded; the complete draft remains intact. */
export function visibleListEntries(
  field: ProtocolField,
  count: ProtocolField | undefined,
  countValue: string,
  dicts: DictSetView[],
  savedRows: ListEntry[],
): ListEntry[] {
  return Array.from(
    { length: entryCount(count, countValue) },
    (_, i) => savedRows[i] ?? Object.fromEntries(
      (field.unit_fields ?? []).map((unit) => [
        unit.name,
        (!unit.dict && unit.value.includes("{") ? unit.value : defaultFieldValue(unit, dicts)) ||
          (unit.enc === "hex" ? "00".repeat(unit.len) : unit.enc === "uint" ? "0" : ""),
      ]),
    ),
  );
}

export function encodeListEntries(
  field: ProtocolField,
  rows: ListEntry[],
  dicts: DictSetView[],
): string {
  const units = field.unit_fields ?? [];
  if (units.some((unit) =>
    unit.unit_fields?.length || unit.when,
  ))
    throw new Error(`${field.name} 的条目含嵌套或条件字段，当前仅支持定长条目；请调整条目结构`);
  if (units.reduce((sum, unit) => sum + unit.len, 0) !== field.len)
    throw new Error("记录字段总长度与列表单条长度不一致");
  return rows.map((row) =>
    units.map((unit) => {
      const value = row[unit.name] ?? "";
      if (value.includes("{") && !unit.dict) {
        const variable = value.trim().match(/^\{(seq|session|tlen|timestamp|timestamp_ms|node_id|random)(?::(\d+))?\}$/);
        if (!variable || unit.enc === "ascii")
          throw new Error(`${unit.name} 的取值表达式暂不支持条目编码，请改用固定值或自动流水号、会话、长度、时间戳`);
        if (variable[2] && Number(variable[2]) !== unit.len)
          throw new Error(`${unit.name} 的变量长度必须与字段的 ${unit.len} 字节一致`);
        // The outer array is one variable-length Hex field. Explicit widths keep
        // Rust from applying the entire record's width to each inner variable.
        return `{${variable[1]}:${unit.len}}`;
      }
      const encoded = encodeValue(unit, value);
      if (unit.dict && !dicts.find((dict) => dict.key === unit.dict)?.items.some((item) => item.value === encoded))
        throw new Error(`${unit.name} 请选择有效的字典项`);
      return encoded;
    }).join(""),
  ).join("");
}
