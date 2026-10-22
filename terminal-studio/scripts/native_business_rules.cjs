// Run against a fresh SIMTERM_DB with SIMTERM_UI_TEST_PORT=9259.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const dgram = require('node:dgram');
const fs = require('node:fs');
(async () => {
  const peer = dgram.createSocket('udp4');
  await new Promise(resolve => peer.bind(0, '127.0.0.1', resolve));
  const reserve = dgram.createSocket('udp4'); await new Promise(resolve => reserve.bind(0, '127.0.0.1', resolve));
  const localPort = reserve.address().port; await new Promise(resolve => reserve.close(resolve));
  const wires=[];
  peer.on('message',(msg,from)=>{ wires.push(msg.toString('hex')); const reply=msg[0]===0x21 ? Buffer.from([0x22,msg[1],2,1,0xAA,2,0]) : Buffer.from([0x32,msg[1],0xAA]);peer.send(reply,from.port,from.address); });
  const browser = await chromium.connectOverCDP('http://127.0.0.1:9259');
  const page = browser.contexts()[0].pages()[0];page.setDefaultTimeout(12000);
  const invoke = (command,args={}) => page.evaluate(([c,a])=>window.__TAURI_INTERNALS__.invoke(c,a),[command,args]);
  const waitRecords = async count => { const deadline = Date.now() + 6000; while (Date.now() < deadline) { if ((await invoke('list_business_records')).length === count) return; await new Promise(r => setTimeout(r, 80)); } throw new Error('Timed out waiting for '+count+' saved records'); };
  try {
    assert.deepEqual(await invoke('list_protocols'),[], 'Use a new isolated database only');
    const system = await invoke('create_system',{payload:{name:'应答保存验收',description:''}});
    const group = await invoke('create_group',{payload:{name:'设备与电话',system_ref:system.id}});
    const f=(name,len,value='',role='none',extra={})=>({name,len,value,role,enc:'hex',...extra});
    const definitions=[
      ['device_req','设备配置','Request','21',[f('流水号',1,'01'),f('数量',1,'02','device_count'),f('条目',3,'011234025678','device_list',{unit_fields:[f('设备地址',1),f('端口',2)]})]],
      ['device_ack','设备配置应答','Response','22',[f('流水号',1),f('数量',1,'','device_count'),f('结果条目',2,'','device_list',{unit_fields:[f('设备地址',1),f('结果',1)]})]],
      ['phone_req','电话配置','Request','31',[f('流水号',1,'02'),f('电话号码',2,'1234'),f('所属设备',1,'01')]],
      ['phone_ack','电话配置应答','Response','32',[f('流水号',1),f('结果',1)]],
    ];
    for (const [id,name,type,code,fields] of definitions) await invoke('upsert_protocol',{payload:{protocol_id:id,name,msg_type:type,format:'hex',def_mode:'fields',system_ref:system.id,group_ref:group.id,fields:[f('信令码',1,code),...fields],match_fields:[{field:'信令码',value:code}]}});
    const node=await invoke('create_node',{payload:{name:'本机业务资料验收',role:'simulator',local_ip:'127.0.0.1',local_port:localPort,remote_ip:'127.0.0.1',remote_port:peer.address().port,node_id_hex:null,enabled:true,group_binds:[group.id]}});
    await invoke('save_record_type',{system:system.id,definition:{kind:'device',name:'设备档案',device_required:false,key_scope:'channel',properties:['设备地址','端口']}});
    await invoke('save_record_type',{system:system.id,definition:{kind:'phone',name:'电话资料',device_required:true,key_scope:'channel',properties:['电话号码']}});
    await page.getByText('协议管理',{exact:true}).first().click();
    async function choose(label,option){await page.getByRole('combobox',{name:label,exact:true}).click();await page.getByRole('option',{name:option,exact:true}).click();}
    async function mapping(name,field){await page.getByRole('button',{name:'添加属性',exact:true}).click();const index=await page.getByLabel(/^属性名称 /).count();await choose(`属性名称 ${index}`,name);await choose(`字段 ${index}`,field);}
    async function open(name){await page.getByRole('row').filter({has:page.getByRole('cell',{name,exact:true})}).getByRole('button',{name:'应答保存规则',exact:true}).click();await page.getByRole('checkbox').check();}
    await open('设备配置');await choose('应答协议','设备配置应答');
    await choose('请求流水号','流水号 · 1 B');await choose('应答流水号','流水号 · 1 B');
    await choose('请求资料','条目');await choose('应答结果','结果条目');
    await choose('条目对应','按条目标识');await choose('请求条目标识','条目 / 设备地址 · 1 B');await choose('应答条目标识','条目 / 设备地址 · 1 B');
    await choose('成功结果字段','条目 / 结果 · 1 B');await page.getByLabel('成功值',{exact:true}).fill('AA');
    await choose('唯一标识（请求字段）','条目 / 设备地址 · 1 B');
    await mapping('设备地址','条目 / 设备地址 · 1 B');await mapping('端口','条目 / 端口 · 2 B');
    await page.getByRole('button',{name:'保存规则',exact:true}).click();await page.getByRole('heading',{name:/应答保存规则/}).waitFor({state:'hidden'});
    await open('电话配置');await choose('应答协议','电话配置应答');await choose('保存到','电话资料');
    await choose('请求流水号','流水号 · 1 B');await choose('应答流水号','流水号 · 1 B');await choose('成功结果字段','结果 · 1 B');
    await page.getByLabel('成功值',{exact:true}).fill('AA');await choose('唯一标识（请求字段）','电话号码 · 2 B');await choose('所属设备标识（必选）','所属设备 · 1 B');
    await mapping('电话号码','电话号码 · 2 B');await page.getByRole('button',{name:'保存规则',exact:true}).click();await page.getByRole('heading',{name:/应答保存规则/}).waitFor({state:'hidden'});
    assert.equal((await invoke('list_save_rules')).length,2);
    await invoke('send_udp_message',{nodeId:node.id,protocolId:'device_req',params:{}});
    await waitRecords(1);
    await invoke('send_udp_message',{nodeId:node.id,protocolId:'phone_req',params:{}});
    await waitRecords(2);
    const records=await invoke('list_business_records');const devices=await invoke('list_terminals');
    assert.equal(devices.length,1);assert.equal(devices[0].device_key,'01');assert.equal(records.find(r=>r.kind==='phone').device_ref,devices[0].id);
    await assert.rejects(()=>invoke('delete_terminal',{id:devices[0].id}),/关联业务资料/);
    const xml=await invoke('export_protocols_xml_text',{protocolIds:['device_req','phone_req']});assert.equal((xml.match(/<save-rule /g)||[]).length,2);
    const imported=await invoke('import_protocols_xml_text',{xml});assert.deepEqual(imported.failed,[]);assert.equal((await invoke('list_save_rules')).length,2);
    await page.getByText('设备管理',{exact:true}).first().click();await page.getByRole('button',{name:'业务资料',exact:true}).click();
    await page.getByText('所属设备：01',{exact:true}).waitFor();await page.locator('summary').filter({hasText:'电话'}).click();
    fs.mkdirSync('artifacts/business-save',{recursive:true});await page.screenshot({path:'artifacts/business-save/native-records.png'});
    fs.writeFileSync('artifacts/business-save/native-result.json',JSON.stringify({wires,records,devices,xmlRoundtrip:true,linkedDeviceDeleteBlocked:true},null,2));
    await invoke('stop_all');console.log('PASS native EXE: UI config -> Rust UDP partial device ACK -> linked phone -> XML roundtrip -> delete guard');
  } finally { peer.close();await browser.close(); }
})().catch(e=>{console.error(e);process.exit(1);});
