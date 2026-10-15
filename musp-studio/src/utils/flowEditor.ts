import type {FlowNode} from '../api/tauri';
export type FlowEditorData=Record<string,unknown>&{nodeKind:string;label:string};
export function flowNodeData(node:FlowNode,fallbackLabel:string):FlowEditorData{return {...node.data,nodeKind:node.type,label:String(node.data.label||fallbackLabel)}}
export function serializeNodeData(editor:FlowEditorData):{type:string;data:Record<string,unknown>}{const {nodeKind,...data}=editor;return {type:nodeKind,data}}
export function canRunFlow({published,savedPublished,dirty,busy,endpointId}:{published:boolean;savedPublished?:boolean;dirty:boolean;busy:boolean;endpointId?:string}):boolean{return published&&savedPublished===true&&!dirty&&!busy&&!!endpointId}
