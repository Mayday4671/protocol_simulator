import {useEffect,useRef,useState} from 'react';
import {ReactFlow,Background,Controls,Handle,Position,addEdge,useNodesState,useEdgesState,type Node,type NodeProps,type Connection,type Edge} from '@xyflow/react';
import '@xyflow/react/dist/style.css';
import {Alert,Button,Input,Select,Dropdown,Tag,Modal} from 'antd';
import {PlusOutlined,SaveOutlined,PlayCircleOutlined,DeleteOutlined,UploadOutlined} from '@ant-design/icons';
import {useQuery} from '@tanstack/react-query';
import type {PageContext} from '../App';
import {studio,type Flow,type Run,type ProtocolCatalog,type WorkflowTemplate} from '../api/tauri';
import {useUI} from '../store/ui';
import {PageTitle,Panel,Blank,Status,useAction} from '../components/Shared';
import {RunControls} from '../components/RunControls';
import {FlowNodeEditor} from '../components/FlowNodeEditor';
import {FlowPackets} from '../components/FlowPackets';
import {flowNodeData,serializeNodeData,flowStartReason,matchingEndpoint,parseObjectJson,nodeDraftError,type FlowEditorData,type FlowNodeDraft} from '../utils/flowEditor';
import './flow-workspace.css';

const types:Record<string,string>={START:'开始',END:'结束',SEND:'发送报文',WAIT_PACKET:'等待报文',SEND_AND_WAIT:'发送并等待',CONDITION:'条件分支',ASSIGN:'变量赋值',DELAY:'延时',LOOP:'循环',PARALLEL:'并行',JOIN:'合并',INVOKE_SUBFLOW:'调用子流程',UPDATE_DEVICE:'更新设备',UPDATE_SESSION:'更新会话',UPDATE_CALL:'更新通话',UPDATE_RESOURCE:'更新资源',RAISE_EVENT:'发出事件',ERROR:'异常'};
type FlowUINode=Node<FlowEditorData>;
const codeLabel=(value:unknown)=>typeof value==='number'?'0x'+value.toString(16).padStart(2,'0').toUpperCase():String(value??'');
function WorkNode({data,selected}:NodeProps<FlowUINode>){return <div className={'work-node node-'+data.nodeKind.toLowerCase()+(selected?' selected':'')}><Handle type="target" position={Position.Left}/><span className="node-kind">{data.nodeKind}</span><strong>{data.label}</strong><small>{codeLabel(data.code??data.requestMsgCode??data.expression??types[data.nodeKind])}</small><Handle type="source" position={Position.Right}/></div>}
const nodeTypes={work:WorkNode};
function toNodes(flow:Flow):FlowUINode[]{return flow.nodes.map((node,index)=>({id:node.id,type:'work',position:node.position||{x:60+(index%3)*230,y:60+Math.floor(index/3)*140},data:flowNodeData(node,types[node.type]||node.type)}))}
function toEdges(flow:Flow):Edge[]{return flow.edges.map((edge,index)=>({...edge,id:edge.id||'e'+index,sourceHandle:undefined,animated:false}))}
const runTime=(run:Run)=>(run as Run&{startedMs?:number}).startedMs??run.trace?.[0]?.timeMs??0;
export default function Flows(props:PageContext){return <FlowWorkspace key={props.projectId} {...props}/>}

function FlowWorkspace({data,projectId}:PageContext){
 const {perform,busy,message}=useAction();
 const [saving,setSaving]=useState(false);const saveInFlight=useRef(false);const editorLocked=saving||busy;
 const [id,setId]=useState<string>();const [baseFlow,setBaseFlow]=useState<Flow>();const [published,setPublished]=useState(false);
 const [name,setName]=useState('新建交互流程');const [protocol,setProtocol]=useState<string>();
 const [nodes,setNodes,onNodesChange]=useNodesState<FlowUINode>([]);const [edges,setEdges,onEdgesChange]=useEdgesState<Edge>([]);
 const [selected,setSelected]=useState<string>();const [edgeId,setEdgeId]=useState<string>();const [drafts,setDrafts]=useState<Record<string,FlowNodeDraft>>({});
 const [endpoint,setEndpoint]=useState<string>();const [remote,setRemote]=useState('127.0.0.1:9002');const [variables,setVariables]=useState('{}');
 const [dirty,setDirty]=useState(false);const [runId,setRunId]=useState<string>();const [templateId,setTemplateId]=useState<string>();const [templateOpen,setTemplateOpen]=useState(false);
 const initialized=useRef(false);const requestedFlow=useUI(state=>state.flowId);
 const catalog=useQuery({queryKey:['flow-catalog',projectId,protocol],queryFn:()=>studio<ProtocolCatalog>('protocol_catalog',{protocolId:protocol,expectedProjectId:projectId}),enabled:!!protocol});
 const templates=useQuery({queryKey:['flow-templates',projectId],queryFn:()=>studio<WorkflowTemplate[]>('template_list',{expectedProjectId:projectId}),enabled:templateOpen});
 const selectedNode=nodes.find(node=>node.id===selected);const selectedEdge=edges.find(edge=>edge.id===edgeId);
 const matching=data?.endpoints.filter(item=>item.running&&item.protocolId===protocol)||[];
 const runs=(data?.runs.filter(run=>(run.flowId||run.definitionId)===id)||[]).sort((left,right)=>runTime(right)-runTime(left));
 const currentRun=runs.find(run=>run.id===runId);const pending=data?.pending.filter(item=>item.runId===runId)||[];
 let parsedVariables:Record<string,unknown>={};let variablesError='';try{parsedVariables=parseObjectJson(variables,'运行变量')}catch(error){variablesError=String(error).replace(/^Error: /,'')}
 const draftError=nodeDraftError(drafts);
 const startReason=flowStartReason({flowId:id,protocolId:protocol,published,savedPublished:data?.flows.find(flow=>flow.id===id)?.published,dirty,busy:editorLocked,endpoint:data?.endpoints.find(item=>item.id===endpoint),availableEndpoints:matching.length,remote,variablesError,nodeDraftError:draftError});
 useEffect(()=>{setEndpoint(current=>matchingEndpoint(data?.endpoints||[],protocol,current))},[data?.endpoints,protocol]);
 useEffect(()=>{
  if(editorLocked)return;
  if(requestedFlow){const target=data?.flows.find(flow=>flow.id===requestedFlow);if(target){showFlow(target);useUI.getState().setFlow('');initialized.current=true;return}}
  if(initialized.current||!data?.flows.length)return;
  const recent=sessionStorage.getItem('musp-flow:'+projectId);
  const lastRun=[...(data.runs||[])].sort((left,right)=>runTime(right)-runTime(left)).find(run=>data.flows.some(flow=>flow.id===(run.flowId||run.definitionId)));
  showFlow(data.flows.find(flow=>flow.id===recent)||data.flows.find(flow=>flow.id===(lastRun?.flowId||lastRun?.definitionId))||data.flows[0]);initialized.current=true;
 },[data?.flows,requestedFlow,projectId,editorLocked]);
 function showFlow(flow:Flow){
  setId(flow.id);setBaseFlow(flow);setPublished(flow.published===true);setName(flow.nameZh);setProtocol(flow.protocol?.id);
  setNodes(toNodes(flow));setEdges(toEdges(flow));setVariables(JSON.stringify(flow.defaultVariables||{},null,2));setDrafts({});setDirty(false);setEdgeId(undefined);
  setSelected(flow.nodes.find(node=>['SEND','SEND_AND_WAIT','WAIT_PACKET'].includes(node.type))?.id||flow.nodes[0]?.id);
  setRunId(data?.runs.filter(run=>(run.flowId||run.definitionId)===flow.id).sort((left,right)=>runTime(right)-runTime(left))[0]?.id);
  sessionStorage.setItem('musp-flow:'+projectId,flow.id);
 }
 function changeFlow(value:string){if(editorLocked)return;const flow=data?.flows.find(item=>item.id===value);if(!flow)return;if(dirty||draftError){Modal.confirm({title:'放弃当前未发布修改？',okText:'切换流程',cancelText:'继续编辑',onOk:()=>{if(!saveInFlight.current)showFlow(flow)}})}else showFlow(flow)}
 function newFlow(){if(editorLocked)return;initialized.current=true;setBaseFlow(undefined);setId(undefined);setPublished(false);setName('新建交互流程');setProtocol(undefined);setNodes(toNodes({id:'',nameZh:'',nodes:[{id:'start',type:'START',data:{label:'开始'}},{id:'end',type:'END',data:{label:'结束'}}],edges:[]}));setEdges([]);setSelected('start');setEdgeId(undefined);setDrafts({});setVariables('{}');setDirty(true);setRunId(undefined)}
 function add(kind:string){
  if(editorLocked)return;
  const nodeId=crypto.randomUUID();const signal=catalog.data?.messages?.[0];let values:Record<string,unknown>={};
  if(['SEND','SEND_AND_WAIT'].includes(kind))values={...(signal?{code:signal.code}:{}),values:signal?.defaultValues||{}};
  if(['WAIT_PACKET','SEND_AND_WAIT'].includes(kind))values={...values,expectedCodes:kind==='SEND_AND_WAIT'?(signal?.responseCodes||[]):signal?[signal.code]:[],timeoutMs:3000,...(kind==='SEND_AND_WAIT'?{retries:0}:{})};
  setNodes(current=>[...current,{id:nodeId,type:'work',position:{x:80+(current.length%3)*220,y:60+Math.floor(current.length/3)*140},data:{nodeKind:kind,label:types[kind],...values}}]);setSelected(nodeId);setEdgeId(undefined);setDirty(true);
 }
 function changeNode(data:Record<string,unknown>){if(editorLocked||!selectedNode)return;setNodes(current=>current.map(node=>node.id===selectedNode.id?{...node,data:flowNodeData({id:node.id,type:node.data.nodeKind,data},types[node.data.nodeKind])}:node));setDirty(true)}
 function deleteSelection(){if(editorLocked)return;if(selected){setNodes(current=>current.filter(node=>node.id!==selected));setEdges(current=>current.filter(edge=>edge.source!==selected&&edge.target!==selected));setDrafts(current=>{const next={...current};delete next[selected];return next});setSelected(undefined)}else{setEdges(current=>current.filter(edge=>edge.id!==edgeId));setEdgeId(undefined)}setDirty(true)}
 function connect(connection:Connection){if(editorLocked)return;setEdges(current=>addEdge({...connection,label:'success'},current));setDirty(true)}
 function serialize():Flow{const bound=data?.protocols.find(item=>item.id===protocol);return {...baseFlow,id:id||crypto.randomUUID(),nameZh:name,version:baseFlow?.version||'0.1.0',protocol:bound?{id:bound.id,version:String(bound.version)}:undefined,config:baseFlow?.config||{maxDurationMs:60000,maxConcurrentInstances:200},defaultVariables:parseObjectJson(variables,'运行变量'),nodes:nodes.map(node=>({id:node.id,position:node.position,...serializeNodeData(node.data)})),edges:edges.map(edge=>({id:edge.id,source:edge.source,target:edge.target,label:typeof edge.label==='string'?edge.label:undefined}))}}
 async function save(publish=false){
  if(editorLocked||saveInFlight.current)return;
  if(draftError){message.error(draftError);return}
  saveInFlight.current=true;setSaving(true);
  try{const flow=serialize();setPublished(false);const result=await perform<Flow>('flow_save',{flow,expectedProjectId:projectId},'流程草稿已保存');if(!result)return;const savedId=result.id||flow.id;setId(savedId);setBaseFlow(result);setDirty(false);sessionStorage.setItem('musp-flow:'+projectId,savedId);if(publish){const version=await perform<Flow>('flow_publish',{flowId:savedId,expectedProjectId:projectId},'流程已发布');if(version){setPublished(true);setBaseFlow(version)}}}catch(error){message.error(String(error))}finally{saveInFlight.current=false;setSaving(false)}
 }
 async function start(){if(startReason){message.warning(startReason);return}const result=await perform<{id?:string;runId?:string}>('flow_run',{flowId:id,endpointId:endpoint,remote:remote.trim(),variables:parsedVariables,expectedProjectId:projectId},'流程已启动');if(result)setRunId(result.runId||result.id)}
 async function install(){if(editorLocked)return;const flow=await perform<Flow>('template_install',{id:templateId,protocolId:protocol,expectedProjectId:projectId},'模板已创建');if(flow){showFlow(flow);initialized.current=true;setTemplateOpen(false)}}
 const nodeNames=Object.fromEntries(nodes.map(node=>[node.id,node.data.label]));
 const response=currentRun?.variables?.response;
 return <div className="flow-workspace">
  <PageTitle title="流程联调" actions={<><Select disabled={editorLocked} aria-label="流程选择" placeholder="选择已保存流程" value={id} onChange={changeFlow} options={data?.flows.map(flow=>({label:flow.nameZh,value:flow.id}))} style={{width:230}}/><Button disabled={editorLocked} icon={<PlusOutlined/>} onClick={newFlow}>新建流程</Button><Button disabled={editorLocked} onClick={()=>setTemplateOpen(true)}>从模板创建</Button></>}/>
  <div className="flow-run-bar" aria-label="流程运行栏">
   <label>运行端点<Select disabled={editorLocked} aria-label="运行端点" placeholder="运行端点" value={endpoint} onChange={setEndpoint} options={matching.map(item=>({value:item.id,label:item.name+' · '+item.bind}))}/></label>
   <label>对端地址<Input disabled={editorLocked} aria-label="对端地址" value={remote} onChange={event=>setRemote(event.target.value)} placeholder="IP:端口"/></label>
   <Button type="primary" icon={<PlayCircleOutlined/>} disabled={!!startReason} loading={busy} onClick={()=>void start()}>启动流程</Button>
   <div className="flow-start-status" aria-live="polite">{startReason?<span className="flow-inline-error">{startReason}</span>:<span className="flow-ready">可启动</span>}</div>
   <details className="flow-run-variables"><summary>运行变量</summary><Input.TextArea disabled={editorLocked} aria-label="运行变量 JSON" value={variables} onChange={event=>setVariables(event.target.value)} rows={4} className="code-input"/>{variablesError&&<span className="flow-inline-error">{variablesError}</span>}</details>
  </div>
  <div className="flow-edit-bar">
   <Input disabled={editorLocked} aria-label="流程名称" value={name} onChange={event=>{setName(event.target.value);setDirty(true)}}/>
   <Select disabled={editorLocked} aria-label="绑定发布协议" placeholder="绑定发布协议" value={protocol} onChange={value=>{setProtocol(value);setDirty(true)}} options={data?.protocols.filter(item=>item.published).map(item=>({value:item.id,label:item.name+' · v'+item.version}))}/>
   {dirty?<Tag color="orange">未保存</Tag>:<Tag color={published?'green':'orange'}>{published?'已发布':'草稿 · 需发布后运行'}</Tag>}
   <span className="toolbar-spacer"/><Button icon={<SaveOutlined/>} disabled={editorLocked||!nodes.length||!!draftError||!!variablesError} loading={saving} onClick={()=>void save()}>保存草稿</Button><Button icon={<UploadOutlined/>} disabled={editorLocked||!nodes.length||!!draftError||!!variablesError} loading={saving} onClick={()=>void save(true)}>校验并发布</Button>
  </div>
  <div className="flow-test-grid">
   <Panel title="流程画布" className="flow-graph-panel" extra={<Dropdown disabled={editorLocked} menu={{items:Object.entries(types).map(([key,label])=>({key,label})),onClick:({key})=>add(key)}}><Button disabled={editorLocked} icon={<PlusOutlined/>}>添加节点</Button></Dropdown>}>
    <div className="flow-node-shortcuts">{['SEND','SEND_AND_WAIT','WAIT_PACKET'].map(kind=><Button disabled={editorLocked} key={kind} onClick={()=>add(kind)}>{types[kind]}</Button>)}</div>
    <div className="flow-graph"><ReactFlow key={id||'new'} nodes={nodes} edges={edges} nodeTypes={nodeTypes} nodesDraggable={!editorLocked} nodesConnectable={!editorLocked} onNodesChange={changes=>{if(editorLocked){onNodesChange(changes.filter(change=>change.type==='select'||change.type==='dimensions'));return}onNodesChange(changes);if(changes.some(change=>['position','add','remove','replace'].includes(change.type)))setDirty(true);const removed=changes.filter(change=>change.type==='remove').map(change=>change.id);if(removed.length)setDrafts(current=>Object.fromEntries(Object.entries(current).filter(([key])=>!removed.includes(key))))}} onEdgesChange={changes=>{if(editorLocked){onEdgesChange(changes.filter(change=>change.type==='select'));return}onEdgesChange(changes);if(changes.some(change=>change.type!=='select'))setDirty(true)}} onConnect={connect} onNodeClick={(_event,node)=>{setSelected(node.id);setEdgeId(undefined)}} onEdgeClick={(_event,edge)=>{setSelected(undefined);setEdgeId(edge.id)}} fitView minZoom={.2} snapToGrid snapGrid={[20,20]} deleteKeyCode={editorLocked?null:'Delete'} defaultEdgeOptions={{type:'smoothstep',style:{stroke:'#45a6f8',strokeWidth:1.5}}}><Background color="#264159" gap={20}/><Controls/></ReactFlow>{!nodes.length&&<div className="flow-graph-empty"><Blank text="新建流程或从模板创建"/></div>}</div>
    <div className="flow-node-index" aria-label="流程节点">{nodes.map(node=><button key={node.id} className={selected===node.id?'selected':''} onClick={()=>{setSelected(node.id);setEdgeId(undefined)}}><small>{types[node.data.nodeKind]||node.data.nodeKind}</small><span>{node.data.label}</span></button>)}</div>
   </Panel>
   <Panel title={selectedEdge?'分支配置':'节点配置'} className="flow-node-panel" extra={(selectedNode||selectedEdge)&&<Button disabled={editorLocked} aria-label="删除所选节点或连线" danger type="text" icon={<DeleteOutlined/>} onClick={deleteSelection}/> }>
    {selectedNode?<FlowNodeEditor disabled={editorLocked} key={projectId+':'+id+':'+selectedNode.id} projectId={projectId} protocolId={protocol} node={{id:selectedNode.id,...serializeNodeData(selectedNode.data)}} catalog={catalog.data} catalogError={catalog.error?String(catalog.error):undefined} variables={parsedVariables} variablesError={variablesError} response={response&&typeof response==='object'&&!Array.isArray(response)?response as Record<string,unknown>:undefined} draft={drafts[selectedNode.id]||{}} onDraft={draft=>{if(editorLocked)return;setDrafts(current=>({...current,[selectedNode.id]:draft}));setDirty(true)}} onChange={changeNode}/>:selectedEdge?<label className="flow-field">分支标签<Input disabled={editorLocked} value={String(selectedEdge.label||'')} onChange={event=>{setEdges(current=>current.map(edge=>edge.id===edgeId?{...edge,label:event.target.value}:edge));setDirty(true)}} placeholder="success / matched / timeout / true / false"/></label>:<Blank text="选择节点或连线"/>}
   </Panel>
   <Panel title="运行收发" className="flow-observe" extra={currentRun&&<Status value={currentRun.status}/>}>
    <Select aria-label="查看运行实例" placeholder="查看运行实例" value={runId} onChange={setRunId} options={[...(runId&&!runs.some(run=>run.id===runId)?[{value:runId,label:runId.slice(0,8)+' · 读取状态中'}]:[]),...runs.map(run=>({value:run.id,label:(runTime(run)?new Date(runTime(run)).toLocaleTimeString('zh-CN',{hour12:false})+' · ':'')+run.id.slice(0,8)}))]} className="flow-run-select"/>
    {currentRun&&<div className="flow-run-state"><span>当前节点：{nodeNames[currentRun.currentNode||'']||currentRun.currentNode||currentRun.trace?.at(-1)?.nodeId||'—'}</span><RunControls runId={currentRun.id} status={currentRun.status}/></div>}
    {currentRun?.failure&&<Alert type="error" showIcon message={currentRun.failure}/>}
    {!!pending.length&&<div className="flow-waits">{pending.map(item=><div key={item.id}><Status value={item.status||'WAITING'}/><span>等待 {item.expectedMsgCodes?.map(codeLabel).join(' / ')||'响应'}</span>{item.deadline&&<time>至 {new Date(item.deadline).toLocaleTimeString('zh-CN',{hour12:false})}</time>}</div>)}</div>}
    <FlowPackets key={projectId+':'+id} projectId={projectId} flowId={id} runId={runId} endpointId={endpoint} catalog={catalog.data} nodeNames={nodeNames}/>
    {currentRun&&<details className="flow-run-trace"><summary>执行轨迹（{currentRun.steps??0} 步）</summary><pre className="trace-view">{JSON.stringify(currentRun.trace||[],null,2)}</pre></details>}
   </Panel>
  </div>
  <Modal title="从模板创建" open={templateOpen} onCancel={()=>setTemplateOpen(false)} onOk={()=>void install()} okText="创建流程" cancelText="取消" okButtonProps={{disabled:editorLocked||!templateId}} confirmLoading={busy}><Select disabled={editorLocked} aria-label="业务流程模板" placeholder="选择内置业务模板" value={templateId} onChange={setTemplateId} options={templates.data?.map(template=>({value:template.id,label:template.nameZh}))} style={{width:'100%'}}/>{templates.error&&<Alert type="error" message={String(templates.error)}/>}</Modal>
 </div>;
}
