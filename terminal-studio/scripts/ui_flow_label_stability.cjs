const { chromium } = require('playwright');
const assert = require('node:assert/strict');
(async()=>{
 const browser=await chromium.launch({channel:'chrome',headless:true});
 const page=await browser.newPage({viewport:{width:1400,height:900}});
 await page.route('**/__flow_fixture',route=>route.fulfill({contentType:'text/html',body:`<!doctype html><html><body><div id="root" style="height:850px"></div><script type="module">
 import RefreshRuntime from '/@react-refresh';RefreshRuntime.injectIntoGlobalHook(window);window.$RefreshReg$=()=>{};window.$RefreshSig$=()=>type=>type;window.__vite_plugin_react_preamble_installed__=true;
 const {default:React}=await import('/node_modules/.vite/deps/react.js');const {default:ReactDOM}=await import('/node_modules/.vite/deps/react-dom_client.js');
 window.runs=[];window.__TAURI_INTERNALS__={invoke:async(c)=>c==='flow_traces'?structuredClone(window.runs):[]};
 await import('/src/styles/globals.css');await import('/node_modules/@xyflow/react/dist/style.css');
 const {FlowTestView}=await import('/src/components/flows/FlowTestView.tsx');
 const flow={id:1,name:'标签稳定性',system_ref:1,steps:[{id:'a',name:'配置',protocol_id:'a',x:0,y:0,on_success:'b'},{id:'b',name:'注册',protocol_id:'b',x:0,y:200}]};
 window.flow=flow;
 ReactDOM.createRoot(document.getElementById('root')).render(React.createElement(FlowTestView,{flow,protocols:[],groups:[],onBack:()=>{}}));
 </script></body></html>`}));
 try {
  await page.goto('http://127.0.0.1:1426/__flow_fixture');
  await page.locator('.react-flow__edge-text').waitFor();
  await page.waitForTimeout(1000);
  const sample=()=>page.evaluate(async()=>{
   const root=document.querySelector('.react-flow');const text=root.querySelector('.react-flow__edge-text');
   const changes=[];
   const observer=new MutationObserver(ms=>{for(const m of ms)if(m.type==='childList'&&[...m.removedNodes].some(n=>n.contains?.(text)||n.nodeType===1&&n.querySelector?.('.react-flow__edge-text')))changes.push('label removed');else if(m.type==='attributes'&&m.attributeName==='visibility')changes.push(m.target.getAttribute('visibility'));});
   observer.observe(root,{childList:true,subtree:true,attributes:true,attributeFilter:['visibility']});
   await new Promise(r=>setTimeout(r,2200));observer.disconnect();return {changes,same:text===root.querySelector('.react-flow__edge-text')};
  });
  for(const running of [false,true]){
   if(running){
    await page.evaluate(()=>{const at=Date.now();window.runs=[{run_id:1,flow:window.flow,node_id:0,node_name:'测试',terminal_ids:[],status:'running',started_at:at,events:[],transitions:[],step_states:{a:{status:'waiting',at,attempt:1}}}];});
    await page.locator('.flow-test-node.state-waiting').waitFor();
    await page.waitForTimeout(500);
   }
   const before=await page.locator('.flow-test-node').first().innerText();
   const result=await sample();
   console.log({running,...result});assert.equal(result.same,true,'Polling/timer must preserve the label DOM');assert.deepEqual(result.changes,[],'Labels must not hide or remount between polls');
   if(running)assert.notEqual(await page.locator('.flow-test-node').first().innerText(),before,'Waiting elapsed time must still update');
  }
 }finally{await browser.close();}
})().catch(e=>{console.error(e);process.exitCode=1;});
