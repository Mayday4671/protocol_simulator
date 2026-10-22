const {chromium}=require('playwright');
const assert=require('node:assert/strict'),fs=require('node:fs');
(async()=>{
 const browser=await chromium.launch({channel:'chrome',headless:true});
 const page=await browser.newPage({viewport:{width:1400,height:900}});page.setDefaultTimeout(7000);
 const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.route('**/__console_devices',r=>r.fulfill({contentType:'text/html',body:`<!doctype html><html><head><meta charset="utf-8"></head><body><div id="root"></div><script type="module">
 import R from '/@react-refresh';R.injectIntoGlobalHook(window);window.$RefreshReg$=()=>{};window.$RefreshSig$=()=>type=>type;window.__vite_plugin_react_preamble_installed__=true;
 const {default:React}=await import('/node_modules/.vite/deps/react.js');const {default:ReactDOM}=await import('/node_modules/.vite/deps/react-dom_client.js');
 const f=(name,role,profile_key,enc='hex')=>({name,role,profile_key,enc,len:2,value:''});
 const head=[f('主叫','src_id'),f('被叫','dst_id'),{...f('凭据','session'),value:'{session}'}];
 const body=[f('资产编号','device_sn','serial'),f('采集频率','none','rate'),f('独立字段','none')];
 const p={id:1,protocol_id:'req',name:'查询',system_ref:1,group_ref:1,def_mode:'fields',msg_type:'Request',fields_json:JSON.stringify(body),auto_respond:null};
 window.sent=[];window.__TAURI_INTERNALS__={transformCallback:()=>1,unregisterCallback:()=>{},invoke:async(c,a)=>{
   if(c==='list_systems')return [{id:1,name:'系统甲'},{id:2,name:'系统乙'}];
   if(c==='list_groups')return [{id:1,name:'组',system_ref:1,header_enabled:true,header_fields_json:JSON.stringify(head)}];
   if(c==='list_protocols')return [p,{...p,id:2,protocol_id:'archive',system_ref:2,name:'档案定义',group_ref:null,fields_json:JSON.stringify(body.map(f=>f.profile_key==='rate'?{...f,enc:'uint'}:f))}];
   if(c==='list_nodes')return [{id:1,name:'通道甲',group_bind_json:'[1]',remote_ip:'127.0.0.1',remote_port:9999}];
   if(c==='list_terminals')return [
     {id:1,name:'终端甲',node_ref:1,system_ref:1,protocol_ref:'req',device_key:'0012',fields_json:'{"serial":"1234","rate":"0014"}'},
     {id:2,name:'终端乙',node_ref:2,system_ref:2,protocol_ref:'archive',device_key:'0012',fields_json:'{"serial":"5678","rate":"30"}'},
     {id:3,name:'超宽终端',node_ref:1,system_ref:1,protocol_ref:'req',device_key:'123456',fields_json:'{}'}];
   if(c==='engine_status')return {listening_nodes:[1],periodic_sends:[],heartbeats:[]};
   if(c==='log_stats')return {today_in:0,today_out:0,total:0};
   if(c==='send_udp_message'){window.sent.push(a);return {bytes:10,hex:'0012'};}
   return [];
 }};
 await import('/src/styles/globals.css');const {SimulationConsole}=await import('/src/pages/SimulationConsole.tsx');const {LogProvider}=await import('/src/hooks/useUdpLogs.tsx');
 ReactDOM.createRoot(document.getElementById('root')).render(React.createElement(LogProvider,null,React.createElement(SimulationConsole)));
 </script></body></html>`}));
 try{
  await page.goto('http://127.0.0.1:1426/__console_devices');
  await page.getByLabel('协议',{exact:true}).click();await page.getByRole('option',{name:'查询',exact:true}).click();
  assert.equal(await page.locator('#console-设备').count(),0);
  assert.equal(await page.locator('.console-selectors > div').count(),4);
  await page.getByRole('button',{name:/^公共头/}).click();
  const input=name=>page.locator('[id="f-'+name+'"]');
  const choose=async(name,label)=>{await input(name).locator('..').getByRole('button',{name:'选择建议值'}).click();await page.locator('[role=dialog][data-state=open]').getByRole('option',{name:label,exact:true}).click();await page.locator('[role=dialog]').waitFor({state:'hidden'});};
  await choose('主叫','终端甲 · 0012 · 系统甲');
  assert.equal(await input('资产编号').inputValue(),'1234');assert.equal(await input('采集频率').inputValue(),'0014');
  await choose('主叫','终端乙 · 0012 · 系统乙'); // Same address, distinct profile: must not select the first by value.
  assert.equal(await input('资产编号').inputValue(),'5678');assert.equal(await input('采集频率').inputValue(),'001E');
  assert.equal(await input('凭据').inputValue(),'');
  await choose('被叫','终端甲 · 0012 · 系统甲');
  assert.equal(await input('资产编号').inputValue(),'5678');assert.equal(await input('主叫').inputValue(),'0012');
  await choose('资产编号','终端甲 · 1234 · 系统甲');
  assert.equal(await input('采集频率').inputValue(),'0014');
  await input('主叫').fill('000F');await page.keyboard.press('Escape');
  await input('采集频率').fill('002A');await page.keyboard.press('Escape');
  assert.equal(await input('主叫').getAttribute('readonly'),null);
  await page.getByRole('button',{name:'发送报文',exact:true}).click();
  await page.waitForFunction(()=>window.sent.length===1);
  const sent=await page.evaluate(()=>window.sent[0].params);
  assert.deepEqual(sent,{'主叫':'000F','被叫':'0012','资产编号':'1234','采集频率':'002A'});
  await input('主叫').click();assert.equal(await page.getByRole('option',{name:/超宽终端/}).count(),0);await page.keyboard.press('Escape');
  fs.mkdirSync('artifacts/console-devices',{recursive:true});await page.screenshot({path:'artifacts/console-devices/light.png'});
  await page.evaluate(()=>document.documentElement.classList.add('dark'));await page.setViewportSize({width:1024,height:768});
  assert(await page.locator('.console-selectors').evaluate(e=>e.scrollWidth<=e.clientWidth));
  await page.screenshot({path:'artifacts/console-devices/dark-compact.png'});
  assert.deepEqual(errors,[]);
  console.log('PASS four selectors; header/payload profile choice, all systems, duplicate addresses, auto-fill, destination isolation, editable values and exact send parameters');
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
