import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { ReactFlow, Background, Controls, Handle, Position, type Node, type NodeProps, type Edge, type OnNodesChange } from "@xyflow/react";
import { ArrowLeft, ArrowRight, CheckCircle2, Circle, CircleStop, Clock3, Eye, Play, XCircle } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { DatagramDetail } from "@/components/monitor/DatagramDetail";
import { getFlowTraces, listNodes, listTerminals, startFlowRun, stopFlowRun, type FlowRow, type FlowTrace, type FlowTraceEvent } from "@/lib/api";
import type { Node as Channel, Protocol, ProtocolGroup, TerminalProfile } from "@/types";
import { cn } from "@/lib/utils";
import "./flow-test.css";

export const traceStatus: Record<string,string> = { running:"执行中", delay:"延时中", waiting_send:"等待发送", waiting:"等待接收", sent:"已发送", success:"完成", failure:"失败", timeout:"超时", done:"完成", issues:"有失败 / 超时", error:"中止", aborted:"已停止", unmatched:"未匹配", pending:"未执行" };
export function useFlowTraces(flowId: number) {
  const [traces,setTraces]=useState<FlowTrace[]>([]);
  const [error,setError]=useState("");
  const refresh=useCallback(async()=>{ try { setTraces(await getFlowTraces(flowId)); setError(""); } catch(e){setError(String(e));} },[flowId]);
  useEffect(()=>{ let cancelled=false; let timer:ReturnType<typeof setTimeout>;
    setTraces([]);
    const tick=async()=>{ try {const rows=await getFlowTraces(flowId); if(!cancelled){setTraces(rows);setError("");}}catch(e){if(!cancelled)setError(String(e));} if(!cancelled)timer=setTimeout(tick,500); };
    void tick(); return()=>{cancelled=true;clearTimeout(timer);};
  },[flowId]);
  return {traces,error,refresh};
}
function Status({status}:{status:string}) {
  const Icon=status==="success"||status==="done"?CheckCircle2:["failure","error","issues"].includes(status)?XCircle:["waiting","waiting_send","timeout","delay"].includes(status)?Clock3:Circle;
  return <span className={cn("flow-status",`state-${status}`)}><Icon size={14}/>{traceStatus[status]??status}</span>;
}
type TestNode = Node<{name:string; protocol:string; expect:string; state:string; elapsed:string; attempt:number},"test">;
function TestStep({data,selected}:NodeProps<TestNode>){ return <div className={cn("flow-test-node",selected&&"selected",`state-${data.state}`)}>
  <Handle type="target" position={Position.Top}/><div className="flex justify-between gap-3"><strong>{data.name}</strong><Status status={data.state}/></div>
  <p className="mt-2 text-xs text-muted-foreground break-words">{data.protocol}</p>
  {data.expect&&<p className="mt-1 text-xs text-muted-foreground break-words">等待 {data.expect}</p>}
  <div className="mt-2 flex justify-between text-xs text-muted-foreground"><span>{data.attempt?`执行 #${data.attempt}`:""}</span><span className="font-mono">{data.elapsed}</span></div>
  <Handle type="source" position={Position.Bottom}/>
</div>; }
const nodeTypes={test:TestStep};
const time=(at:number)=>new Date(at).toLocaleTimeString("zh-CN",{hour12:false})+"."+String(at%1000).padStart(3,"0");
const edgeFields=["next","on_success","on_failure","on_timeout"] as const;
const edgeLabels:Record<string,string>={next:"下一步",on_success:"成功",on_failure:"失败",on_timeout:"超时",route:"条件"};

export function FlowTestView({flow,protocols,groups,onBack,initialNodeId,initialRunId,initialTerminalIds=[]}: {flow:FlowRow;protocols:Protocol[];groups:ProtocolGroup[];onBack:()=>void;initialNodeId?:number;initialRunId?:number;initialTerminalIds?:number[]}){
  const {traces,error,refresh}=useFlowTraces(flow.id);
  const [runId,setRunId]=useState<number|null>(initialRunId??null);
  const [channels,setChannels]=useState<Channel[]>([]);
  const [devices,setDevices]=useState<TerminalProfile[]>([]);
  const [nodeId,setNodeId]=useState(initialNodeId??0);
  const [deviceIds,setDeviceIds]=useState<number[]>(initialTerminalIds);
  const [busy,setBusy]=useState(false);
  const [tab,setTab]=useState<"graph"|"sequence">("graph");
  const [stepId,setStepId]=useState<string|null>(null);
  const [detail,setDetail]=useState<number|null>(null);
  const [now,setNow]=useState(Date.now());
  const [follow,setFollow]=useState(true);
  // Controlled nodes must retain measurements across timer and trace updates;
  // otherwise ReactFlow clears handle bounds and unmounts the connected edges.
  const [measurements,setMeasurements]=useState<Record<string,{width:number;height:number}>>({});
  const onNodesChange=useCallback<OnNodesChange<TestNode>>(changes=>{
    setMeasurements(previous=>{
      let next=previous;
      for(const change of changes)if(change.type==="dimensions"&&change.dimensions){
        const old=previous[change.id];
        if(old?.width!==change.dimensions.width||old?.height!==change.dimensions.height){
          if(next===previous)next={...previous};
          next[change.id]=change.dimensions;
        }
      }
      return next;
    });
  },[]);
  const eventScroll=useRef<HTMLDivElement>(null);
  const trace=traces.find(t=>t.run_id===runId)??traces[0];
  const active=trace&&!trace.ended_at?trace:traces.find(t=>!t.ended_at&&t.node_id===nodeId);
  useEffect(()=>{ if(active) { setNodeId(active.node_id); setDeviceIds(active.terminal_ids); } },[active?.run_id,active?.node_id]);
  const shown=trace?.flow??flow;
  useEffect(()=>{if(follow&&eventScroll.current)eventScroll.current.scrollTop=eventScroll.current.scrollHeight;},[trace?.run_id,trace?.events.length,stepId,follow]);
  const name=useCallback((id?:string|null)=>protocols.find(p=>p.protocol_id===id)?.name??id??"",[protocols]);
  useEffect(()=>{let cancelled=false;Promise.all([listNodes(),listTerminals()]).then(([nodes,terminals])=>{
    if(cancelled)return; const allowed=groups.filter(g=>g.system_ref===flow.system_ref).map(g=>g.id);
    const scoped=nodes.filter(n=>{try{return (JSON.parse(n.group_bind_json??"[]") as number[]).some(id=>allowed.includes(id));}catch{return false;}});
    setChannels(scoped);setDevices(terminals);setNodeId(id=>scoped.some(n=>n.id===id)?id:scoped[0]?.id??0);
  }).catch(e=>toast.error(String(e)));return()=>{cancelled=true;};},[flow.system_ref,groups]);
  useEffect(()=>{const t=setInterval(()=>setNow(Date.now()),250);return()=>clearInterval(t);},[]);
  const states=useMemo(()=>{
    return new Map(Object.entries(trace?.step_states??{}));
  },[trace]);
  const nodes=useMemo(()=>shown.steps.map((s,i):TestNode=>{
    const e=states.get(s.id);const working=e&&["running","waiting","waiting_send","delay"].includes(e.status);
    const state=working&&trace?.ended_at?(trace.status==="aborted"?"aborted":"error"):e?.status??"pending";
    return {id:s.id,type:"test",measured:measurements[s.id],position:shown.steps.every(s=>!s.x&&!s.y)?{x:(i%3)*300,y:Math.floor(i/3)*200}:{x:s.x,y:s.y}, selected:stepId===s.id,
      data:{name:s.name,protocol:(s.action==="receive"?"接收 ":"发送 ")+name(s.protocol_id),expect:name(s.expect_protocol),state,elapsed:working&&!trace?.ended_at?`${Math.max(0,(now-e.at)/1000).toFixed(1)} / ${(s.delay_ms&&state==='delay'?s.delay_ms:s.timeout_ms)/1000} s`:"",attempt:e?.attempt??0}};
  }),[shown,states,stepId,name,now,trace,measurements]);
  const edges=useMemo(()=>{
    const list:Edge[]=[];
    for(const s of shown.steps){
      const links=[...edgeFields.map(k=>({to:s[k],branch:k,label:edgeLabels[k]})),...(s.routes??[]).map(r=>({to:r.to,branch:"route",label:`${r.field}=${r.value}`}))];
      for(const [i,l] of links.entries())if(l.to){const used=trace?.transitions.some(e=>e.step_id===s.id&&e.next_step===l.to&&e.branch===l.branch);
        list.push({id:`${s.id}-${i}-${l.to}`,source:s.id,target:l.to,label:l.label,type:"smoothstep",className:used?"flow-edge-used":"flow-edge-pending",style:{strokeWidth:used?3:1.5},labelStyle:{fontSize:11},labelBgStyle:{fill:"hsl(var(--card))"}});
      }
    }return list;
  },[shown,trace]);
  const packets=useMemo(()=>{
    const map=new Map<number,FlowTraceEvent>();for(const e of trace?.events??[])if(e.packet)map.set(e.packet.id,e);
    return [...map.values()].sort((a,b)=>a.packet!.ts.localeCompare(b.packet!.ts)||a.index-b.index);
  },[trace]);
  const start=async(manual:boolean)=>{setBusy(true);try{const id=await startFlowRun(flow.id,deviceIds,nodeId,manual);setRunId(id);setStepId(null);await refresh();}catch(e){toast.error("无法开始测试",{description:String(e)});}finally{setBusy(false);}};
  const stop=async()=>{if(!active)return;setBusy(true);try{await stopFlowRun(active.run_id);await refresh();}catch(e){toast.error(String(e));}finally{setBusy(false);}};
  const selected=shown.steps.find(s=>s.id===stepId);
  const selectedEvents=(trace?.events??[]).filter(e=>!stepId||e.step_id===stepId).filter(e=>e.status!=="transition");
  return <div className="flow-test-view">
    <header className="flex flex-wrap items-center gap-3 border-b px-4 py-3">
      <Button variant="ghost" size="sm" onClick={onBack}><ArrowLeft/>返回</Button><strong className="mr-auto text-sm">{flow.name}</strong>
      <select aria-label="测试记录" className="flow-select" value={trace?.run_id??""} onChange={e=>{setRunId(Number(e.target.value));setStepId(null);}}>
        {!traces.length&&<option value="">暂无测试记录</option>}{traces.map(t=><option key={t.run_id} value={t.run_id}>#{t.run_id} · {time(t.started_at)} · {t.manual?"手动跟踪":"自动运行"} · {traceStatus[t.status]}</option>)}
      </select>{trace&&<><span className="text-xs text-muted-foreground">{trace.node_name}</span><Status status={trace.status}/></>}
    </header>
    <div className="flex flex-wrap items-center gap-2 border-b px-4 py-2">
      <label className="flex items-center gap-2 text-xs">通道<select aria-label="测试通道" className="flow-select" disabled={!!active} value={nodeId} onChange={e=>{setNodeId(Number(e.target.value));setDeviceIds([]);}}>{!channels.length&&<option value="0">请先绑定系统通道</option>}{channels.map(n=><option key={n.id} value={n.id}>{n.name} · {n.remote_ip}:{n.remote_port}</option>)}</select></label>
      <details className="relative"><summary className="cursor-pointer rounded border px-3 py-2 text-xs">设备 {deviceIds.length?`已选 ${deviceIds.length} 台`:"使用字段值"}</summary><div className="absolute z-20 mt-1 max-h-52 w-64 overflow-auto rounded border bg-card p-2 shadow-md">{devices.filter(d=>d.node_ref===nodeId&&d.system_ref===flow.system_ref).map(d=><label key={d.id} className="flex gap-2 p-2 text-xs"><input type="checkbox" disabled={!!active} checked={deviceIds.includes(d.id)} onChange={e=>setDeviceIds(v=>e.target.checked?[...v,d.id]:v.filter(i=>i!==d.id))}/>{d.name} · {d.device_key}</label>)}<p className="px-2 text-xs text-muted-foreground">不选设备时使用信令配置值</p></div></details>
      <div className="ml-auto flex gap-2">{active?<Button size="sm" variant="destructive" disabled={busy} onClick={stop}><CircleStop/>停止测试</Button>:<><Button size="sm" variant="outline" disabled={busy||!nodeId} onClick={()=>start(true)}><Eye/>手动跟踪</Button><Button size="sm" disabled={busy||!nodeId} onClick={()=>start(false)}><Play/>自动运行</Button></>}</div>
    </div>
    {error&&<p role="alert" className="px-4 py-2 text-sm text-destructive">{error}</p>}
    {active?.manual&&<p className="border-b px-4 py-2 text-xs text-muted-foreground">跟踪中 · 在交互控制台发送，或等待对端发送。此模式不自动发包。</p>}
    <div className="flow-test-content">
      <section className="flex min-h-0 min-w-0 flex-col">
        <div className="flex items-center gap-1 border-b px-3 py-2"><Button size="sm" variant={tab==="graph"?"secondary":"ghost"} onClick={()=>setTab("graph")}>流程进度</Button><Button size="sm" variant={tab==="sequence"?"secondary":"ghost"} onClick={()=>setTab("sequence")}>交互时序 <span className="font-mono">{packets.length}</span></Button><span className="ml-auto text-xs text-muted-foreground">本次应用会话</span></div>
        {tab==="graph"?<div className="min-h-0 flex-1"><ReactFlow key={shown.id+":"+(trace?.run_id??0)} nodes={nodes} edges={edges} nodeTypes={nodeTypes} onNodesChange={onNodesChange} nodesDraggable={false} nodesConnectable={false} onNodeClick={(_,n)=>setStepId(n.id)} onPaneClick={()=>setStepId(null)} fitView fitViewOptions={{padding:.25,maxZoom:1}} proOptions={{hideAttribution:true}}><Background gap={20} size={1}/><Controls showInteractive={false}/></ReactFlow></div>:<div className="flow-sequence">
          <div className="sequence-head"><span>模拟端</span><span>对端系统</span></div>
          {!packets.length&&<p className="p-6 text-center text-sm text-muted-foreground">尚无收发报文</p>}
          {packets.map(e=><button key={e.packet!.id} className={cn("sequence-message",e.packet!.direction==="in"&&"incoming",e.status==="unmatched"&&"unmatched")} onClick={()=>setDetail(e.packet!.id)}><span className="text-xs text-muted-foreground">{e.packet!.ts} · {e.step_name}</span><strong className="text-sm">{name(e.packet!.protocol_id)||"未识别信令"}</strong><span className="sequence-line"><ArrowRight size={16}/></span><span className="flex justify-between gap-2 text-xs"><span>{e.packet!.remote_addr}</span><span>{e.status==="unmatched"?e.note:`${(e.packet!.raw_hex??"").replace(/\s/g,"").length/2} B`}</span></span></button>)}
        </div>}
      </section>
      <aside className="flow-step-detail"><div className="flex items-center justify-between border-b p-3"><strong className="text-sm">{selected?.name??"执行记录"}</strong><label className="ml-auto mr-2 flex items-center gap-1 text-xs text-muted-foreground"><input type="checkbox" checked={follow} onChange={e=>setFollow(e.target.checked)}/>跟随最新</label>{stepId&&<Button variant="ghost" size="sm" onClick={()=>setStepId(null)}>全部步骤</Button>}</div>
        <div ref={eventScroll} className="min-h-0 flex-1 overflow-auto p-3">{!selectedEvents.length&&<p className="py-8 text-center text-sm text-muted-foreground">{trace?"此步骤尚未执行":"选择通道后开始测试"}</p>}
          {trace?.omitted? <p className="mb-2 text-xs text-muted-foreground">较早的 {trace.omitted} 条事件已折叠清理</p>:null}
          {selectedEvents.map(e=><div key={e.index} className="flow-event"><div className="flex items-center justify-between gap-2"><Status status={e.status}/><time className="font-mono text-xs text-muted-foreground">{time(e.at)}</time></div><p className="mt-1 text-xs">{!stepId&&e.step_name}{e.terminal?` · 设备 ${e.terminal}`:""}{e.attempt?` · #${e.attempt}`:""}</p><p className="mt-1 text-xs text-muted-foreground break-words">{e.note}</p>{e.packet&&<button className="flow-packet" onClick={()=>setDetail(e.packet!.id)}><span className="flex justify-between text-xs"><b>{e.packet.direction==="out"?"发 →":"← 收"} {name(e.packet.protocol_id)}</b><span>解析</span></span><code>{e.packet.raw_hex}</code></button>}</div>)}
        </div>
      </aside>
    </div>
    {detail!==null&&<DatagramDetail logId={detail} onClose={()=>setDetail(null)}/>}
  </div>;
}
