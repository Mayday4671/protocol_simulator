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
        defaultFieldValue(unit, dicts) ||
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
    unit.unit_fields?.length || unit.when || (unit.value.includes("{") && !unit.dict),
  ))
    throw new Error("此记录含嵌套、条件或变量，当前条目编辑器暂不支持，已阻止误编码发送");
  if (units.reduce((sum, unit) => sum + unit.len, 0) !== field.len)
    throw new Error("记录字段总长度与列表单条长度不一致");
  return rows.map((row) =>
    units.map((unit) => {
      const encoded = encodeValue(unit, row[unit.name] ?? "");
      if (unit.dict && !dicts.find((dict) => dict.key === unit.dict)?.items.some((item) => item.value === encoded))
        throw new Error(`${unit.name} 请选择有效的字典项`);
      return encoded;
    }).join(""),
  ).join("");
}
