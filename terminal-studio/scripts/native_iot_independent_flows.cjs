// Isolated EXE/database on CDP 9259 and real UDP peer 39172 only.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const fs=require('node:fs');
(async()=>{
 const browser=await chromium.connectOverCDP('http://127.0.0.1:9259');
 const page=browser.contexts()[0].pages()[0];page.setDefaultTimeout(15000);
 const out='artifacts/iot-independent-flows';fs.mkdirSync(out,{recursive:true});
 const invoke=(c,a={})=>page.evaluate(([c,a])=>window.__TAURI_INTERNALS__.invoke(c,a),[c,a]);
 try{
  await page.reload();await page.waitForFunction(()=>!!window.__TAURI_INTERNALS__);
  if(await page.locator('html').evaluate(e=>e.classList.contains('dark')))await page.getByTitle('切换到明亮模式',{exact:true}).click();
  const flows=await invoke('list_flows');
  const ops=[['register','0x01 注册设备'],['heartbeat','0x02 心跳保活'],['configure','0x10 配置采样参数'],['query','0x11 查询设备状态'],['control','0x20 控制继电器'],['logout','0x7F 注销设备']];
  await page.getByText('交互控制台',{exact:true}).first().click();
  await page.getByLabel('系统',{exact:true}).click();
  await page.getByRole('option',{name:'物联网联调协议 v1（自定义）',exact:true}).click();
  const alignment=await page.locator('.console-selectors').evaluate(el=>{
   const boxes=Array.from(el.children).map(e=>({label:e.querySelector('label').getBoundingClientRect().top,input:e.querySelector('button[role="combobox"]').getBoundingClientRect().top,height:e.querySelector('button[role="combobox"]').getBoundingClientRect().height}));
   return boxes;
  });
  for(const box of alignment){assert(Math.abs(box.label-alignment[0].label)<1,'Selector labels aligned');assert(Math.abs(box.input-alignment[0].input)<1,'Selector inputs aligned');assert.equal(box.height,alignment[0].height);}
  await page.getByRole('button',{name:'展开流程进度',exact:true}).click();
  const select=async label=>{await page.getByLabel('协议',{exact:true}).click();await page.getByRole('option',{name:label,exact:true}).click();};
  for(const [op,label] of ops){
   const candidates=flows.filter(f=>f.steps.some(s=>s.protocol_id===`iot_walk_v1_${op}_req`));
   assert.equal(candidates.length,1);const flow=candidates[0];assert.equal(flow.steps.length,1);
   await select(label);
   await page.locator(`[data-current-flow="${flow.id}"]`).waitFor();
   assert.equal(await page.getByLabel('控制台测试流程',{exact:true}).count(),0);
   assert.equal(await page.locator('.flow-timeline-step').count(),2);
   await page.getByText('2 个步骤',{exact:true}).waitFor();
   const baseline=(await invoke('flow_traces',{flowId:flow.id}))[0]?.run_id??0;
   await page.getByRole('button',{name:'发送报文',exact:true}).click();
   const sendNode=page.locator(`.flow-timeline-step[data-step="${op}"][data-phase="send"]`);
   const responseNode=page.locator(`.flow-timeline-step[data-step="${op}"][data-phase="response"]`);
   await responseNode.locator('.flow-status.state-waiting').first().waitFor();
   assert((await sendNode.getAttribute('class')).includes('state-success'),'Send completes before response arrives');
   if(op==='logout')await page.screenshot({path:out+'/logout-waiting.png'});
   let trace;
   for(let i=0;i<100;i++){trace=(await invoke('flow_traces',{flowId:flow.id}))[0];if(trace?.run_id>baseline&&trace?.ended_at)break;await new Promise(r=>setTimeout(r,100));}
   assert(trace?.run_id>baseline,'Current send must create a fresh run');
   assert.equal(trace?.step_states[op]?.status,'success',JSON.stringify(trace));
   const packets=[...new Map(trace.events.filter(e=>e.packet).map(e=>[e.packet.id,e.packet])).values()];
   assert.equal(packets.length,2);assert.deepEqual(packets.map(p=>p.protocol_id).sort(),[`iot_walk_v1_${op}_req`,`iot_walk_v1_${op}_resp`].sort());
   await responseNode.locator('.flow-status.state-success').first().waitFor();
   const responseLabel=label.replace(/^0x[0-9A-F]+/,m=>'0x'+(parseInt(m,16)|0x80).toString(16).toUpperCase())+'响应';
   await select(responseLabel);await page.locator(`[data-current-flow="${flow.id}"]`).waitFor();
   assert.equal(await page.locator('.flow-timeline-step').count(),2,'Response maps to same independent exchange');
   await select(label);
   if(op==='logout'){
    await page.locator('.flow-timeline-summary').nth(0).click();
    await page.locator('.flow-timeline-summary').nth(1).click();
    assert.equal(await page.locator('.flow-packet').count(),2);
    await page.screenshot({path:out+'/logout.png'});
   }
   fs.writeFileSync(`${out}/${op}.json`,JSON.stringify(trace,null,2));
  }
  // A second logout must fail (no registration); do not silently run registration.
  const logout=flows.find(f=>f.steps[0]?.protocol_id==='iot_walk_v1_logout_req');
  const previous=(await invoke('flow_traces',{flowId:logout.id}))[0].run_id;
  await page.getByRole('button',{name:'发送报文',exact:true}).click();
  let failure;
  for(let i=0;i<100;i++){failure=(await invoke('flow_traces',{flowId:logout.id}))[0];if(failure?.run_id!==previous&&failure?.ended_at)break;await new Promise(r=>setTimeout(r,100));}
  assert.equal(failure.step_states.logout.status,'failure');
  assert.equal(Object.keys(failure.step_states).length,1);
  await page.locator('.flow-timeline-step[data-phase="response"].state-failure').waitFor();
  assert((await page.locator('.flow-timeline-step[data-phase="send"]').getAttribute('class')).includes('state-success'));
  await page.screenshot({path:out+'/logout-rejected.png'});
  // Isolated flow only: make its deadline shorter than the peer's response delay.
  try {
   await invoke('upsert_flow',{payload:{...logout,steps:logout.steps.map(s=>({...s,timeout_ms:1000}))}});
   await page.getByRole('button',{name:'发送报文',exact:true}).click();
   let timed;
   for(let i=0;i<60;i++){timed=(await invoke('flow_traces',{flowId:logout.id}))[0];if(timed?.run_id!==failure.run_id&&timed?.ended_at)break;await new Promise(r=>setTimeout(r,100));}
   assert.equal(timed.step_states.logout.status,'timeout');
   await page.locator('.flow-timeline-step[data-phase="response"].state-timeout').waitFor();
   assert((await page.locator('.flow-timeline-step[data-phase="send"]').getAttribute('class')).includes('state-success'));
   await page.screenshot({path:out+'/logout-timeout.png'});
  } finally { await invoke('upsert_flow',{payload:logout}); }
  await page.getByTitle('切换到暗黑模式',{exact:true}).click();
  await page.screenshot({path:out+'/dark.png'});
  await page.setViewportSize({width:1024,height:768});
  assert(await page.locator('.flow-drawer').evaluate(e=>e.scrollWidth<=e.clientWidth+1&&e.getBoundingClientRect().right<=innerWidth));
  await page.screenshot({path:out+'/compact.png'});
  console.log('PASS aligned selectors; six independent two-node exchanges; send complete while response waiting; real UDP success, rejection and timeout; dark and compact layout');
 }catch(e){await page.screenshot({path:out+'/failure.png'});throw e;}
 finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
