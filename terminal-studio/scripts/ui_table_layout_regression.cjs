// Real page/components, isolated IPC fixtures. No user database or UDP traffic.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const fixture=`<!doctype html><html><head><meta charset="UTF-8"></head><body><div id="root"></div><script type="module">
  import RefreshRuntime from '/@react-refresh';
  RefreshRuntime.injectIntoGlobalHook(window);window.$RefreshReg$=()=>{};window.$RefreshSig$=()=>type=>type;window.__vite_plugin_react_preamble_installed__=true;
  const {default:React}=await import('/node_modules/.vite/deps/react.js');
  const {default:ReactDOM}=await import('/node_modules/.vite/deps/react-dom_client.js');
  await import('/src/styles/globals.css');
  const {ProtocolManager}=await import('/src/pages/ProtocolManager.tsx');
  const {NodeManager}=await import('/src/pages/NodeManager.tsx');
  const {DictionarySection}=await import('/src/components/DictionarySection.tsx');
  const {LogProvider}=await import('/src/hooks/useUdpLogs.tsx');
  const h=React.createElement;
  function Fixture(){
    const [open,setOpen]=React.useState(true);
    const view=new URLSearchParams(location.search).get('view')||'protocols';
    const content=view==='protocols'?h(ProtocolManager):view==='nodes'?h(LogProvider,null,h(NodeManager)):h('div',{className:'p-6'},h(DictionarySection,{dictionary:window.__dictionary,open,selected:false,onToggle:()=>setOpen(value=>!value),onEditSet:()=>{},onDeleteSet:()=>{},onEditItem:()=>{},onDeleteItem:()=>{}}));
    return h('div',{className:'flex h-screen overflow-hidden'},h('aside',{className:'w-56 shrink-0 border-r bg-card p-4'},h('strong',null,'多协议模拟系统'),h('div',{className:'mt-8 text-sm'},'协议管理 · 通道管理 · 字典管理')),h('div',{className:'min-w-0 flex-1 flex flex-col'},h('header',{className:'h-10 shrink-0 border-b px-6 flex items-center text-sm'},'列表布局验收'),h('main',{className:'min-h-0 flex-1 overflow-auto'},content)));
  }
  ReactDOM.createRoot(document.getElementById('root')).render(h(Fixture));
</script></body></html>`;

(async()=>{
  const browser=await chromium.launch({channel:'chrome',headless:true});
  const page=await browser.newPage({viewport:{width:1600,height:1000}});
  page.setDefaultTimeout(8000);
  const out=process.env.UI_ARTIFACT_DIR||path.resolve('artifacts/table-layout');fs.mkdirSync(out,{recursive:true});
  const errors=[];page.on('pageerror',error=>errors.push(String(error)));
  const base=process.env.UI_BASE_URL||'http://127.0.0.1:1423';
  const report={checks:[],layouts:[],pageErrors:errors};
  const pass=name=>{report.checks.push(name);console.log('PASS: '+name);};
  await page.addInitScript(()=>{
    const protocols=Array.from({length:80},(_,index)=>({id:index+1,protocol_id:'p'+(index+1),name:'联调协议 '+String(index+1).padStart(2,'0'),def_mode:'fields',format:'hex',msg_type:['Request','Response','Heartbeat','Event'][index%4],fields_json:JSON.stringify([{name:'信令码',len:1,enc:'hex',value:(index+1).toString(16).padStart(2,'0'),role:'message_type'}]),payload_template:'',system_ref:1,group_ref:1,header_refs_json:'[]',system_name:'联调系统',group_name:'配置分组',respond_delay_ms:0,auto_periodic_interval_ms:1000}));
    const nodes=Array.from({length:18},(_,index)=>({id:index+1,name:'联调通道 '+String(index+1).padStart(2,'0'),role:'simulator',local_ip:'127.0.0.1',local_port:39000+index,remote_ip:'127.0.0.1',remote_port:40000+index,node_id_hex:'01',group_bind_json:'[1]',enabled:true}));
    window.__dictionary={id:1,key:'test-results',name:'联调结果',byte_len:1,group_ref:1,system_name:'联调系统',group_name:'配置分组',items:Array.from({length:65},(_,index)=>({id:index+1,set_ref:1,value:index.toString(16).padStart(2,'0').toUpperCase(),label:'配置结果 '+String(index+1).padStart(2,'0'),sort_order:index}))};
    window.__ipcCalls=[];
    window.__TAURI_INTERNALS__={transformCallback:()=>1,unregisterCallback:()=>{},invoke:async(cmd,args)=>{
      window.__ipcCalls.push({cmd,args});
      if(cmd==='list_protocols')return structuredClone(protocols);
      if(cmd==='list_nodes')return structuredClone(nodes);
      if(cmd==='list_dict_sets')return [structuredClone(window.__dictionary)];
      if(cmd==='list_systems')return [{id:1,name:'联调系统'}];
      if(cmd==='list_groups')return [{id:1,system_ref:1,name:'配置分组',header_enabled:false,header_fields_json:null,header_profiles_json:'[]'}];
      if(cmd==='engine_status')return {listening_nodes:[],heartbeats:[],periodic_sends:[]};
      if(cmd==='log_stats')return {today_in:0,today_out:0,total:0};
      if(cmd==='plugin:event|listen')return 1;
      if(cmd==='plugin:event|unlisten')return null;
      throw new Error('Unexpected fixture command: '+cmd);
    }};
    window.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener:()=>{}};
  });
  await page.route('**/__table_layout_fixture?*',route=>route.fulfill({contentType:'text/html',body:fixture}));
  const load=async(view,width=1600,height=1000,theme='light')=>{
    await page.setViewportSize({width,height});
    await page.goto(base+'/__table_layout_fixture?view='+view);
    await page.locator('tbody tr').first().waitFor();
    await page.evaluate(dark=>document.documentElement.classList.toggle('dark',dark),theme==='dark');
    await page.waitForFunction(()=>Array.from(document.querySelectorAll('button')).every(element=>element.getAnimations().every(animation=>animation.playState!=='running')));
  };
  const metrics=()=>page.locator('table').evaluate(table=>{
    const rect=el=>{const r=el.getBoundingClientRect();return {x:r.x,y:r.y,width:r.width,height:r.height,right:r.right,bottom:r.bottom};};
    const cells=selector=>Array.from(table.querySelectorAll(selector),el=>({text:el.textContent.trim(),paddingLeft:getComputedStyle(el).paddingLeft,paddingRight:getComputedStyle(el).paddingRight,...rect(el)}));
    const ancestors=[];let el=table.parentElement;
    while(el&&el.tagName!=='BODY'){
      const css=getComputedStyle(el);ancestors.push({slot:el.dataset.slot,className:el.className,overflowX:css.overflowX,overflowY:css.overflowY,clientHeight:el.clientHeight,scrollHeight:el.scrollHeight,clientWidth:el.clientWidth,scrollWidth:el.scrollWidth,...rect(el)});el=el.parentElement;
    }
    const filter=Array.from(document.querySelectorAll('button')).find(button=>button.textContent==='请求')?.parentElement;
    const nav=Array.from(document.querySelectorAll('nav')).find(nav=>nav.getAttribute('aria-label')?.endsWith('分页'));
    return {heads:cells('thead th'),body:cells('tbody tr:first-child td'),ancestors,rows:table.querySelectorAll('tbody tr').length,header:{position:getComputedStyle(table.tHead).position,...rect(table.tHead)},filter:filter?{paddingLeft:getComputedStyle(filter).paddingLeft,paddingTop:getComputedStyle(filter).paddingTop,...rect(filter)}:null,nav:nav?{paddingLeft:getComputedStyle(nav).paddingLeft,...rect(nav)}:null,pageOverflow:document.documentElement.scrollWidth>innerWidth};
  });
  const selectSize=async(label,value)=>{await page.getByRole('combobox',{name:label}).click();await page.getByRole('option',{name:value+' 条/页',exact:true}).click();};
  const checkSticky=async(view)=>{
    const scroll=page.locator('[data-slot="table-scroll"]').first();
    const header=page.locator('thead');
    const before=await header.boundingBox();
    await scroll.hover();await page.mouse.wheel(0,420);
    await page.waitForFunction(()=>document.querySelector('[data-slot="table-scroll"]').scrollTop>50);
    const after=await header.boundingBox();
    assert.ok(Math.abs(before.y-after.y)<2,view+' header remains pinned during vertical scrolling');
    return {before,after,scrollTop:await scroll.evaluate(element=>element.scrollTop)};
  };
  try{
    if(process.argv.includes('--before')){
      const baseline={};
      for(const view of ['protocols','nodes','dictionary']){
        await load(view);
        assert.equal(await page.locator('[data-slot="table-scroll"]').count(),0,'--before requires the original layout; do not label repaired layout as baseline');
        baseline[view]=await metrics();
        if(view==='dictionary'){
          baseline.dictionary.stickyBefore=await page.locator('thead').boundingBox();
          await page.locator('table').hover();await page.mouse.wheel(0,240);
          await page.waitForTimeout(100);
          baseline.dictionary.stickyAfter=await page.locator('thead').boundingBox();
        }
        await page.screenshot({path:path.join(out,'before-'+view+'.png'),fullPage:true});
      }
      fs.writeFileSync(path.join(out,'before.json'),JSON.stringify(baseline,null,2));
      console.log('Baseline captured');return;
    }

    for(const width of [1600,1280,760]){
      for(const theme of ['light','dark']){
        await load('protocols',width,900,theme);
        await page.locator('[data-slot="table-toolbar"]').waitFor();
        const measured=await metrics();
        assert.equal(measured.filter.paddingLeft,'12px');
        assert.equal(measured.filter.paddingTop,'10px');
        assert.equal(measured.nav.paddingLeft,'12px');
        for(const cell of [...measured.heads,...measured.body]){assert.equal(cell.paddingLeft,'12px');assert.equal(cell.paddingRight,'12px');}
        assert.equal(await page.locator('[data-slot="table-panel"] [data-slot="table-toolbar"]').count(),1);
        const scroll=page.locator('[data-slot="table-scroll"]');
        const layout=await scroll.evaluate(element=>({clientHeight:element.clientHeight,scrollHeight:element.scrollHeight,scrollWidth:element.scrollWidth,clientWidth:element.clientWidth,nested:element.querySelectorAll('[data-slot="table-scroll"]').length}));
        assert.equal(layout.nested,0);
        assert.ok(layout.scrollHeight<=layout.clientHeight+2,'auto pagination fits actual table viewport');
        assert.equal(measured.pageOverflow,false);
        const rowHeight=measured.body[0].height;
        assert.ok(layout.clientHeight-(measured.header.height+measured.rows*rowHeight)<rowHeight+24,'auto pagination fills available space');
        for(let i=0;i<measured.heads.length;i++)assert.ok(Math.abs(measured.heads[i].x-measured.body[i].x)<2,'header/body columns align');
        report.layouts.push({view:'protocols',width,theme,measured,layout});
        await page.screenshot({path:path.join(out,`protocols-${width}-${theme}.png`),fullPage:true});
      }
    }
    pass('protocol filters, columns and pagination align at 1600/1280/760 in light/dark; auto rows fit the scroll viewport');

    await load('protocols');
    await page.getByRole('button',{name:'请求',exact:true}).click();
    await page.waitForFunction(()=>Array.from(document.querySelectorAll('tbody tr')).every(row=>row.children[1]?.textContent==='请求'));
    assert.match(await page.getByRole('navigation',{name:'协议分页'}).innerText(),/共 20 条/);
    await page.getByRole('button',{name:'全部',exact:true}).click();
    await selectSize('协议每页条数',50);
    await page.waitForFunction(()=>document.querySelectorAll('tbody tr').length===50);
    report.protocolSticky=await checkSticky('protocol');
    await page.getByRole('button',{name:'下一页',exact:true}).click();
    await page.waitForFunction(()=>document.querySelectorAll('tbody tr').length===30);
    assert.match(await page.locator('tbody tr').first().innerText(),/联调协议 51/);
    await page.getByRole('button',{name:'上一页',exact:true}).click();
    await page.waitForFunction(()=>document.querySelectorAll('tbody tr').length===50);
    await page.setViewportSize({width:760,height:900});
    const protocolScroll=page.locator('[data-slot="table-scroll"]');
    await protocolScroll.hover();await page.mouse.wheel(440,0);
    await page.waitForFunction(()=>document.querySelector('[data-slot="table-scroll"]').scrollLeft>0);
    const action=await page.locator('tbody tr').first().locator('td').last().boundingBox();
    const scroller=await protocolScroll.boundingBox();
    assert.ok(Math.abs(action.x+action.width-(scroller.x+scroller.width))<2,'protocol actions stay at the right viewport edge');
    await page.screenshot({path:path.join(out,'protocols-50-scrolled.png'),fullPage:true});
    pass('protocol filtering and paging work; 50-row vertical/horizontal scroll retains headers and actions');

    for(const width of [1600,1280,760]){
      for(const theme of ['light','dark']){
      await load('dictionary',width,900,theme);
      await selectSize('联调结果条目每页条数',50);
      await page.waitForFunction(()=>document.querySelectorAll('tbody tr').length===50);
      const measured=await metrics();
      const sameTableScroll=measured.ancestors.filter(ancestor=>/auto|scroll/.test(ancestor.overflowY)).filter(ancestor=>ancestor.className!=='min-h-0 flex-1 overflow-auto');
      assert.equal(sameTableScroll.length,1,'dictionary table has one scroll owner');
      assert.equal(measured.pageOverflow,false);
      const sticky=await checkSticky('dictionary');
      report.layouts.push({view:'dictionary',width,theme,measured,sticky});
      await page.getByRole('button',{name:'下一页',exact:true}).click();
      await page.waitForFunction(()=>document.querySelectorAll('tbody tr').length===15);
      assert.match(await page.locator('tbody tr').first().innerText(),/配置结果 51/);
      await page.screenshot({path:path.join(out,`dictionary-${width}-${theme}.png`),fullPage:true});
      }
    }
    await page.setViewportSize({width:720,height:900});
    const dictionaryScroll=page.locator('[data-slot="table-scroll"]');
    await dictionaryScroll.hover();await page.mouse.wheel(180,0);
    await page.waitForFunction(()=>document.querySelector('[data-slot="table-scroll"]').scrollLeft>0);
    await page.screenshot({path:path.join(out,'dictionary-horizontal-scroll.png'),fullPage:true});
    pass('dictionary 50-row scrolling has one scroll container, pinned header, working page two and no page overflow');

    for(const width of [1600,1280,760]){
      for(const theme of ['light','dark']){
      await load('nodes',width,900,theme);
      const measured=await metrics();
      assert.equal(measured.pageOverflow,false);
      const panel=page.locator('[data-slot="table-panel"]');
      const clipping=await panel.evaluate(element=>({overflow:getComputedStyle(element).overflow,borderRadius:getComputedStyle(element).borderRadius}));
      assert.equal(clipping.overflow,'hidden');assert.notEqual(clipping.borderRadius,'0px');
      for(const cell of [...measured.heads,...measured.body]){assert.equal(cell.paddingLeft,'12px');assert.equal(cell.paddingRight,'12px');}
      for(let i=0;i<measured.heads.length;i++)assert.ok(Math.abs(measured.heads[i].x-measured.body[i].x)<2,'node header/body columns align');
      const sticky=await checkSticky('nodes');
      if(width===760){
        await page.locator('[data-slot="table-scroll"]').hover();await page.mouse.wheel(430,0);
        await page.waitForFunction(()=>document.querySelector('[data-slot="table-scroll"]').scrollLeft>0);
        const action=await page.locator('tbody tr').first().locator('td').last().boundingBox();
        const viewport=await page.locator('[data-slot="table-scroll"]').boundingBox();
        assert.ok(Math.abs(action.x+action.width-(viewport.x+viewport.width))<2,'node actions stay at the viewport edge while scrolling');
      }
      report.layouts.push({view:'nodes',width,theme,measured,clipping,sticky});
      await page.screenshot({path:path.join(out,`nodes-${width}-${theme}.png`),fullPage:true});
      }
    }
    pass('node table clips rounded corners and aligns header/action columns without outer overflow');
    assert.deepEqual(errors,[]);
    const writes=await page.evaluate(()=>window.__ipcCalls.filter(call=>!/^(list_|engine_status|log_stats|plugin:event\|)/.test(call.cmd)));
    assert.deepEqual(writes,[]);
    fs.writeFileSync(path.join(out,'results.json'),JSON.stringify(report,null,2));
  }catch(error){
    console.error('Page errors:',errors);
    console.error('Body:',(await page.locator('body').innerText()).slice(-1200));
    await page.screenshot({path:path.join(out,'failure.png'),fullPage:true});throw error;
  }finally{await browser.close();}
})().catch(error=>{console.error(error);process.exitCode=1;});
