import type {FieldSpan} from '../api/tauri';
export function formatMessageCode(code:number|null|undefined):string{return typeof code==='number'?code.toString(16).padStart(2,'0').toUpperCase():'—'}
export function flattenFields(fields:FieldSpan[]):FieldSpan[]{return fields.flatMap(field=>[field,...flattenFields(field.children||[])])}
export function findFieldAtByte(fields:FieldSpan[],index:number):FieldSpan|undefined{
 const candidates=fields.filter(field=>index>=field.startOffset0&&index<field.startOffset0+field.byteLength);
 for(const field of candidates){const child=findFieldAtByte(field.children||[],index);if(child)return child}
 return candidates.sort((a,b)=>a.byteLength-b.byteLength)[0];
}
export function byteRange(field:FieldSpan,oneBased:boolean):string{const start=field.startOffset0+(oneBased?1:0);return field.byteLength>1?`${start}–${start+field.byteLength-1}`:String(start)}
