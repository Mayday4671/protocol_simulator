import {useEffect,useRef,useState} from 'react';
import {Input} from 'antd';
import type {ProtocolCatalogField} from '../api/tauri';
export type EditableCatalogField=ProtocolCatalogField&{min?:number|null;max?:number|null};
type FieldPath=Pick<ProtocolCatalogField,'path'|'key'>;
function atPath(values:Record<string,unknown>,parts:string[]):unknown{let value:unknown=values;for(const key of parts){if(!value||typeof value!=='object'||!Object.hasOwn(value,key))return undefined;value=(value as Record<string,unknown>)[key]}return value}
function valuePath(values:Record<string,unknown>,field:FieldPath):string[]{const exact=field.path.split('.');if(atPath(values,exact)!==undefined)return exact;const section=[exact[0],field.key];if(atPath(values,section)!==undefined)return section;if(Object.hasOwn(values,field.key))return [field.key];return exact}
export function readFieldValue(values:Record<string,unknown>,field:FieldPath):unknown{return atPath(values,valuePath(values,field))}
export function writeFieldValue(values:Record<string,unknown>,field:FieldPath,value:unknown):Record<string,unknown>{
 const parts=valuePath(values,field);const result=JSON.parse(JSON.stringify(values)) as Record<string,unknown>;let cursor=result;
 if(parts.some(key=>['__proto__','constructor','prototype'].includes(key)))return result;
 for(const key of parts.slice(0,-1)){if(!cursor[key]||typeof cursor[key]!=='object'||Array.isArray(cursor[key]))cursor[key]={};cursor=cursor[key] as Record<string,unknown>}
 cursor[parts.at(-1)!]=value;return result;
}
function leaves(fields:EditableCatalogField[]):EditableCatalogField[]{return fields.flatMap(field=>field.kind==='struct'?leaves(field.children||[]):[field])}
const generated=(field:EditableCatalogField)=>!!(field.fixed||field.generated||field.computed||field.binding==='message.code');
function FieldInput({field,value,label,disabled,onChange}:{field:EditableCatalogField;value:unknown;label:string;disabled:boolean;onChange:(value:unknown)=>void}){
 const numeric=['uint','int','bitset'].includes(field.valueKind||'');const decimal=numeric&&(field.bytes??0)<=6;
 const display=(next:unknown)=>next===undefined?'':typeof next==='object'?JSON.stringify(next):decimal&&typeof next==='string'&&next!==''&&!next.startsWith('${')?'0x'+next.replace(/\s/g,''):String(next);
 const [text,setText]=useState(()=>display(value));const emitted=useRef(value);
 useEffect(()=>{if(!Object.is(emitted.current,value)){emitted.current=value;setText(display(value))}},[value]);
 function edit(next:string){
  setText(next);let parsed:unknown=next;
  if(decimal&&/^-?\d+$/.test(next.trim())&&Number.isSafeInteger(Number(next)))parsed=Number(next);
  else if(/^0x[0-9a-f\s]*$/i.test(next.trim()))parsed=next.trim().slice(2);
  emitted.current=parsed;onChange(parsed);
 }
 return <><Input aria-label={label} disabled={disabled} value={text} onChange={event=>edit(event.target.value)} placeholder={decimal?'十进制 / 0xHEX / ${变量}':numeric||field.valueKind==='bytes'?'HEX / ${变量}':undefined}/>{(numeric||field.valueKind==='bytes')&&<small>{decimal?'十进制 · HEX 加 0x · ${变量}':'HEX · ${变量}'}</small>}</>;
}
export function FlowFields({fields,values,code,disabled=false,onChange}:{fields:EditableCatalogField[];values:Record<string,unknown>;code?:number;disabled?:boolean;onChange:(values:Record<string,unknown>)=>void}){
 const flat=leaves(fields);const editable=flat.filter(field=>field.kind==='field'&&!generated(field));const automatic=flat.filter(field=>field.kind==='field'&&generated(field));
 function renderField(field:EditableCatalogField){
  const readonly=generated(field);const raw=readFieldValue(values,field);const value=raw??field.defaultValue;
  const label=(field.label||field.key)+'（'+field.path+'）';
  return <label className="flow-schema-field" key={field.path} title={field.path}><span>{field.label||field.key}<small>{field.bytes!==undefined?field.bytes+' B':''}{field.required?' · 必填':''}</small></span>{readonly?<Input aria-label={label} disabled value={field.binding==='message.code'?(code===undefined?'由信令选择确定':'0x'+code.toString(16).toUpperCase()):field.computed?'自动计算':field.generated?'运行时生成':String(value??'')}/>:<FieldInput field={field} value={value} label={label} disabled={disabled} onChange={next=>onChange(writeFieldValue(values,field,next))}/>}</label>;
 }
 return <div className="flow-fields-form"><div className="flow-fields-list">{editable.map(renderField)}</div>{flat.filter(field=>field.kind==='array').map(field=><div className="flow-array-hint" key={field.path}>{field.label||field.key} · 数组（在 JSON 中编辑）</div>)}{!!automatic.length&&<details><summary>自动与固定字段（{automatic.length}）</summary><div className="flow-fields-list">{automatic.map(renderField)}</div></details>}</div>;
}
