import { useEffect, useRef, useState } from "react";
import { Check, ChevronDown, Circle, Clock3, X } from "lucide-react";
import type { FlowRow, FlowTrace } from "@/lib/api";
import type { Protocol } from "@/types";
import { traceStatus } from "./FlowTestView";
import { cn } from "@/lib/utils";
import { timelinePhases } from "./timelinePhases";

const working=["running","waiting","waiting_send","delay"];
const clock=(at:number)=>new Date(at).toLocaleTimeString("zh-CN",{hour12:false});
export function FlowTimeline({flow,trace,protocols,follow,onPacket}:{flow:FlowRow;trace?:FlowTrace;protocols:Protocol[];follow:boolean;onPacket:(id:number)=>void}){
  const [expanded,setExpanded]=useState<Set<string>>(()=>new Set());
  const list=useRef<HTMLOListElement>(null);
  const phases=timelinePhases(flow,trace);
  const current=phases.filter(p=>working.includes(p.state)).sort((a,b)=>(b.last?.at??0)-(a.last?.at??0))[0]?.key;
  useEffect(()=>{
    if(!follow||!current||trace?.ended_at)return;
    const container=list.current;
    const item=Array.from(container?.children??[]).find(e=>e.getAttribute("data-phase-key")===current) as HTMLElement|undefined;
    if(container&&item){const delta=item.getBoundingClientRect().top-container.getBoundingClientRect().top;container.scrollTop+=delta-16;}
  },[current,follow,trace?.ended_at]);
  const name=(id?:string|null)=>protocols.find(p=>p.protocol_id===id)?.name??id??"";
  return <ol ref={list} className="flow-timeline" aria-label="流程步骤时间轴">
    {phases.map(s=>{
      const {last,state,events}=s;
      const Icon=state==='success'?Check:working.includes(state)||state==='timeout'?Clock3:['failure','error','aborted'].includes(state)?X:Circle;
      const isOpen=expanded.has(s.key);
      const title=(name(s.protocolId)||s.name).replace(/^0x[0-9a-f]+\s*/i,"");
      const action=s.phase==='send'?'发送':s.phase==='response'?'等待':'接收';
      const status=state==='success'&&s.phase==='send'?'已发送':traceStatus[state]??state;
      return <li key={s.key} data-step={s.stepId} data-phase={s.phase} data-phase-key={s.key} className={cn("flow-timeline-step",`state-${state}`)} aria-current={current===s.key&&!trace?.ended_at?"step":undefined}>
        <span className={cn("flow-timeline-dot","flow-status",`state-${state}`)}><Icon size={15}/></span>
        <button className="flow-timeline-summary" aria-expanded={isOpen} onClick={()=>setExpanded(previous=>{const next=new Set(previous);next.has(s.key)?next.delete(s.key):next.add(s.key);return next;})}>
          <span className="flow-timeline-title"><strong>{action}{title}{s.phase==='send'&&!title.endsWith('请求')&&flow.steps.find(step=>step.id===s.stepId)?.expect_protocol?'请求':''}</strong><span className={cn("flow-status",`state-${state}`)}>{status}</span><ChevronDown size={13} className={isOpen?'rotate-180':''}/></span>
          <span className="flow-timeline-protocol">{name(s.protocolId)}</span>
          {last&&<span className="flow-timeline-meta"><time>{clock(last.at)}</time>{last.attempt>0&&<span>执行 #{last.attempt}</span>}</span>}
        </button>
        {isOpen&&<div className="flow-timeline-events">{!events.length&&<p className="text-xs text-muted-foreground">尚无执行记录</p>}{events.map(e=><div key={e.index} className="flow-timeline-event"><div className="flex items-center gap-2 text-xs"><time className="text-muted-foreground">{clock(e.at)}</time><span className={cn("flow-status",`state-${e.status}`)}>{traceStatus[e.status]??e.status}</span>{e.attempt>0&&<span className="ml-auto text-muted-foreground">#{e.attempt}</span>}</div><p className="mt-1 text-xs text-muted-foreground break-words">{e.terminal?`设备 ${e.terminal} · `:''}{e.note}</p>{e.packet&&<button className="flow-packet" onClick={()=>onPacket(e.packet!.id)}><span className="flex items-start justify-between gap-2 text-xs"><b>{e.packet.direction==='out'?'发 →':'← 收'} {name(e.packet.protocol_id)||'未识别信令'}</b><span className="shrink-0">解析</span></span><code>{e.packet.raw_hex}</code></button>}</div>)}</div>}
      </li>;
    })}
    {trace?.omitted? <li className="text-xs text-muted-foreground">较早的 {trace.omitted} 条事件已清理</li>:null}
  </ol>;
}
