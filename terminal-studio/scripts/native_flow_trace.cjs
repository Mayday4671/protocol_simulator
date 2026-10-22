// Isolated native EXE with SIMTERM_DB + SIMTERM_UI_TEST_PORT=9259; never use the user's DB.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const dgram = require('node:dgram');
const fs = require('node:fs');
(async()=>{
  const peer=dgram.createSocket('udp4');await new Promise(r=>peer.bind(0,'127.0.0.1',r));
  const reserve=async()=>{const s=dgram.createSocket('udp4');await new Promise(r=>s.bind(0,'127.0.0.1',r));const p=s.address().port;await new Promise(r=>s.close(r));return p;};
  const wires=[];
  peer.on('message',(b,src)=>{wires.push(b.toString('hex'));
    if(b[0]===0x21){peer.send(Buffer.from([0x22,0xFE,0xAA]),src.port,src.address);peer.send(Buffer.from([0x22,b[1],0xAA]),src.port,src.address);peer.send(Buffer.from([0x41,b[1]]),src.port,src.address);}
    if(b[0]===0x31)peer.send(Buffer.from([0x32,b[1],0xAA]),src.port,src.address);
  });
  const browser=await chromium.connectOverCDP('http://127.0.0.1:9259');const page=browser.contexts()[0].pages()[0];page.setDefaultTimeout(15000);
  const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.waitForFunction(()=>!!window.__TAURI_INTERNALS__);
  const invoke=(c,a={})=>page.evaluate(([c,a])=>window.__TAURI_INTERNALS__.invoke(c,a),[c,a]);
  const out='artifacts/flow-trace';fs.mkdirSync(out,{recursive:true});
  const result={wires};
  const stableLabels=async()=>{
    await page.locator('.flow-test-view .react-flow__edge-text').first().waitFor();
    const stable=await page.evaluate(async()=>{
      const root=document.querySelector('.flow-test-view .react-flow');
      const labels=[...root.querySelectorAll('.react-flow__edge-text')];
      let removed=0;
      const observer=new MutationObserver(ms=>{for(const m of ms)for(const n of m.removedNodes)if(n.nodeType===1&&(n.matches('.react-flow__edge-text')||n.querySelector('.react-flow__edge-text')))removed++;});
      observer.observe(root,{subtree:true,childList:true});
      await new Promise(r=>setTimeout(r,1600));observer.disconnect();
      return removed===0&&labels.every(e=>e.isConnected);
    });
    assert(stable,'Flow edge labels must survive timer and trace refresh');
  };
  try{
    if(await page.locator('html').evaluate(e=>e.classList.contains('dark')))await page.getByTitle('切换到明亮模式',{exact:true}).click();
    const systems=await invoke('list_systems');assert.equal(systems.length,0,'Requires fresh isolated database');
    const system=await invoke('create_system',{payload:{name:'流程联调验收',description:''}});
    const group=await invoke('create_group',{payload:{name:'测试信令',system_ref:system.id,header_enabled:false,header_fields:[]}});
    const field=(name,len,value='',role='none',section='payload')=>({name,len,enc:'hex',value,role,section});
    for(const [id,name,type,code] of [['q','设备配置请求','Request','21'],['a','设备配置响应','Response','22'],['n','配置生效通知','Event','41'],['h','心跳请求','Request','31'],['ha','心跳响应','Response','32']]){
      const fields=[field('信令码',1,code,'type_code','header'),field('流水号',1,'01','correlation','header')];
      if(type==='Response')fields.push(field('结果',1,'AA'));
      await invoke('upsert_protocol',{payload:{protocol_id:id,name,msg_type:type,format:'hex',def_mode:'fields',system_ref:system.id,group_ref:group.id,fields}});
    }
    const nodes=[];for(const name of ['通道 A','通道 B'])nodes.push(await invoke('create_node',{payload:{name,role:'simulator',local_ip:'127.0.0.1',local_port:await reserve(),remote_ip:'127.0.0.1',remote_port:peer.address().port,node_id_hex:null,enabled:true,group_binds:[group.id]}}));
    const step=(id,name,p,ack,next,x=100,y=80)=>({id,name,x,y,protocol_id:p,expect_protocol:ack,next:null,on_success:next,params:{},timeout_ms:4000,delay_ms:0,terminal_mode:'single',result_field:ack?'结果':'',success_hex:ack?'AA':''});
    const flow=await invoke('upsert_flow',{payload:{id:0,name:'配置 → 生效通知 → 心跳',system_ref:system.id,group_ref:group.id,description:'',steps:[step('s1','配置设备','q','a','s2'),{...step('s2','接收生效通知','n',null,'s3',100,280),action:'receive'},step('s3','心跳确认','h','ha',null,100,480)],trigger_protocols:[],schedule_interval_ms:0}});
    const manual=await invoke('upsert_flow',{payload:{...flow,id:0,name:'手动配置跟踪',steps:[step('s1','配置设备','q','a',null)]}});
    await page.getByText('业务流程',{exact:true}).first().click();
    await page.getByRole('button',{name:/配置 → 生效通知 → 心跳/}).click();
    await page.getByRole('button',{name:'测试流程',exact:true}).click();
    await page.getByLabel('测试通道',{exact:true}).selectOption(String(nodes[1].id));
    await stableLabels();
    await page.getByRole('button',{name:'自动运行',exact:true}).click();
    const end=async id=>{for(let i=0;i<100;i++){const trace=(await invoke('flow_traces',{flowId:id}))[0];if(trace?.ended_at)return trace;await new Promise(r=>setTimeout(r,80));}throw Error('Run did not finish');};
    const auto=await end(flow.id);assert.equal(auto.status,'done');assert.equal(auto.events.filter(e=>e.status==='success').length,3);assert(auto.events.some(e=>e.status==='unmatched'&&e.note==='关联字段不符'));result.auto=auto;
    await page.waitForFunction(()=>document.querySelectorAll('.flow-test-node.state-success').length===3);
    await stableLabels();
    await page.screenshot({path:out+'/native-progress.png'});
    await page.locator('.react-flow__node').filter({hasText:'配置设备'}).click();
    await page.locator('.flow-packet').last().click();
    await page.getByText('报文解析',{exact:true}).waitFor();
    await page.screenshot({path:out+'/native-parse.png'});
    await page.keyboard.press('Escape');
    await page.getByRole('button',{name:/交互时序/}).click();
    await page.locator('.sequence-message').first().waitFor();await page.screenshot({path:out+'/native-sequence.png'});
    // Native stop and re-entry: runtime B must not be displayed as default channel A.
    await page.getByRole('button',{name:'手动跟踪',exact:true}).click();
    await page.getByRole('button',{name:'返回',exact:true}).click();
    await page.getByRole('button',{name:'测试流程',exact:true}).click();
    await page.waitForFunction(id=>document.querySelector('[aria-label="测试通道"]')?.value===String(id),nodes[1].id);
    assert(await page.getByLabel('测试通道',{exact:true}).isDisabled());
    await page.getByRole('button',{name:'停止测试',exact:true}).click();
    await page.getByRole('button',{name:'返回',exact:true}).click();
    // Manual sends are observed while the user stays in the console.
    await page.getByText('交互控制台',{exact:true}).first().click();
    // Select the request using the existing console protocol selector.
    const protocol=page.getByLabel('协议',{exact:true});
    if(await protocol.count()){await protocol.click();await page.getByRole('option',{name:'设备配置请求',exact:true}).click();}
    assert.equal(await page.getByRole('dialog',{name:'流程进度',exact:true}).count(),0);
    await page.getByRole('button',{name:'展开流程进度',exact:true}).click();
    await page.getByLabel('控制台测试流程',{exact:true}).selectOption(String(manual.id));
    await page.getByRole('button',{name:'手动跟踪',exact:true}).click();
    const before=wires.length;await new Promise(r=>setTimeout(r,100));assert.equal(wires.length,before,'Observer must not send');
    await page.getByRole('button',{name:'收起流程进度',exact:true}).click();
    await page.getByRole('button',{name:'发送报文',exact:true}).click();
    const observed=await end(manual.id);assert.equal(observed.status,'done');result.manual=observed;
    await page.getByRole('button',{name:'展开流程进度',exact:true}).click();
    await page.locator('.flow-timeline-step.state-success').waitFor();
    await page.screenshot({path:out+'/native-console.png'});
    await page.getByRole('button',{name:/配置设备.*完成/}).click();
    await page.locator('.flow-drawer .flow-packet').last().click();
    await page.getByText('报文解析',{exact:true}).waitFor();
    await page.keyboard.press('Escape');
    assert.equal(await page.getByRole('dialog',{name:'流程进度',exact:true}).count(),1,'Closing parse must preserve drawer');
    await page.getByRole('button',{name:'收起流程进度',exact:true}).click();
    await page.getByRole('button',{name:'展开流程进度',exact:true}).click();
    await page.getByLabel('控制台测试流程',{exact:true}).selectOption(String(flow.id));
    await page.getByRole('button',{name:'自动运行',exact:true}).click();
    await page.waitForFunction(()=>document.querySelectorAll('.flow-timeline-step.state-success').length===3);
    await page.screenshot({path:out+'/native-console-detail.png'});
    await page.getByText('业务流程',{exact:true}).first().click();
    assert.equal(await page.getByRole('dialog',{name:'流程进度',exact:true}).count(),0,'Hidden console must hide its drawer');
    await page.getByText('交互控制台',{exact:true}).first().click();
    await page.getByRole('dialog',{name:'流程进度',exact:true}).waitFor();
    await page.getByTitle('切换到暗黑模式',{exact:true}).click();
    await page.waitForTimeout(250);
    await page.screenshot({path:out+'/native-dark.png'});
    await page.setViewportSize({width:1024,height:768});
    await page.waitForFunction(()=>document.querySelector('.flow-drawer')?.clientWidth>0);
    assert(await page.locator('.flow-drawer').evaluate(e=>e.scrollWidth<=e.clientWidth+1&&e.getBoundingClientRect().right<=innerWidth));
    await page.screenshot({path:out+'/native-compact.png'});
    assert.deepEqual(errors,[]);result.errors=errors;fs.writeFileSync(out+'/result.json',JSON.stringify(result,null,2));console.log('PASS native flow progress, actual UDP, sequence, parse, stop/re-entry and console manual tracking');
  }catch(e){await page.screenshot({path:out+'/failure.png'});fs.writeFileSync(out+'/failure.txt',await page.locator('body').innerText());throw e;}
  finally{await browser.close();peer.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
