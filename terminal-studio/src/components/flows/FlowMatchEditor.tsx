import type { FlowStep } from "@/lib/api";
import type { Protocol, ProtocolGroup } from "@/types";
import { protocolFrame } from "@/lib/deviceProfiles";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Plus, X } from "lucide-react";

export function FlowMatchEditor({step,protocols,groups,patch}:{step:FlowStep;protocols:Protocol[];groups:ProtocolGroup[];patch:(p:Partial<FlowStep>)=>void}) {
  const fields=(id?:string|null)=>{const p=protocols.find(p=>p.protocol_id===id);return p?protocolFrame(p,groups.find(g=>g.id===p.group_ref)).fields.filter(f=>!f.unit_fields?.length):[];};
  const request=fields(step.protocol_id);const response=fields(step.action==="receive"?step.protocol_id:step.expect_protocol);
  const pairs=step.correlation??[];
  const selector=(value:string,options:typeof request,onChange:(v:string)=>void,label:string)=><select aria-label={label} value={value} onChange={e=>onChange(e.target.value)} className="h-8 min-w-0 rounded border bg-background px-2 text-xs"><option value="">选择字段</option>{options.map(f=><option key={f.name} value={f.name}>{f.name}</option>)}</select>;
  return <div className="space-y-3 border-y py-3">
    <label className="grid gap-1 text-xs">步骤动作<select aria-label="步骤动作" className="h-8 rounded border bg-background px-2" value={step.action||"send"} onChange={e=>patch({action:e.target.value,...(e.target.value==="receive"?{expect_protocol:null,expect_def:null,correlation:[]}:{} )})}><option value="send">发送信令</option><option value="receive">被动接收信令</option></select></label>
    {step.expect_protocol&&<><div className="flex items-center justify-between text-xs"><span>应答关联</span><Button variant="ghost" size="sm" onClick={()=>patch({correlation:[...pairs,{request:"",response:""}]})}><Plus size={14}/>字段对</Button></div>
      {!pairs.length&&<p className="text-xs text-muted-foreground">自动使用“关联序号”，源/目标设备地址反向对应。</p>}
      {pairs.map((pair,i)=><div key={i} className="grid grid-cols-[1fr_1fr_24px] items-center gap-1">{selector(pair.request,request,v=>patch({correlation:pairs.map((p,j)=>i===j?{...p,request:v}:p)}),`请求关联字段 ${i+1}`)}{selector(pair.response,response,v=>patch({correlation:pairs.map((p,j)=>i===j?{...p,response:v}:p)}),`应答关联字段 ${i+1}`)}<button aria-label={`删除关联字段 ${i+1}`} onClick={()=>patch({correlation:pairs.filter((_,j)=>j!==i)})}><X size={14}/></button></div>)}
      {!pairs.length&&<label className="flex items-start gap-2 text-xs"><input type="checkbox" checked={!!step.allow_unkeyed_reply} onChange={e=>patch({allow_unkeyed_reply:e.target.checked})}/>无关联序号，按通道、对端和协议串行匹配</label>}
    </>}
    {(step.expect_protocol||step.action==="receive")&&<label className="grid gap-1 text-xs">成功判定<select aria-label="成功判定字段" className="h-8 rounded border bg-background px-2" value={step.result_field??""} onChange={e=>patch({result_field:e.target.value})}><option value="">沿用协议“配置结果”字段规则</option>{response.map(f=><option key={f.name} value={f.name}>{f.name}</option>)}</select>{step.result_field?<Input aria-label="步骤成功值" className="h-8 font-mono" placeholder="成功值，如 AA" value={step.success_hex??""} onChange={e=>patch({success_hex:e.target.value})}/>:<span className="text-muted-foreground">结果字段按字典首项判定；无字典按 00；无结果字段仅确认收到。</span>}</label>}
  </div>;
}
