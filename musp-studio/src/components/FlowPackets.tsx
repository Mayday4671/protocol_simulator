import {useEffect,useState} from 'react';
import {Alert,Pagination,Segmented,Tag} from 'antd';
import {useQuery} from '@tanstack/react-query';
import {studio,type Packet,type ProtocolCatalog} from '../api/tauri';
import {Blank} from './Shared';
import PacketInspector from './PacketInspector';
import {formatMessageCode} from '../utils/packet';
interface Props{projectId:string;flowId?:string;runId?:string;endpointId?:string;catalog?:ProtocolCatalog;nodeNames?:Record<string,string>}
export function FlowPackets({projectId,flowId,runId,endpointId,catalog,nodeNames={}}:Props){
 const [scope,setScope]=useState('run');const [page,setPage]=useState(1);const [selected,setSelected]=useState<string>();
 const context=JSON.stringify([projectId,flowId,runId,endpointId,scope]);
 useEffect(()=>{setSelected(undefined);setPage(1)},[context]);
 useEffect(()=>{if(runId)setScope('run')},[runId]);
 const enabled=scope==='run'?!!runId:!!endpointId;
 const query=useQuery({queryKey:['packets',projectId,'flow',flowId,scope,scope==='run'?runId:endpointId,page],queryFn:()=>studio<{items:Packet[];total:number}>('packet_query',{expectedProjectId:projectId,offset:(page-1)*100,limit:100,...(scope==='run'?{runId}:{endpointId})}),enabled,refetchInterval:enabled?750:false});
 const rows=enabled?(query.data?.items||[]).filter(packet=>scope!=='run'||packet.runId===runId):[];
 const packet=rows.find(item=>item.id===selected)||rows[0];
 return <section className="flow-packets" aria-label={scope==='run'?'本次收发':'端点收发'}>
  <div className="flow-packet-scope"><Segmented aria-label="报文范围" value={scope} options={[{value:'run',label:'本次运行'},{value:'endpoint',label:'当前端点'}]} onChange={value=>setScope(String(value))}/><span>{enabled?query.data?.total??0:0} 条</span></div>
  {query.error&&enabled&&<Alert type="error" message={String(query.error)}/>}
  <div className="flow-packet-list" aria-label="实际报文列表">
   {rows.map(item=><button key={item.id} type="button" data-packet-id={item.id} data-direction={item.direction} data-run-id={item.runId||''} data-node-id={item.nodeId||''} className={'flow-packet-row '+(packet?.id===item.id?'selected':'')} onClick={()=>setSelected(item.id)}><span className={item.direction==='TX'?'tx':'rx'}>{item.direction}</span><time>{new Date(item.timestamp).toLocaleTimeString('zh-CN',{hour12:false})}</time><strong>{formatMessageCode(item.msgCode)} {catalog?.messages.find(message=>message.code===item.msgCode)?.name||''}</strong><span>{item.length} B</span><small>{item.nodeId?nodeNames[item.nodeId]||item.nodeId:'未关联节点'} · {item.status}</small></button>)}
   {!rows.length&&<Blank text={!enabled?(scope==='run'?'启动流程或选择历史运行；也可切换到当前端点':'先选择运行端点'):query.isFetching?'正在读取报文…':'尚无收发报文'}/>}
  </div>
  {!!query.data?.total&&enabled&&<Pagination size="small" simple current={page} pageSize={100} total={query.data.total} showSizeChanger={false} onChange={setPage}/>}
  {packet&&<><div className="flow-packet-meta"><Tag color={packet.direction==='TX'?'green':'blue'}>{packet.direction}</Tag><code>{packet.remote}</code><span>{packet.status}</span></div><PacketInspector hex={packet.hex} decoded={packet.decoded??undefined}/></>}
 </section>;
}
