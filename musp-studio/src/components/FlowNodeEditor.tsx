import {useEffect,useRef,useState} from 'react';
import {Alert,Button,Checkbox,Input,InputNumber,Select,Tag} from 'antd';
import {studio,type FlowNode,type FlowPreview,type ProtocolCatalog} from '../api/tauri';
import {parseObjectJson,type FlowNodeDraft} from '../utils/flowEditor';
import PacketInspector from './PacketInspector';
import {FlowFields} from './FlowFields';

interface Props{
 projectId:string;protocolId?:string;node:FlowNode;catalog?:ProtocolCatalog;catalogError?:string;disabled?:boolean;
 variables:Record<string,unknown>;variablesError?:string;response?:Record<string,unknown>;
 draft:FlowNodeDraft;onDraft:(draft:FlowNodeDraft)=>void;onChange:(data:Record<string,unknown>)=>void;
}
const hexCode=(code:number)=>'0x'+code.toString(16).padStart(2,'0').toUpperCase();
const codeNumber=(value:unknown)=>typeof value==='number'?value:typeof value==='string'?Number(value):undefined;
export function FlowNodeEditor({projectId,protocolId,node,catalog,catalogError,variables,variablesError,response,draft,onDraft,onChange,disabled=false}:Props){
 const [preview,setPreview]=useState<FlowPreview>();const [previewError,setPreviewError]=useState('');const [previewing,setPreviewing]=useState(false);const [useResponse,setUseResponse]=useState(false);
 const request=useRef(0);const send=['SEND','SEND_AND_WAIT'].includes(node.type);const wait=['WAIT_PACKET','SEND_AND_WAIT'].includes(node.type);
 const code=codeNumber(node.data.code??node.data.requestMsgCode);
 const expected=((node.data.expectedCodes??node.data.expectCodes) as unknown[]||[]).map(codeNumber).filter((value):value is number=>value!==undefined&&Number.isFinite(value));
 const messages=catalog?.messages||[];const currentMessage=messages.find(message=>message.code===code);
 const options=messages.map(message=>({value:message.code,label:hexCode(message.code)+' · '+message.name}));
 if(code!==undefined&&!options.some(option=>option.value===code))options.unshift({value:code,label:hexCode(code)+' · 协议未定义'});
 const valueText=draft.values??JSON.stringify(node.data.values||{},null,2);let valuesError='';
 try{parseObjectJson(valueText,'字段值')}catch(error){valuesError=String(error).replace(/^Error: /,'')}
 const advancedPending=draft.advanced!==undefined;const editDisabled=disabled||advancedPending;const previewBlock=variablesError||valuesError||(advancedPending?'先应用或撤销高级 JSON 修改':'')||(!protocolId?'先绑定发布协议':'');
 const context=JSON.stringify({projectId,protocolId,node,variables,response,useResponse,valueText,advanced:draft.advanced});
 useEffect(()=>{request.current++;setPreview(undefined);setPreviewError('');setPreviewing(false);return()=>{request.current++}},[context]);
 function update(patch:Record<string,unknown>){if(disabled)return;onChange({...node.data,...patch})}
 function selectMessage(nextCode:number){
  const message=messages.find(item=>item.code===nextCode);if(!message)return;
  if(code===nextCode)return;
  update({code:nextCode,requestMsgCode:undefined,values:node.data.values??message.defaultValues,...(wait?{expectedCodes:message.responseCodes,expectCodes:undefined}:{})});
 }
 function editValues(text:string){onDraft({...draft,values:text});try{update({values:parseObjectJson(text,'字段值')})}catch{/* Keep invalid edits visible until corrected. */}}
 async function inspect(){
  if(previewBlock)return;const ticket=++request.current;setPreviewing(true);setPreviewError('');
  try{const result=await studio<FlowPreview>('flow_preview',{expectedProjectId:projectId,protocolId,node,variables,...(useResponse&&response?{response}:{})});if(ticket===request.current)setPreview(result)}
  catch(error){if(ticket===request.current)setPreviewError(String(error))}
  finally{if(ticket===request.current)setPreviewing(false)}
 }
 return <div className="flow-node-editor">
  <div className="flow-field-row"><label htmlFor="flow-node-name">节点名称</label><Input id="flow-node-name" aria-label="节点名称" value={String(node.data.label||'')} disabled={editDisabled} onChange={event=>update({label:event.target.value})}/><Tag>{node.type}</Tag></div>
  {send&&<label className="flow-field">发送信令<Select aria-label="发送信令" placeholder="选择发送信令" value={code} options={options} disabled={editDisabled||!!valuesError||!protocolId} onChange={selectMessage} showSearch optionFilterProp="label"/></label>}
  {wait&&<><label className="flow-field">等待信令<Select aria-label="等待信令" placeholder="选择等待信令" mode="multiple" value={expected} options={options} disabled={editDisabled||!protocolId} onChange={codes=>update({expectedCodes:codes,expectCodes:undefined})} showSearch optionFilterProp="label"/></label><div className="flow-field-row"><label>超时（毫秒）<InputNumber aria-label="等待超时毫秒" min={1} max={600000} value={Number(node.data.timeoutMs??3000)} disabled={editDisabled} onChange={value=>update({timeoutMs:value})}/></label>{node.type==='SEND_AND_WAIT'&&<label>额外重试<InputNumber aria-label="额外重试次数" min={0} max={20} value={Number(node.data.retries??node.data.maxRetries??0)} disabled={editDisabled} onChange={value=>update({retries:value,maxRetries:undefined})}/></label>}</div></>}
  {catalogError&&<Alert type="error" message={catalogError}/>}
  {response&&(send||wait)&&<Checkbox checked={useResponse} onChange={event=>setUseResponse(event.target.checked)}>引用所选运行的响应</Checkbox>}
  {send&&<><div className="flow-field-heading"><strong>字段值</strong><Button size="small" disabled={!currentMessage||editDisabled} onClick={()=>{onDraft({...draft,values:undefined});update({values:currentMessage!.defaultValues})}}>载入字段默认值</Button></div>{!!currentMessage?.fields.length&&<FlowFields fields={currentMessage.fields} values={(node.data.values||{}) as Record<string,unknown>} code={code} disabled={editDisabled||!!valuesError} onChange={values=>{onDraft({...draft,values:undefined});update({values})}}/>}<details className="flow-values-json" open={!!valuesError||!currentMessage?.fields.length}><summary>字段值 JSON</summary><Input.TextArea id="flow-values" aria-label="字段值 JSON" value={valueText} onChange={event=>editValues(event.target.value)} disabled={editDisabled} autoSize={{minRows:6,maxRows:12}} className="code-input"/></details>{valuesError&&<Alert type="error" message={valuesError}/>}</>}
  {(send||wait)&&<section className="flow-preview" aria-label="发送预览"><div className="flow-field-heading"><strong>{send?'发送预览':'等待条件'}</strong><Button onClick={()=>void inspect()} loading={previewing} disabled={!!previewBlock}>{send?'预览报文':'检查等待条件'}</Button></div>{previewBlock&&<span className="flow-inline-error">{previewBlock}</span>}{previewError&&<Alert type="error" message={previewError}/>} {!!preview?.missingVariables.length&&<Alert type="warning" message={'缺少变量：'+preview.missingVariables.join('、')}/>} {!!preview?.diagnostics.length&&<Alert type="warning" message={preview.diagnostics.join('；')}/>} {!!preview?.placeholderFields.length&&<Alert type="info" message={'预览占位：'+preview.placeholderFields.join('、')+'；实际发送时重新生成'}/>} {!!preview?.expectedCodes.length&&<div className="flow-expected">等待 {preview.expectedCodes.map(hexCode).join(' / ')}</div>}{preview?.hex&&<><Tag>未发送</Tag><PacketInspector hex={preview.hex} decoded={preview.decoded}/></>}{preview&&!preview.hex&&!preview.missingVariables.length&&!preview.diagnostics.length&&!preview.expectedCodes.length&&<span className="muted">无待编码字段</span>}</section>}
  <details className="flow-advanced" open={!send&&!wait||advancedPending}><summary>高级 JSON</summary><Input.TextArea disabled={disabled} aria-label="节点高级 JSON" value={draft.advanced??JSON.stringify(node.data,null,2)} onChange={event=>onDraft({...draft,advanced:event.target.value})} autoSize={{minRows:5,maxRows:14}} className="code-input"/><div className="toolbar"><Button disabled={disabled||!advancedPending} onClick={()=>{try{const data=parseObjectJson(draft.advanced!,'节点属性');onChange(data);onDraft({});setPreviewError('')}catch(error){setPreviewError(String(error))}}}>应用 JSON 属性</Button><Button disabled={disabled||!advancedPending} onClick={()=>{onDraft({...draft,advanced:undefined});setPreviewError('')}}>撤销 JSON 修改</Button></div>{advancedPending&&<span className="flow-inline-error">高级 JSON 尚未应用</span>}</details>
 </div>;
}
