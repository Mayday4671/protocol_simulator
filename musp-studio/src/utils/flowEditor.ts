import type {Endpoint,FlowNode} from '../api/tauri';
export type FlowEditorData=Record<string,unknown>&{nodeKind:string;label:string};
export function flowNodeData(node:FlowNode,fallbackLabel:string):FlowEditorData{return {...node.data,nodeKind:node.type,label:String(node.data.label||fallbackLabel)}}
export function serializeNodeData(editor:FlowEditorData):{type:string;data:Record<string,unknown>}{const {nodeKind,...data}=editor;return {type:nodeKind,data}}
export function canRunFlow({published,savedPublished,dirty,busy,endpointId}:{published:boolean;savedPublished?:boolean;dirty:boolean;busy:boolean;endpointId?:string}):boolean{return published&&savedPublished===true&&!dirty&&!busy&&!!endpointId}
export function parseObjectJson(text:string,label:string):Record<string,unknown>{
 let value:unknown;try{value=JSON.parse(text)}catch{throw new Error(label+' JSON 格式错误')}
 if(!value||typeof value!=='object'||Array.isArray(value))throw new Error(label+'必须为 JSON 对象');
 return value as Record<string,unknown>;
}
export function matchingEndpoint(endpoints:Endpoint[],protocolId?:string,currentId?:string):string|undefined{
 const matching=endpoints.filter(endpoint=>endpoint.running&&endpoint.protocolId===protocolId);
 return matching.some(endpoint=>endpoint.id===currentId)?currentId:matching.length===1?matching[0].id:undefined;
}
export interface FlowStartState{flowId?:string;protocolId?:string;published:boolean;savedPublished?:boolean;dirty:boolean;busy:boolean;endpoint?:Endpoint;availableEndpoints:number;remote:string;variablesError?:string;nodeDraftError?:string}
export function flowStartReason(state:FlowStartState):string{
 if(state.busy)return '操作进行中';
 if(!state.flowId)return '选择或创建流程';
 if(!state.protocolId)return '绑定已发布协议';
 if(state.nodeDraftError)return state.nodeDraftError;
 if(state.variablesError)return state.variablesError;
 if(state.dirty)return '修改尚未发布，点击“校验并发布”';
 if(!state.published||!state.savedPublished)return '草稿需先“校验并发布”';
 if(!state.endpoint)return state.availableEndpoints?'选择运行端点（'+state.availableEndpoints+' 个可用）':'没有匹配协议的运行端点，请在系统设置中启动';
 if(!state.endpoint.running)return '运行端点已停止';
 if(state.endpoint.protocolId!==state.protocolId)return '端点协议与流程协议不一致';
 const remote=state.remote.trim();if(!remote)return '填写对端 IP:端口';
 const match=remote.match(/^(\d{1,3}(?:\.\d{1,3}){3}|\[[0-9a-fA-F:]+\]):(\d+)$/);
 if(!match||Number(match[2])<1||Number(match[2])>65535||(!match[1].startsWith('[')&&match[1].split('.').some(part=>Number(part)>255)))return '对端地址需为 IP:端口（端口 1–65535）';
 return '';
}
export interface FlowNodeDraft{values?:string;advanced?:string}
export function nodeDraftError(drafts:Record<string,FlowNodeDraft>):string{
 for(const [id,draft]of Object.entries(drafts)){
  if(draft.advanced!==undefined)return '节点 '+id+' 的高级 JSON 尚未应用';
  if(draft.values!==undefined){try{parseObjectJson(draft.values,'字段值')}catch(error){return '节点 '+id+'：'+String(error).replace(/^Error: /,'')}}
 }
 return '';
}
