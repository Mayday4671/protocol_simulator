import type { ProtocolField } from "@/types";

/** Device properties retain their declared encoding; never truncate an address to fit. */
export function devicePropertyValue(raw: string, field: Pick<ProtocolField, "enc" | "dict"> | undefined, enc: string, bytes: number) {
  if (!field) throw new Error("原协议属性定义不存在，无法确定编码");
  if (field.enc === "ascii" && !field.dict) {
    if (enc !== "ascii") throw new Error("文本属性不能填入数值字段");
    if (new TextEncoder().encode(raw).length > bytes) throw new Error("属性长度超过字段长度");
    return raw;
  }
  if (enc === "ascii") throw new Error("数值属性不能填入文本字段");
  const compact = raw.replace(/\s/g, "");
  const hex = field.enc === "hex" || Boolean(field.dict);
  if (!(hex ? /^(?:0x)?[0-9a-f]+$/i : /^\d+$/).test(compact)) throw new Error("属性值不是有效数值");
  const number = BigInt(hex ? `0x${compact.replace(/^0x/i, "")}` : compact);
  if (!Number.isInteger(bytes) || bytes < 1 || bytes > 65535 || number >= (1n << BigInt(bytes * 8)))
    throw new Error("属性值超过字段长度");
  return enc === "uint" ? number.toString() : number.toString(16).toUpperCase().padStart(bytes * 2, "0");
}


export type DeviceFieldSource = {
  id: number;
  name: string;
  address: string;
  properties: Record<string, string>;
  fields: ProtocolField[];
  systemName?: string;
};

export function isDeviceField(field: ProtocolField) {
  return ["src_id", "dst_id", "device_sn"].includes(field.role ?? "") || Boolean(field.profile_key);
}

/** The field's configured role/key defines its data source, never its display name. */
export function deviceFieldValue(field: ProtocolField, device: DeviceFieldSource): string {
  if (field.role === "src_id" || field.role === "dst_id" || (field.role === "device_sn" && !field.profile_key))
    return devicePropertyValue(device.address, { enc: "hex" }, field.dict ? "hex" : field.enc, field.len);
  if (!field.profile_key) throw new Error("未配置设备字段用途或属性 Key");
  const raw = device.properties[field.profile_key];
  if (raw === undefined) throw new Error(`设备未填写属性 ${field.profile_key}`);
  const source = device.fields.find(f => f.profile_key === field.profile_key);
  return devicePropertyValue(raw, source, field.dict ? "hex" : field.enc, field.len);
}

/** One atomic fill; a destination only changes itself, and no fill can override a session/calculation. */
export function deviceSelectionValues(selected: ProtocolField, device: DeviceFieldSource, fields: ProtocolField[]) {
  const values: Record<string, string> = { [selected.name]: deviceFieldValue(selected, device) };
  if (selected.role === "dst_id") return values;
  for (const field of fields) {
    if (field === selected || !field.profile_key || field.role === "device_list" ||
        ["src_id", "dst_id", "session", "correlation", "type_code"].includes(field.role ?? "") ||
        /\{(?:tlen|rlen|crc)/i.test(field.value)) continue;
    try {
      // Missing properties must not retain the last selected device's values.
      values[field.name] = device.properties[field.profile_key] === undefined ? "" : deviceFieldValue(field, device);
    } catch (error) { throw new Error(`${field.name}：${(error as Error).message}`); }
  }
  return values;
}
