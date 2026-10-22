// Isolated desktop EXE / test DB, local UDP 39171 -> 39172. No production data.
const {chromium}=require('playwright');
const assert=require('node:assert/strict'),fs=require('node:fs');
(async()=>{
 const browser=await chromium.connectOverCDP('http://127.0.0.1:9259');
 const page=browser.contexts()[0].pages()[0];page.setDefaultTimeout(10000);
 const invoke=(c,a={})=>page.evaluate(([c,a])=>window.__TAURI_INTERNALS__.invoke(c,a),[c,a]);
 let original;
 try{
  await page.reload();await page.waitForFunction(()=>!!window.__TAURI_INTERNALS__);
  original=(await invoke('list_terminals')).find(t=>t.system_ref===4&&t.device_key==='0001');assert(original);
  await invoke('save_terminal',{payload:{...original,fields_json:JSON.stringify({serial:'A1B2C3D4'})}});
  await page.getByText('交互控制台',{exact:true}).first().click();
  await page.getByLabel('系统',{exact:true}).click();await page.getByRole('option',{name:'物联网联调协议 v1（自定义）',exact:true}).click();
  assert.equal(await page.locator('#console-设备').count(),0);assert.equal(await page.locator('.console-selectors > div').count(),4);
  const select=async name=>{await page.getByLabel('协议',{exact:true}).click();await page.getByRole('option',{name,exact:true}).click();};
  const input=name=>page.locator('[id="f-'+name+'"]');
  const choose=async(name,value)=>{
   await input(name).locator('..').getByRole('button',{name:'选择建议值'}).click();
   await page.locator('[role=dialog][data-state=open]').getByRole('option',{name:new RegExp('环境采集终端 01 · '+value)}).click();
   await page.locator('[role=dialog]').waitFor({state:'hidden'});
  };
  await select('0x01 注册设备');await page.getByRole('button',{name:/^公共头/}).click();
  await choose('源设备','0001');assert.equal(await input('设备序列号').inputValue(),'A1B2C3D4');
  await choose('目标设备','0001');assert.equal(await input('源设备').inputValue(),'0001');assert.equal(await input('设备序列号').inputValue(),'A1B2C3D4');
  await input('目标设备').fill('0000');await page.keyboard.press('Escape');
  await choose('设备序列号','A1B2C3D4');
  await input('设备序列号').fill('11223344');await page.keyboard.press('Escape');
  const flows=await invoke('list_flows');
  const exchange=async op=>{
   const flow=flows.find(f=>f.steps[0]?.protocol_id==='iot_walk_v1_'+op+'_req');
   const baseline=(await invoke('flow_traces',{flowId:flow.id}))[0]?.run_id??0;
   await page.getByRole('button',{name:'发送报文',exact:true}).click();
   let trace;
   for(let i=0;i<100;i++){trace=(await invoke('flow_traces',{flowId:flow.id}))[0];if(trace?.run_id>baseline&&trace.ended_at)break;await new Promise(r=>setTimeout(r,100));}
   assert(trace?.run_id>baseline);assert.equal(trace.step_states[op].status,'success',JSON.stringify(trace));
   return [...new Map(trace.events.filter(e=>e.packet).map(e=>[e.packet.id,e.packet])).values()];
  };
  const packets=await exchange('register');assert.equal(packets.length,2);
  const sent=packets.find(p=>p.direction==='out');
  const hex=(sent.raw_hex||'').replace(/\s/g,'').toUpperCase();
  assert(hex.endsWith('11223344'),JSON.stringify(sent));assert.equal(hex.slice(8,12),'0001');assert.equal(hex.slice(12,16),'0000');
  await select('0x11 查询设备状态');await choose('源设备','0001');
  const query=await exchange('query');assert.equal(query.length,2,'Session capture must still follow the actual selected source');
  await page.getByRole('button',{name:'展开流程进度',exact:true}).click();
  fs.mkdirSync('artifacts/console-devices',{recursive:true});
  fs.writeFileSync('artifacts/console-devices/native-exchanges.json',JSON.stringify({packets,query},null,2));
  await page.screenshot({path:'artifacts/console-devices/native.png'});
  console.log('PASS native EXE: four selectors, real header/payload profile autofill, editable payload bytes, independent destination, UDP registration + query response, correct session');
 }finally{if(original)await invoke('save_terminal',{payload:original});await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
