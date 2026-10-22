import { invoke } from '@tauri-apps/api/core';
import type { FieldEnc } from '@/types';

export interface SaveRule {
  request: string; response: string; kind: string;
  operation: 'upsert' | 'delete'; key_scope: 'channel' | 'device';
  request_array: string; response_array: string;
  pairing: 'index' | 'key'; request_match: string; response_match: string;
  request_sequence: string; response_sequence: string; serial: boolean; timeout_ms: number;
  success_field: string; success_hex: string; identity: string; name_field: string; device_field: string;
  mappings: { property: string; source: 'request' | 'response'; field: string }[];
}
export interface BusinessCell { value: string; hex: string; enc: FieldEnc; bytes: number }
export interface BusinessRecord {
  id: number; node_ref: number; system_ref: number; kind: string; record_key: string; name: string;
  device_ref: number | null; device_name: string | null; properties_json: string; request_ref: string; updated_at: string;
}
export const listSaveRules = () => invoke<SaveRule[]>('list_save_rules');
export const saveBusinessRule = (request: string, rule: SaveRule | null) => invoke<void>('save_business_rule', { request, rule });
export const listBusinessRecords = () => invoke<BusinessRecord[]>('list_business_records');
export const deleteBusinessRecord = (id: number) => invoke<void>('delete_business_record', { id });
export const kindName = (kind: string) => ({ device: '设备', phone: '电话' }[kind] ?? kind);

export interface RecordType {
  kind: string; name: string; device_required: boolean;
  key_scope: 'channel' | 'device'; properties: string[];
}
export const listRecordTypes = (system: number) => invoke<RecordType[]>('list_record_types', { system });
export const saveRecordType = (system: number, definition: RecordType) => invoke<void>('save_record_type', { system, definition });
export const deleteRecordType = (system: number, kind: string) => invoke<void>('delete_record_type', { system, kind });
