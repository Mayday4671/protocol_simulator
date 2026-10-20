const {chromium}=require('playwright');
const assert=require('node:assert/strict');
(async()=>{
 const browser=await chromium.launch({channel:'chrome',headless:true});
 const page=await browser.newPage({viewport:{width:1440,height:1000}});
 const errors=[];page.on('pageerror',e=>errors.push(String(e)));
 await page.addInitScript(()=>{
   const sets=[{id:1,key:'types',name:'设备类型',byte_len:1,group_ref:null,items:Array.from({length:21},(_,i)=>({id:i+1,set_ref:1,value:i.toString(16).padStart(2,'0'),label:'类型条目'+i,sort_order:i}))},{id:2,key:'states',name:'运行状态',byte_len:1,group_ref:1,system_name:'测试系统',group_name:'配置',items:[{id:30,set_ref:2,value:'01',label:'在线',sort_order:0}]}];
   window.calls=[];
   window.__TAURI_INTERNALS__={transformCallback:()=>1,unregisterCallback:()=>{},invoke:async(cmd,args)=>{
     window.calls.push({cmd,args});
     if(cmd==='list_dict_sets')return sets;
     if(cmd==='list_systems')return [{id:1,name:'测试系统'}];
     if(cmd==='list_groups')return [{id:1,system_ref:1,name:'配置'}];
     if(cmd==='engine_status')return {listening_nodes:[],heartbeats:[],periodic_sends:[]};
     if(cmd==='log_stats')return {total:0,in_count:0,out_count:0,error_count:0};
     if(cmd==='dict_set_refs')return ['协议'+args.id];
     if(cmd==='upsert_dict_item')return {};
     return [];
   }};
   window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:()=>{}};
 });
 try {
   await page.goto('http://127.0.0.1:1422/dict');
   await page.getByText('全局共享',{exact:true}).first().waitFor();
   assert.equal(await page.getByRole('button',{name:/折叠字典：|展开字典：/}).count(),2,'right pane must contain two collapsible dictionaries');
   const first=page.locator('#dict-panel-1'),second=page.locator('#dict-panel-2');
   await first.getByRole('button',{name:'下一页'}).click();await first.getByText('类型条目20',{exact:true}).waitFor();
   await first.getByRole('button',{name:'折叠字典：设备类型'}).click();assert.equal(await first.getByRole('table').count(),0);
   await first.getByRole('button',{name:'展开字典：设备类型'}).click();
   await first.getByText('类型条目20',{exact:true}).waitFor();
   const tree=page.getByRole('navigation',{name:'字典树'});assert.equal(await tree.getByText('类型条目20',{exact:true}).count(),0);assert.equal(await tree.getByText('设备类型',{exact:true}).count(),0);
   await second.getByRole('button',{name:'展开字典：运行状态'}).click();
   await second.getByRole('button',{name:'编辑条目'}).click();
   await page.locator('#di-label').fill('在线修改');await page.getByRole('button',{name:'保存',exact:true}).click();
   const call=await page.evaluate(()=>window.calls.filter(c=>c.cmd==='upsert_dict_item').at(-1));assert.equal(call.args.payload.set_ref,2);
   await page.getByPlaceholder('搜索字典名 / Key / 条目').fill('在线');
   await page.getByRole('button',{name:/字典：运行状态/}).waitFor();
   assert.equal(await page.getByRole('button',{name:/折叠字典：|展开字典：/}).count(),1);
   await page.screenshot({path:'ui-dictionary-regression.png',fullPage:true});
   assert.deepEqual(errors,[]);console.log('PASS dictionary accordion, tree navigation, independent pagination, edit ownership and search');
 }catch(e){console.log(await page.locator('body').innerText());throw e;}finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
