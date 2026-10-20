const {chromium}=require('playwright');
const assert=require('node:assert/strict');
(async()=>{
 const browser=await chromium.launch({channel:'chrome',headless:true});
 const page=await browser.newPage({viewport:{width:1440,height:1000}});
 const errors=[];page.on('pageerror',e=>errors.push(String(e)));
 await page.addInitScript(({aliases})=>{
   const f=(name,value,when)=>({name,len:1,enc:'hex',value,role:'none',...(when?{when:{field:'模式',value:when}}:{})});
   const fields=[f('模式','01'),f('A字段1','A1','01'),f('A字段2','A2',aliases?'0 1':'01'),f('B字段1','B1','02'),f('B字段2','B2','02'),f('尾标','EE')];
   const protocol={id:1,protocol_id:'branches',name:'分支验收',def_mode:'fields',format:'hex',msg_type:'Request',fields_json:JSON.stringify(fields),payload_template:'',system_ref:1,group_ref:1,system_name:'测试系统',group_name:'配置',respond_delay_ms:0,auto_periodic_interval_ms:1000};
   const sets=Array.from({length:13},(_,i)=>({id:i+1,key:'d'+i,name:'配置字典'+i,byte_len:1,group_ref:1,system_name:'测试系统',group_name:'配置',items:[{id:i+1,set_ref:i+1,value:'01',label:'条目'+i,sort_order:0}]}));
   sets.push({id:50,key:'biz',name:'业务字典',byte_len:1,group_ref:2,system_name:'测试系统',group_name:'业务',items:[]});
   window.calls=[];window.__TAURI_INTERNALS__={transformCallback:()=>1,unregisterCallback:()=>{},invoke:async(cmd,args)=>{
     window.calls.push({cmd,args});
     if(cmd==='list_protocols')return [protocol];
     if(cmd==='list_dict_sets')return structuredClone(sets);
     if(cmd==='list_systems')return [{id:1,name:'测试系统'}];
     if(cmd==='list_groups')return [{id:1,system_ref:1,name:'配置',header_enabled:true,header_fields_json:JSON.stringify([f('公共类型','01')])},{id:2,system_ref:1,name:'业务'}];
     if(cmd==='list_nodes')return [{id:1,name:'测试通道',role:'simulator',local_ip:'127.0.0.1',local_port:19001,remote_ip:'127.0.0.1',remote_port:19002,node_id_hex:'01',group_bind_json:'[1]',enabled:true}];
     if(cmd==='engine_status')return {listening_nodes:[],heartbeats:[],periodic_sends:[]};
     if(cmd==='log_stats')return {total:0,in_count:0,out_count:0,error_count:0};
     if(cmd==='upsert_protocol')return protocol;
     if(cmd==='delete_dict_set'){sets.splice(sets.findIndex(s=>s.id===args.id),1);return null;}
     if(cmd==='upsert_dict_set'){
       const set={...args.payload,id:Math.max(...sets.map(s=>s.id))+1,key:'created',items:[],system_name:'测试系统',group_name:'配置'};
       sets.push(set);return set;
     }
     return [];
   }};window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:()=>{}};
 },{aliases:process.argv.includes('--aliases')});
 try {
   if(process.argv.includes('--console')) {
     await page.goto('http://127.0.0.1:1422/console');
     await page.getByRole('combobox').nth(1).click();await page.getByRole('option',{name:'配置',exact:true}).click();
     await page.getByRole('combobox').nth(2).click();await page.getByRole('option',{name:'分支验收',exact:true}).click();
     await page.locator('[id="f-A字段1"]').waitFor();
     assert.equal(await page.locator('[data-branch-group]').count(),1,'console shares one active condition group');
     assert.equal(await page.locator('[id="f-A字段2"]').count(),1,'equivalent condition values must not hide matching fields');
     assert.equal(await page.locator('[id="f-B字段1"]').count(),0);
     await page.locator('[id="f-模式"]').fill('02');await page.locator('[id="f-B字段1"]').waitFor();
     assert.equal(await page.locator('[id="f-A字段1"]').count(),0);
     await page.locator('[id="f-B字段1"]').fill('CC');
     await page.getByRole('button',{name:'收起分支组：模式'}).click();
     assert.equal(await page.locator('[id="f-B字段1"]').isVisible(),false);
     await page.getByRole('button',{name:'展开分支组：模式'}).click();
     assert.equal(await page.locator('[id="f-B字段1"]').inputValue(),'CC');
     assert.equal(await page.getByLabel('当前分支：模式').count(),0,'console branch follows field input, not an independent preview');
     await page.screenshot({path:'ui-console-branches.png',fullPage:true});
   } else if(process.argv.includes('--dict')) {
     await page.goto('http://127.0.0.1:1422/dict');
     const tree=page.getByRole('navigation',{name:'字典树'});
     await tree.getByText('测试系统',{exact:true}).waitFor();
     assert.equal(await tree.getByText('配置字典0',{exact:true}).count(),0,'tree stops at system/group; dictionary names belong on right');
     await tree.getByRole('button',{name:/配置/}).click();
     assert.equal(await page.locator('[id^="dict-panel-"]').count(),13,'auto page size fills the viewport with every set on one page');
     await page.getByLabel('字典每页条数').click();await page.getByRole('option',{name:'10 条/页'}).click();
     assert.equal(await page.locator('[id^="dict-panel-"]').count(),10,'fixed page size paginates');
     await page.getByRole('button',{name:'下一页字典'}).click();
     await page.locator('#dict-panel-13').waitFor();assert.equal(await page.locator('[id^="dict-panel-"]').count(),3);
     await page.screenshot({path:'ui-dictionary-page2.png',fullPage:true});
     page.on('dialog', dialog => dialog.accept());
     for(const id of [13,12,11]) {
       await page.locator('#dict-panel-'+id).getByTitle('删除字典',{exact:true}).click();
       await page.locator('#dict-panel-'+id).waitFor({state:'detached'});
     }
     await page.locator('#dict-panel-1').waitFor();assert.equal(await page.locator('[id^="dict-panel-"]').count(),10,'deleting the last page falls back to page one');
     await page.getByTitle('新增字典',{exact:true}).click();await page.locator('#ds-name').fill('新增配置字典');await page.getByRole('button',{name:'保存',exact:true}).click();
     await page.locator('#dict-panel-51').waitFor();assert.equal(await page.locator('[id^="dict-panel-"]').count(),1,'new dictionary opens on its actual page');
     const created=await page.evaluate(()=>window.calls.filter(c=>c.cmd==='upsert_dict_set').at(-1));assert.equal(created.args.payload.group_ref,1,'new dictionary defaults to selected group');
     await tree.getByRole('button',{name:/业务/}).click();await page.locator('#dict-panel-50').waitFor();
     assert.equal(await page.locator('[id^="dict-panel-"]').count(),1);
     await page.getByPlaceholder('查询字典名 / Key / 条目').fill('不存在');await page.getByText('没有匹配的字典',{exact:true}).waitFor();
     await page.getByPlaceholder('查询字典名 / Key / 条目').fill('');await page.locator('#dict-panel-50').waitFor();
     await page.screenshot({path:'ui-dictionary-scope.png',fullPage:true});
   } else {
     await page.goto('http://127.0.0.1:1422/protocols');await page.getByTitle('编辑',{exact:true}).click();
     const section=value=>page.locator(`[data-branch-group="模式"][data-branch-value="${value}"]`);
     await section('01').waitFor();await section('02').waitFor();
     assert.equal(await page.locator('[data-branch-group]').count(),2,'every condition value renders as its own group in the editor');
     await page.locator('input[value="A字段1"]').waitFor();await page.locator('input[value="A字段2"]').waitFor();
     await page.locator('input[value="B字段1"]').waitFor();
     await section('01').getByRole('button',{name:'收起分支组：模式'}).click();
     assert.equal(await page.locator('input[value="A字段1"]').isVisible(),false);
     assert.equal(await page.locator('input[value="B字段1"]').isVisible(),true,'collapsing one group leaves other choices visible');
     await section('01').getByRole('button',{name:'展开分支组：模式'}).click();
     await page.locator('input[value="A字段1"]').waitFor();
     await page.screenshot({path:'ui-editor-branches.png',fullPage:true});
     await page.getByRole('button',{name:'保存',exact:true}).click();
     const save=await page.evaluate(()=>window.calls.filter(c=>c.cmd==='upsert_protocol').at(-1));
     assert.deepEqual(save.args.payload.fields.map(f=>f.name),['模式','A字段1','A字段2','B字段1','B字段2','尾标']);
     assert.deepEqual(save.args.payload.fields.slice(1,5).map(f=>f.when.value),['01','01','02','02']);
     await page.getByTitle('编辑',{exact:true}).click();
     const edit01=page.getByRole('button',{name:'编辑分支条件：模式 = 01'});
     await edit01.waitFor();
     assert.equal(await page.getByRole('button',{name:/^编辑分支条件：模式 = /}).count(),2,'one condition editor per branch group');
     await edit01.click();
     await page.getByLabel('分支条件字段').click();
     assert.equal(await page.getByRole('option',{name:'公共类型',exact:true}).count(),1,'common headers remain valid branch condition candidates');
     await page.keyboard.press('Escape');
     await page.getByLabel('分支匹配值').fill('03');
     await page.getByRole('button',{name:'应用分支条件',exact:true}).click();
     await section('03').waitFor();
     await page.getByRole('button',{name:'保存',exact:true}).click();
     const edited=await page.evaluate(()=>window.calls.filter(c=>c.cmd==='upsert_protocol').at(-1));
     assert.deepEqual(edited.args.payload.fields.slice(1,5).map(f=>f.when.value),['03','03','02','02']);
   }
   assert.deepEqual(errors,[]);console.log('PASS',process.argv.includes('--dict')?'dictionary two-level scopes and dictionary pagination':process.argv.includes('--console')?'console active branch groups and collapse retains input':'grouped branch switching, whole-branch condition editing and unchanged wire order');
 }catch(e){console.log(await page.locator('body').innerText());throw e;}finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
