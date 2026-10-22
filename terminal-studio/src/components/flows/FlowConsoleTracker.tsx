import { forwardRef, useContext, useEffect, useImperativeHandle, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Eye, Play, CircleStop, ChevronLeft, ChevronRight, Workflow } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@/components/ui/button";
import { WorkspaceActivityContext } from "@/components/layout/WorkspaceActivity";
import { getFlowTraces, startFlowRun, stopFlowRun, type FlowRow } from "@/lib/api";
import type { Protocol, ProtocolGroup } from "@/types";
import { traceStatus, useFlowTraces } from "./FlowTestView";
import { DatagramDetail } from "@/components/monitor/DatagramDetail";
import { FlowTimeline } from "./FlowTimeline";
import { timelinePhaseCount } from "./timelinePhases";
import "./flow-drawer.css";

export interface FlowConsoleTrackerHandle {
  beforeSend(protocolId:string):Promise<void>;
}
export const FlowConsoleTracker = forwardRef<FlowConsoleTrackerHandle,{flows:FlowRow[];protocolId:string;nodeId?:number;terminalIds:number[];protocols:Protocol[];groups:ProtocolGroup[]}>(function FlowConsoleTracker({flows,protocolId,nodeId,terminalIds,protocols},ref){
  const [choice,setChoice]=useState<{id:number;nodeId?:number}|null>(null);
  const [held,setHeld]=useState<{flow:FlowRow;nodeId:number;runId:number|null}|null>(null);
  const [open,setOpen]=useState(false);const [busy,setBusy]=useState(false);
  const {active:visible}=useContext(WorkspaceActivityContext);
  const [runId,setRunId]=useState<number|null>(null);
  const [detail,setDetail]=useState<number|null>(null);
  const [follow,setFollow]=useState(true);
  const launcher=useRef<HTMLButtonElement>(null);
  const closer=useRef<HTMLButtonElement>(null);
  const starting=useRef<Promise<number>|null>(null);
  const candidates=protocolId?flows.filter(f=>f.steps.some(s=>s.protocol_id===protocolId||s.expect_protocol===protocolId)||f.trigger_protocols?.includes(protocolId)):[];
  const heldHere=held&&held.nodeId===nodeId&&flows.some(f=>f.id===held.flow.id)?held:null;
  const preferred=choice&&choice.nodeId===nodeId?candidates.find(f=>f.id===choice.id):undefined;
  const flow=heldHere?.flow??preferred??(candidates.length===1?candidates[0]:undefined);
  const {traces,refresh,error}=useFlowTraces(flow?.id??0);
  const scoped=traces.filter(t=>t.node_id===nodeId&&t.flow.id===flow?.id);
  const trace=scoped.find(t=>t.run_id===runId)??scoped[0];
  const active=scoped.find(t=>!t.ended_at);
  useEffect(()=>{
    if(flow&&!heldHere)setChoice(previous=>previous?.id===flow.id&&previous.nodeId===nodeId?previous:{id:flow.id,nodeId});
  },[flow?.id,nodeId,heldHere]);
  useEffect(()=>{
    if(active&&nodeId){
      setHeld(previous=>previous?.runId===active.run_id&&previous.nodeId===nodeId?previous:{flow:active.flow,nodeId,runId:active.run_id});
    }else if(heldHere?.runId!=null&&scoped.some(t=>t.run_id===heldHere.runId&&t.ended_at)){
      setHeld(previous=>previous?.runId===heldHere.runId&&previous.nodeId===nodeId?null:previous);
    }
  },[active,heldHere,scoped,nodeId]);
  const close=()=>{setOpen(false);requestAnimationFrame(()=>launcher.current?.focus());};
  useEffect(()=>{setRunId(null);setDetail(null);},[flow?.id,nodeId]);
  useEffect(()=>{if(open&&visible)closer.current?.focus();},[open,visible]);
  useEffect(()=>{
    if(!open||!visible||detail!==null)return;
    const key=(e:KeyboardEvent)=>{if(e.key==="Escape"){e.preventDefault();setOpen(false);requestAnimationFrame(()=>launcher.current?.focus());}};
    document.addEventListener("keydown",key);return()=>document.removeEventListener("keydown",key);
  },[open,visible,detail]);
  const startRun=(manual:boolean):Promise<number>=>{
    if(starting.current)return starting.current;
    if(!flow||!nodeId)return Promise.reject(new Error("请先选择流程和通道"));
    setBusy(true);
    setChoice({id:flow.id,nodeId});setHeld({flow,nodeId,runId:null});
    const task=(async()=>{
      try{const id=await startFlowRun(flow.id,terminalIds,nodeId,manual);setRunId(id);setHeld({flow,nodeId,runId:id});await refresh();return id;}
      catch(e){setHeld(previous=>previous?.flow.id===flow.id&&previous.nodeId===nodeId?null:previous);throw e;}
      finally{starting.current=null;setBusy(false);}
    })();
    starting.current=task;return task;
  };
  const start=async(manual:boolean)=>{try{await startRun(manual);}catch(e){toast.error("无法开始测试",{description:String(e)});}};
  useImperativeHandle(ref,()=>({beforeSend:async(protocolId:string)=>{
    if(!flow||!nodeId)return;
    const first=flow.steps[0];
    if(!first||first.action==="receive"||first.protocol_id!==protocolId)return;
    // Query the runtime, not the 500 ms UI snapshot: a run may just have finished.
    const live=(await getFlowTraces(flow.id)).find(t=>t.node_id===nodeId&&t.flow.id===flow.id&&!t.ended_at);
    if(live){setRunId(live.run_id);return;}
    // start_flow_run establishes the log boundary before returning. The caller must
    // await this before sending, so even an immediate reply belongs to this run.
    await startRun(true);
  }}));
  const stop=async()=>{if(!active)return;setBusy(true);try{await stopFlowRun(active.run_id);await refresh();}catch(e){toast.error(String(e));}finally{setBusy(false);}};
  if(!visible)return null;
  return createPortal(<>
    {!open&&<button ref={launcher} className="flow-drawer-launcher" aria-label="展开流程进度" aria-expanded={false} aria-controls="console-flow-drawer" onClick={()=>setOpen(true)}><ChevronLeft size={14}/><span>流程进度</span>{active&&<span className="flow-drawer-running" title="流程运行中"/>}</button>}
    {open&&<section id="console-flow-drawer" role="dialog" aria-modal="false" aria-label="流程进度" className="flow-drawer">
      <button ref={closer} className="flow-drawer-collapse" aria-label="收起流程进度" aria-expanded={true} aria-controls="console-flow-drawer" onClick={close}><ChevronRight size={16}/></button><header className="flow-drawer-heading"><Workflow size={17}/><h2>流程进度</h2></header>
      <div className="flow-drawer-controls">
        {candidates.length>1&&!heldHere?<select aria-label="控制台测试流程" disabled={busy} className="flow-select" value={flow?.id??0} onChange={e=>setChoice({id:Number(e.target.value),nodeId})}><option value="0" disabled>此信令关联 {candidates.length} 条流程</option>{candidates.map(f=><option key={f.id} value={f.id}>{f.name}</option>)}</select>:flow?<p data-current-flow={flow.id} className="text-sm font-medium break-words">{flow.name}</p>:null}
        {(flow||candidates.length>1)&&<div className="flex items-center gap-2">{active?<Button size="sm" variant="outline" disabled={busy} onClick={stop}><CircleStop/>停止测试</Button>:<><Button size="sm" variant="outline" disabled={!flow||!nodeId||busy} onClick={()=>start(true)}><Eye/>手动跟踪</Button><Button size="sm" variant="outline" disabled={!flow||!nodeId||busy} onClick={()=>start(false)}><Play/>自动运行</Button></>}</div>}
        {flow&&!nodeId&&<p className="text-xs text-muted-foreground">请先在发送区选择协议和通道</p>}
        {flow&&nodeId&&!active&&!trace&&<p className="text-xs text-muted-foreground">{flow.steps[0]?.action==="receive"?"点击手动跟踪，等待对端发送":"发送首步信令时自动开始跟踪"}</p>}
        {trace&&<><select aria-label="流程运行记录" className="flow-select" value={trace.run_id} onChange={e=>{setRunId(Number(e.target.value));setDetail(null);}}>{scoped.map(t=><option key={t.run_id} value={t.run_id}>#{t.run_id} · {new Date(t.started_at).toLocaleTimeString('zh-CN',{hour12:false})} · {t.manual?'手动':'自动'} · {traceStatus[t.status]}</option>)}</select><div className="flex items-center gap-2 text-xs"><span className="min-w-0 flex-1 truncate text-muted-foreground" title={trace.node_name}>{trace.node_name}</span><span className={`flow-status state-${trace.status}`}>{traceStatus[trace.status]}</span></div></>}
        {error&&<p role="alert" className="text-xs text-destructive">{error}</p>}
      </div>
      {flow?<><div className="flow-drawer-subhead"><span>{timelinePhaseCount(trace?.flow??flow)} 个步骤</span><label><input type="checkbox" checked={follow} onChange={e=>setFollow(e.target.checked)}/>跟随当前</label></div><FlowTimeline key={`${flow.id}:${nodeId}:${trace?.run_id??0}`} flow={trace?.flow??flow} trace={trace} protocols={protocols} follow={follow} onPacket={setDetail}/></>:<p className="flow-drawer-empty">{!protocolId?'选择信令后显示对应流程':candidates.length>1?'选择本次测试的流程':'当前信令未关联流程'}</p>}
    </section>}
    {open&&detail!==null&&<DatagramDetail logId={detail} onClose={()=>setDetail(null)}/>}
  </>,document.body);
});
