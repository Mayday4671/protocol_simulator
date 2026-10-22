// Isolated native EXE/DB + iot_walkthrough_peer.py on 39172. Never use the user's DB.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const fs=require('node:fs');
(async()=>{
 const browser=await chromium.connectOverCDP('http://127.0.0.1:9259');
 const page=browser.contexts()[0].pages()[0];page.setDefaultTimeout(12000);
 const out='artifacts/flow-send-tracking';fs.mkdirSync(out,{recursive:true});
 const invoke=(c,a={})=>page.evaluate(([c,a])=>window.__TAURI_INTERNALS__.invoke(c,a),[c,a]);
 const latest=async()=> (await invoke('flow_traces',{flowId:flow.id}))[0];
 let flow;
 try{
  await page.reload();await page.waitForFunction(()=>!!window.__TAURI_INTERNALS__);
  if(await page.locator('html').evaluate(e=>e.classList.contains('dark')))await page.getByTitle('切换到明亮模式',{exact:true}).click();
  flow=(await invoke('list_flows')).find(f=>f.name==='物联网设备交互 · 注册到注销');assert(flow);
  await page.getByText('交互控制台',{exact:true}).first().click();
  await page.getByLabel('系统',{exact:true}).click();
  await page.getByRole('option',{name:'物联网联调协议 v1（自定义）',exact:true}).click();
  await page.getByLabel('协议',{exact:true}).click();
  await page.getByRole('option',{name:'0x01 注册设备',exact:true}).click();
  // Like the report, no device profile and no explicit manual-tracking button.
  await page.getByLabel('设备',{exact:true}).click();
  await page.getByRole('option',{name:'手动填写',exact:true}).click();
  const initialRuns=await invoke('flow_traces',{flowId:flow.id});
  assert(initialRuns.every(t=>t.ended_at),'Viewing a protocol must not start an observer before sending');
  const baseline=Math.max(0,...initialRuns.map(t=>t.run_id));
  await page.getByRole('button',{name:'展开流程进度',exact:true}).click();
  await page.locator(`[data-current-flow="${flow.id}"]`).waitFor();
  assert.equal(await page.getByLabel('控制台测试流程',{exact:true}).count(),0,'Matching flow is selected automatically');
  await page.getByRole('button',{name:'收起流程进度',exact:true}).click();
  const ops=[['register','0x01 注册设备'],['heartbeat','0x02 心跳保活'],['configure','0x10 配置采样参数'],['query','0x11 查询设备状态'],['control','0x20 控制继电器'],['logout','0x7F 注销设备']];
  let runId;
  for(const [id,label] of ops){
   await page.getByLabel('协议',{exact:true}).click();await page.getByRole('option',{name:label,exact:true}).click();
   await page.getByRole('button',{name:'发送报文',exact:true}).click();
   let trace;
   for(let i=0;i<60;i++){trace=await latest();if(trace?.run_id>baseline&&trace?.step_states[id]?.status==='success')break;await new Promise(r=>setTimeout(r,100));}
   assert(trace,'Selected flow must start observing BEFORE sending, without pressing manual tracking');
   assert(trace.run_id>baseline,'The new send must create a fresh observation, not display old success');
   runId??=trace.run_id;assert.equal(trace.run_id,runId,'Later sends reuse one observation');
   assert.equal(trace.manual,true);assert.equal(trace.step_states[id]?.status,'success',JSON.stringify(trace.events.slice(-3)));
   if(id==='register'){
    await page.getByRole('button',{name:'展开流程进度',exact:true}).click();
    await page.locator('.flow-timeline-step[data-step="register"].state-success').waitFor();
    await page.screenshot({path:out+'/first-send.png'});
    await page.getByRole('button',{name:'收起流程进度',exact:true}).click();
   }
  }
  await page.getByRole('button',{name:'展开流程进度',exact:true}).click();
  await page.waitForFunction(()=>document.querySelectorAll('.flow-timeline-step.state-success').length===6);
  await page.screenshot({path:out+'/complete.png'});
  const completed=await latest();assert.equal(completed.status,'done');
  fs.writeFileSync(out+'/trace.json',JSON.stringify(completed,null,2));
  // A fresh first-step send after completion starts a fresh run; history remains.
  await page.getByRole('button',{name:'收起流程进度',exact:true}).click();
  await page.getByLabel('协议',{exact:true}).click();await page.getByRole('option',{name:ops[0][1],exact:true}).click();
  await page.getByRole('button',{name:'发送报文',exact:true}).click();
  let repeated;
  for(let i=0;i<50;i++){repeated=await latest();if(repeated?.run_id!==runId)break;await new Promise(r=>setTimeout(r,100));}
  assert.notEqual(repeated.run_id,runId);assert.equal(repeated.manual,true);
  await page.getByRole('button',{name:'展开流程进度',exact:true}).click();
  await page.getByRole('button',{name:'停止测试',exact:true}).click();
  console.log('PASS protocol auto-selects flow; first send auto-arms tracking; six real UDP steps; collapsed drawer; no profile; completed run restarts on first send');
 }catch(e){await page.screenshot({path:out+'/failure.png'});fs.writeFileSync(out+'/failure.txt',await page.locator('body').innerText());throw e;}
 finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
