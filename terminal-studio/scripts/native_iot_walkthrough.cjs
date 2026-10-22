// Use only an isolated EXE/DB seeded with seed_iot_walkthrough.py and the local peer.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
const fs=require('node:fs');
(async()=>{
  const browser=await chromium.connectOverCDP('http://127.0.0.1:9259');
  const page=browser.contexts()[0].pages()[0];page.setDefaultTimeout(15000);
  const out='artifacts/iot-walkthrough';fs.mkdirSync(out,{recursive:true});
  const invoke=(c,a={})=>page.evaluate(([c,a])=>window.__TAURI_INTERNALS__.invoke(c,a),[c,a]);
  const errors=[];page.on('pageerror',e=>errors.push(e.message));
  try{
    await page.waitForFunction(()=>!!window.__TAURI_INTERNALS__);
    await page.reload();
    await page.waitForFunction(()=>!!window.__TAURI_INTERNALS__);
    const systems=await invoke('list_systems');
    const system=systems.find(s=>s.name==='物联网联调协议 v1（自定义）');assert(system);
    const flows=await invoke('list_flows');const flow=flows.find(f=>f.system_ref===system.id);assert(flow);
    await page.getByText('交互控制台',{exact:true}).first().click();
    await page.getByLabel('系统',{exact:true}).click();
    await page.getByRole('option',{name:system.name,exact:true}).click();
    await page.getByLabel('协议',{exact:true}).click();
    await page.getByRole('option',{name:'0x01 注册设备',exact:true}).click();
    await page.getByLabel('设备',{exact:true}).click();
    await page.getByRole('option',{name:/环境采集终端 01/}).click();
    const opener=page.getByRole('button',{name:'展开流程进度',exact:true});
    await opener.waitFor({state:'visible'});
    assert.equal(await opener.locator('svg').count(),1);
    await page.screenshot({path:out+'/collapsed.png'});
    await opener.click();
    await page.locator(`[data-current-flow="${flow.id}"]`).waitFor();
    assert.equal(await page.getByLabel('控制台测试流程',{exact:true}).count(),0,'Protocol determines its only flow');
    await page.getByRole('button',{name:'自动运行',exact:true}).click();
    await page.waitForFunction(()=>document.querySelectorAll('.flow-timeline-step.state-waiting,.flow-timeline-step.state-delay').length>0);
    await page.screenshot({path:out+'/running.png'});
    let trace;
    for(let i=0;i<250;i++){
      trace=(await invoke('flow_traces',{flowId:flow.id}))[0];
      if(trace?.ended_at)break;
      await new Promise(r=>setTimeout(r,100));
    }
    fs.writeFileSync(out+'/trace.json',JSON.stringify(trace,null,2));
    assert.equal(trace?.status,'done',JSON.stringify(trace?.events?.slice(-4)));
    assert.equal(Object.values(trace.step_states).filter(s=>s.status==='success').length,6);
    const packets=trace.events.filter(e=>e.packet).map(e=>e.packet);
    const unique=[...new Map(packets.map(p=>[p.id,p])).values()];
    assert.equal(unique.length,12,'Six real UDP requests and replies');
    const token=Buffer.from(unique.find(p=>p.protocol_id==='iot_walk_v1_register_resp').raw_hex.replace(/\s/g,''),'hex').readUInt16BE(10);
    assert.notEqual(token,0);
    for(const op of ['heartbeat','configure','query','control','logout']){
      const req=Buffer.from(unique.find(p=>p.protocol_id===`iot_walk_v1_${op}_req`).raw_hex.replace(/\s/g,''),'hex');
      const res=Buffer.from(unique.find(p=>p.protocol_id===`iot_walk_v1_${op}_resp`).raw_hex.replace(/\s/g,''),'hex');
      assert.equal(req.readUInt16BE(10),token,'Registered session reused');
      assert.equal(res.readUInt16BE(8),req.readUInt16BE(8),'Response echoes transaction');
      assert.equal(res.readUInt16BE(4),req.readUInt16BE(6));
      assert.equal(res[14],0);
    }
    await page.waitForFunction(()=>document.querySelectorAll('.flow-timeline-step.state-success').length===6);
    await page.screenshot({path:out+'/completed.png'});
    await page.getByRole('button',{name:/注册设备.*完成/}).click();
    await page.locator('.flow-drawer .flow-packet').last().click();
    await page.getByText('报文解析',{exact:true}).waitFor();
    await page.screenshot({path:out+'/parsed-response.png'});
    await page.keyboard.press('Escape');
    // The same saved flow must also observe normal sends from the console.
    await page.getByRole('button',{name:'手动跟踪',exact:true}).click();
    const operations=[['register','0x01 注册设备'],['heartbeat','0x02 心跳保活'],['configure','0x10 配置采样参数'],['query','0x11 查询设备状态'],['control','0x20 控制继电器'],['logout','0x7F 注销设备']];
    for(const [id,label] of operations){
      await page.getByRole('button',{name:'收起流程进度',exact:true}).click();
      await page.getByLabel('协议',{exact:true}).click();
      await page.getByRole('option',{name:label,exact:true}).click();
      await page.getByRole('button',{name:'发送报文',exact:true}).click();
      let observed;
      for(let i=0;i<70;i++){
        observed=(await invoke('flow_traces',{flowId:flow.id}))[0];
        if(observed?.step_states[id]?.status==='success')break;
        await new Promise(r=>setTimeout(r,100));
      }
      assert.equal(observed?.step_states[id]?.status,'success',JSON.stringify(observed?.events?.slice(-3)));
      await page.getByRole('button',{name:'展开流程进度',exact:true}).click();
    }
    await page.waitForFunction(()=>document.querySelectorAll('.flow-timeline-step.state-success').length===6);
    await page.screenshot({path:out+'/manual-completed.png'});
    assert.deepEqual(errors,[]);
    fs.writeFileSync(out+'/protocols.xml',await invoke('export_protocols_xml_text',{protocolIds:flow.steps.flatMap(s=>[s.protocol_id,s.expect_protocol])}));
    console.log('PASS native UI: six automatic + six manual steps, real UDP, token capture/reuse, response correlation, timeline and response parsing');
  }catch(e){await page.screenshot({path:out+'/failure.png'});fs.writeFileSync(out+'/failure.txt',await page.locator('body').innerText());throw e;}
  finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
