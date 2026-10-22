// Isolated editor fixtures; no production database or protocol is modified.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
(async () => {
  const browser = await chromium.launch({ executablePath: 'C:/Program Files/Google/Chrome/Application/chrome.exe', headless: true });
  const page = await browser.newPage({ viewport: { width: 1000, height: 760 } });
  page.setDefaultTimeout(8000);
  const errors = [];
  page.on('pageerror', error => { errors.push(error.message); console.error('PAGE:', error.message); });
  page.on('requestfailed', request => console.error('REQUEST:', request.url(), request.failure()));
  await page.route('**/__values_fixture', route => route.fulfill({ contentType: 'text/html', body: `<!doctype html><html><head><meta charset="utf-8"></head><body><div id="root"></div><script type="module">
    import RefreshRuntime from '/@react-refresh';RefreshRuntime.injectIntoGlobalHook(window);window.$RefreshReg$=()=>{};window.$RefreshSig$=()=>type=>type;window.__vite_plugin_react_preamble_installed__=true;
    const {default:React}=await import('/node_modules/.vite/deps/react.js');const {default:ReactDOM}=await import('/node_modules/.vite/deps/react-dom_client.js');
    window.__TAURI_INTERNALS__={invoke:async command=>{
      if(command==='list_terminals')return [
        {id:1,name:'设备甲',system_ref:1,protocol_ref:'p1',device_key:'0012',fields_json:'{"address":"0012","serial":"ABCD"}'},
        {id:2,name:'设备乙',system_ref:2,protocol_ref:'p2',device_key:'1234',fields_json:'{"address":"1234"}'}];
      if(command==='list_systems')return [{id:1,name:'系统甲'},{id:2,name:'系统乙'}];
      if(command==='list_groups')return [];
      if(command==='list_business_records')return [{id:10,system_ref:1,kind:'port',device_ref:1,name:'端口01',properties_json:JSON.stringify({'配置值':{value:'22136',hex:'5678',enc:'uint',bytes:2}})}];
      if(command==='list_record_types')return [{kind:'port',name:'端口资料',device_required:true,key_scope:'device',properties:['配置值']}];
      if(command==='list_protocols')return [1,2].map(i=>({protocol_id:'p'+i,system_ref:i,def_mode:'fields',
        fields_json:JSON.stringify([{name:'设备地址',profile_key:'address',enc:'hex',len:2,role:'src_id'},
          {name:'序列号',profile_key:'serial',enc:'hex',len:2,role:'device_sn'}])}));
      throw new Error(command);
    }};
    await import('/src/styles/globals.css');const {FieldValueEditor}=await import('/src/components/protocols/FieldValueEditor.tsx');
    function App(){
      const [values,setValues]=React.useState(['{seq:2}','{session}','{tlen}','{last:认证令牌}','AA {seq:2}','01']);
      const [bound,setBound]=React.useState(true);window.bound=bound;
      window.values=values;window.changes=window.changes||[];
      return React.createElement('main',{className:'grid gap-4 p-6',style:{width:500}},...values.map((value,index)=>React.createElement(FieldValueEditor,{
        key:index,value,bytes:2,label:'字段'+index,onChange:next=>{window.changes.push([index,next]);setValues(v=>v.map((x,i)=>i===index?next:x))},
        onDeviceChange:index===5?next=>{setBound(false);setValues(v=>v.map((x,i)=>i===index?next:x))}:undefined,
        fixedEditor:index===5&&bound?React.createElement('select',{'aria-label':'字典值',value,onChange:e=>setValues(v=>v.map((x,i)=>i===index?e.target.value:x))},
          React.createElement('option',{value:''},'选择'),React.createElement('option',{value:'01'},'开启'),React.createElement('option',{value:'00'},'关闭')):undefined
      })));
    }
    ReactDOM.createRoot(document.getElementById('root')).render(React.createElement(App));
  </script></body></html>` }));
  try {
    await page.goto('http://127.0.0.1:1426/__values_fixture');
    await page.getByText('自动流水号', { exact: true }).waitFor();
    assert.equal(await page.getByText('当前会话', { exact: true }).count(), 1);
    assert.equal(await page.getByText('报文总长', { exact: true }).count(), 1);
    assert.deepEqual(await page.evaluate(() => window.changes), []);
    const choose = async (index, label) => {
      await page.getByRole('button', { name: `值来源：字段${index}`, exact: true }).click();
      await page.locator('[role="dialog"][data-state="open"]').getByRole('button', { name: label, exact: true }).click();
    };
    await choose(0, '自动流水号');
    assert.equal(await page.evaluate(() => window.values[0]), '{seq:2}');
    await choose(0, '固定值');
    await page.getByLabel('字段0', { exact: true }).fill('1234');
    assert.equal(await page.evaluate(() => window.values[0]), '1234');
    await choose(0, '最近接收字段');
    await page.getByLabel('接收字段：字段0', { exact: true }).fill('认证令牌');
    assert.equal(await page.evaluate(() => window.values[0]), '{last:认证令牌}');
    assert.equal(await page.getByLabel('接收字段：字段3', { exact: true }).inputValue(), '认证令牌');
    assert.equal(await page.getByLabel('表达式：字段4', { exact: true }).inputValue(), 'AA {seq:2}');
    await choose(4, '固定值');
    await choose(4, '自定义表达式');
    await page.getByLabel('表达式：字段4', { exact: true }).fill('AB {seq}');
    assert.equal(await page.evaluate(() => window.values[4]), 'AB {seq}');
    await page.getByLabel('字典值').selectOption('00');
    assert.equal(await page.evaluate(() => window.values[5]), '00');
    await choose(5, '当前会话');
    assert.equal(await page.evaluate(() => window.values[5]), '{session}');
    await choose(5, '固定值');
    await page.getByLabel('字典值').selectOption('01');
    assert.equal(await page.evaluate(() => window.values[5]), '01');
    await page.getByRole('button',{name:'从设备取值：字段0',exact:true}).click();
    await page.getByRole('button',{name:/设备甲 系统甲/}).waitFor();
    await page.getByLabel('搜索设备').fill('系统乙');
    assert.equal(await page.getByRole('button',{name:/设备甲 系统甲/}).count(),0);
    await page.getByRole('button',{name:/设备乙 系统乙/}).click();
    await page.getByRole('button',{name:/设备地址 · 1234/}).click();
    assert.equal(await page.evaluate(() => window.values[0]),'1234');
    const conversions=await page.evaluate(async()=>{
      const {devicePropertyValue:f}=await import('/src/components/protocols/DeviceValuePicker.tsx');
      const checks=[f('0012',{enc:'hex'},'uint',2),f('18',{enc:'uint'},'hex',3)];
      for(const args of [['1234',{enc:'hex'},'hex',1],['12',undefined,'hex',2],['xx',{enc:'ascii'},'hex',2]]) {
        try{f(...args);checks.push('unexpected success')}catch{checks.push('rejected')}
      }
      return checks;
    });
    assert.deepEqual(conversions,['18','000012','rejected','rejected','rejected']);
    await page.getByRole('button',{name:'从设备取值：字段5',exact:true}).click();
    await page.getByText('选择设备属性后，此字段改用固定值并解除字典绑定。',{exact:true}).waitFor();
    await page.getByRole('button',{name:/设备甲 系统甲/}).click();
    await page.getByRole('button',{name:/设备地址 · 0012/}).click();
    assert.equal(await page.evaluate(()=>window.bound),false);
    assert.equal(await page.getByLabel('字段5',{exact:true}).inputValue(),'0012');
    await page.getByRole('button',{name:'从设备取值：字段0',exact:true}).click();
    await page.getByRole('button',{name:/设备甲 系统甲/}).click();
    await page.getByRole('button',{name:/端口资料 \/ 端口01 \/ 配置值 · 22136/}).click();
    assert.equal(await page.getByLabel('字段0',{exact:true}).inputValue(),'5678');
    assert.deepEqual(errors, []);
    await page.locator('[role="dialog"]').waitFor({state:'hidden'});
    fs.mkdirSync('artifacts/value-source', { recursive: true });
    await page.screenshot({ path: 'artifacts/value-source/editor-light.png' });
    await page.evaluate(() => document.documentElement.classList.add('dark'));
    await page.screenshot({ path: 'artifacts/value-source/editor-dark.png' });
    console.log('PASS: readable sources; unchanged legacy widths; fixed/captured/custom/dictionary round trips; all-system devices; search and property selection; encoding and overflow checks; no page errors');
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
