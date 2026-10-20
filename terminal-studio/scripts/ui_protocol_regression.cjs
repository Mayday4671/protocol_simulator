// Frontend interaction regression with isolated IPC fixtures; pair with e2e_isolation for real UDP.
const {chromium}=require('playwright');
const assert=require('node:assert/strict');
(async()=>{
  const browser=await chromium.launch({headless:true,channel:'chrome'});
  const page=await browser.newPage({viewport:{width:1440,height:1000}});
  const errors=[];page.on('pageerror',e=>errors.push(String(e)));
  await page.addInitScript(()=>{
    const f=(name,len,enc,value,role='none',extra={})=>({name,len,enc,value,role,...extra});
    const fields=[f('模式',1,'uint','1'),f('条目数',1,'uint','2','device_count'),f('测点',1,'hex','{vlen}','device_list',{unit_fields:[f('温度',2,'uint','300'),f('标签',3,'ascii','Ab')],when:{field:'模式',value:'1'}}),f('扩展数',1,'hex','01','device_count'),f('扩展',1,'hex','{vlen}','device_list',{unit_fields:[f('状态',1,'hex','AA')]}),f('源',1,'hex','{node_id}','src_id'),f('会话',2,'hex','{session}','session')];
    const protocol={id:1,protocol_id:'test',name:'混合分支协议',def_mode:'fields',format:'hex',msg_type:'Request',fields_json:JSON.stringify(fields),system_ref:1,group_ref:1,system_name:'测试系统',group_name:'设备配置',respond_delay_ms:0,auto_periodic_interval_ms:1000};
    const profiles=[{id:1,name:'终端A',node_ref:1,system_ref:1,protocol_ref:'test',device_key:'01',fields_json:'{}',session_hex:'A1A2'},{id:2,name:'终端B',node_ref:1,system_ref:1,protocol_ref:'test',device_key:'02',fields_json:'{}',session_hex:'B1B2'}];
    window.__testCalls=[];
    window.__TAURI_INTERNALS__={transformCallback:()=>1,unregisterCallback:()=>{},invoke:async(cmd,args)=>{
      window.__testCalls.push({cmd,args});
      if(cmd==='list_protocols')return [protocol];
      if(cmd==='list_nodes')return [{id:1,name:'测试通道',role:'simulator',local_ip:'127.0.0.1',local_port:19001,remote_ip:'127.0.0.1',remote_port:19002,node_id_hex:'01',group_bind_json:'[1]',enabled:true}];
      if(cmd==='list_systems')return [{id:1,name:'测试系统'}];
      if(cmd==='list_groups')return [{id:1,system_ref:1,name:'设备配置',header_enabled:false}];
      if(cmd==='list_terminals')return profiles;
      if(cmd==='save_terminal'){profiles.push({...args.payload,id:3});return null;}
      if(cmd==='engine_status')return {listening_nodes:[],heartbeats:[],periodic_sends:[]};
      if(cmd==='log_stats')return {total:0,in_count:0,out_count:0,error_count:0};
      if(cmd==='send_udp_message')return {bytes:1,hex:'AA'};
      if(cmd==='list_dict_sets'||cmd==='query_logs'||cmd==='list_heartbeats')return [];
      return null;
    }};
    window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:()=>{}};
  });
  try {
    await page.goto('http://127.0.0.1:1422/console');
    await page.getByRole('combobox').nth(1).click();await page.getByRole('option',{name:'设备配置',exact:true}).click();
    await page.getByRole('combobox').nth(2).click();await page.getByRole('option',{name:'混合分支协议',exact:true}).click();
    await page.getByLabel('测点 第1条 温度').waitFor();
    assert.equal(await page.getByLabel('测点 第1条 温度').inputValue(),'300');
    assert.equal(await page.getByLabel('测点 第1条 标签').inputValue(),'Ab');
    assert.equal(await page.getByLabel('扩展 第1条 状态').inputValue(),'AA');
    await page.getByLabel('当前终端').selectOption('1');
    await page.getByRole('button',{name:'发送报文'}).click();
    let call=await page.evaluate(()=>window.__testCalls.filter(c=>c.cmd==='send_udp_message').at(-1));
    assert.equal(call.args.params.__terminal_id,'1');
    assert.equal(call.args.params['测点'],'012C416200012C416200');
    assert.equal(call.args.params['扩展'],'AA');
    await page.getByLabel('当前终端').selectOption('2');await page.getByText('Session：B1B2',{exact:true}).waitFor();
    await page.getByRole('button',{name:'发送报文'}).click();
    call=await page.evaluate(()=>window.__testCalls.filter(c=>c.cmd==='send_udp_message').at(-1));assert.equal(call.args.params.__terminal_id,'2');
    await page.locator('[id="f-条目数"]').fill('0');await page.getByLabel('测点 第1条 温度').waitFor({state:'detached'});
    await page.locator('[id="f-模式"]').fill('2');await page.getByRole('button',{name:/重复分组：测点/}).waitFor({state:'detached'});
    await page.screenshot({path:'ui-protocol-regression.png',fullPage:true});
    await page.getByRole('link',{name:'设备管理',exact:true}).click();await page.getByRole('button',{name:'新建终端资料'}).click();
    await page.getByLabel('资料来源协议').selectOption('test');await page.getByText('条目数 · uint · 1B',{exact:true}).waitFor();
    await page.getByRole('link',{name:'协议管理',exact:true}).click();
    await page.getByTitle('编辑',{exact:true}).click();
    await page.getByRole('button',{name:'收起分支组：模式'}).waitFor();
    await page.getByTitle('字段设置',{exact:true}).nth(2).click();
    const defaults=page.getByLabel('子字段默认值');assert.equal(await defaults.nth(0).inputValue(),'300');assert.equal(await defaults.nth(1).inputValue(),'Ab');
    await defaults.nth(0).fill('301');
    await page.getByRole('button',{name:'保存',exact:true}).click();
    const saved=await page.evaluate(()=>window.__testCalls.filter(c=>c.cmd==='upsert_protocol').at(-1));
    assert.equal(saved.args.payload.fields[0].enc,'uint');assert.equal(saved.args.payload.fields[2].unit_fields[0].value,'301');assert.equal(saved.args.payload.fields[2].unit_fields[1].enc,'ascii');
    assert.deepEqual(errors,[]);
    console.log('PASS: two independent arrays, UInt/ASCII defaults, A/B switch, zero count, branch hide, configuration-driven profile fields');
  } finally {await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
