import {invoke, isTauri} from '@tauri-apps/api/core';
export interface Project{id:string;name:string;path?:string}
export interface System{id:string;projectId:string;name:string}
export interface Device{id:string;systemId:string;name:string;address:string;status:'OFFLINE'|'ONLINE'|'COMMUNICATING';source?:string;remote?:string;config?:Record<string,unknown>;identities?:{protocolId:string;address:number[]|string}[];configVersion?:number;activeCalls?:string[]}
export interface Protocol{id:string;protocolId:string;name:string;version:number;published:boolean;status?:string}
export interface Endpoint{id:string;name:string;bind:string;running:boolean;protocolId:string;virtualPeer?:boolean;sent?:number;received?:number}
export interface Diagnostic{message:string;severity?:string;line?:number;column?:number;path?:string}
export interface FieldSpan{path:string;labelZh:string;startOffset0:number;byteLength:number;rawHex:string;decimal?:string|number;binary?:string;decodedValue?:unknown;descriptionZh?:string;meaningZh?:string;children?:FieldSpan[];diagnostics?:string[]}
export interface Decoded{fields:FieldSpan[];diagnostics?:(Diagnostic|string)[];valid?:boolean;messageCode?:number;msgCode?:number}
export interface Packet{id:string;timestamp:number;direction:string;endpointId:string;remote:string;hex:string;length:number;msgCode?:number|null;status:string;decoded?:Decoded|null;runId?:string|null;nodeId?:string|null}
export interface ProtocolCatalogField{path:string;key:string;label:string;kind:string;bytes?:number|null;binding?:string|null;computed?:string|null;required?:boolean;generated?:boolean;fixed?:boolean;valueKind?:string;enums?:Record<string,string>;defaultValue?:unknown;children?:ProtocolCatalogField[];variants?:unknown[];itemTemplate?:Record<string,unknown>;condition?:unknown;sectionCondition?:unknown;secret?:boolean}
export interface ProtocolCatalogMessage{code:number;name:string;responseCodes:number[];fields:ProtocolCatalogField[];defaultValues:Record<string,unknown>;diagnostics:string[]}
export interface ProtocolCatalog{protocolId:string;messages:ProtocolCatalogMessage[]}
export interface FlowPreview{hex?:string;decoded?:Decoded;values?:Record<string,unknown>;missingVariables:string[];diagnostics:string[];expectedCodes:number[];previewOnly:true;placeholderFields:string[]}
export interface FlowNode{id:string;type:string;position?:{x:number;y:number};data:Record<string,unknown>}
export interface FlowEdge{id?:string;source:string;target:string;label?:string;sourceHandle?:string}
export interface Flow{id:string;nameZh:string;version?:string;published?:boolean;protocol?:{id:string;version:string};nodes:FlowNode[];edges:FlowEdge[];config?:Record<string,unknown>;defaultVariables?:Record<string,unknown>;inputs?:string[]|Record<string,unknown>}
export interface WorkflowTemplate{id:string;nameZh:string;description:string;kind:string;protocol:string;nodeCount:number;inputs?:string[]|Record<string,unknown>;defaultVariables?:Record<string,unknown>}
export interface Run{id:string;flowId?:string;definitionId?:string;status:string;currentNode?:string;trace?:{timeMs:number;nodeId:string;event:string}[];variables?:Record<string,unknown>;failure?:string;steps?:number;traceDropped?:number}
export interface Pending{id:string;runId?:string;expectedMsgCodes?:number[];transactionId?:number;deadline?:number;status?:string}
export interface Call{id:string;callId?:string;type?:string;status:string;participants?:unknown[];requestedRx?:number;requestedTx?:number;startupMode?:string}
export interface Resource{id:string;callId:string;deviceId:string;direction:string;status:string;bandwidth?:number;requestedBw?:number;reservedBw?:number;assignedBw?:number;releasedBw?:number;version?:string}
export interface Session{id:string;deviceId:string;peerSystemId:string;wireSessionId:number;epoch:string;validUntilMs:number;authRequired:boolean;authExpiresMs?:number;registered:boolean;active:boolean}
export interface Pool{id:string;name:string;capacity:number;available:number;unit:string}
export interface Snapshot{projects:Project[];projectId:string;systems:System[];devices:Device[];endpoints:Endpoint[];protocols:Protocol[];flows:Flow[];runs:Run[];pending:Pending[];calls:Call[];resources:Resource[];pools:Pool[];sessions?:Session[];stats:Record<string,number>;processingErrors?:number;processedPackets?:number;dataDir?:string}
export type Request = {action:string}&Record<string,unknown>;
export async function studio<T>(action:string,args:Record<string,unknown>={}):Promise<T>{if(!isTauri())throw new Error('请在 MUSP Studio 桌面应用中打开。');return invoke<T>('studio',{request:{action,...args}});}
export const api={snapshot:()=>studio<Snapshot>('snapshot'),source:(id?:string,expectedProjectId?:string)=>studio<{xml:string}>('protocol_source',{id,expectedProjectId}),validate:(xml:string)=>studio<{valid:boolean;diagnostics:Diagnostic[];ast?:unknown}>('protocol_validate',{xml}),saveProtocol:(xml:string,id?:string,name?:string)=>studio<{id:string}>('protocol_save',{xml,id,name}),publish:(xml:string,id?:string)=>studio<{id:string;version:string}>('protocol_publish',{xml,id}),encode:(args:Record<string,unknown>)=>studio<{hex:string;bytes:number[];decoded:Decoded}>('protocol_encode',args),decode:(args:Record<string,unknown>)=>studio<{decoded:Decoded}>('protocol_decode',args),packets:(offset=0,limit=200,filter?:string,direction?:string,expectedProjectId?:string)=>studio<{items:Packet[];total:number}>('packet_query',{offset,limit,filter,direction,expectedProjectId})};
