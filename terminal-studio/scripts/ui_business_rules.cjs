const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
(async () => {
  const browser = await chromium.launch({ channel: 'chrome', headless: true });
  const page = await browser.newPage({ viewport: { width: 1280, height: 900 } });
  const errors = []; page.on('pageerror', e => errors.push(e.message));
  await page.route('**/__business_fixture', route => route.fulfill({ contentType: 'text/html', body: `<!doctype html><html><head><meta charset="utf-8"></head><body><div id="root"></div><script type="module">
    import RefreshRuntime from '/@react-refresh';RefreshRuntime.injectIntoGlobalHook(window);window.$RefreshReg$=()=>{};window.$RefreshSig$=()=>type=>type;window.__vite_plugin_react_preamble_installed__=true;
    const {default:React}=await import('/node_modules/.vite/deps/react.js');const {default:ReactDOM}=await import('/node_modules/.vite/deps/react-dom_client.js');
    window.saved=null;window.__TAURI_INTERNALS__={invoke:async(command,args)=>{if(command==='list_record_types')return [{kind:'device',name:'设备档案',device_required:false,key_scope:'channel',properties:[]},{kind:'phone',name:'电话资料',device_required:true,key_scope:'device',properties:['电话号码']}];if(command==='list_save_rules')return window.saved?[window.saved]:[];if(command==='save_business_rule'){window.saved=args.rule;return;}throw new Error(command);}};
    const fields=[{name:'流水号',len:2,enc:'hex',value:''},{name:'条目数',len:1,enc:'hex',role:'device_count',value:''},{name:'资料',len:4,enc:'hex',role:'device_list',value:'',unit_fields:[{name:'编号',len:1,enc:'hex',value:''},{name:'所属设备',len:1,enc:'hex',value:''},{name:'电话号码',len:2,enc:'hex',value:''}]}];
    const request={protocol_id:'q',name:'电话配置',msg_type:'Request',system_ref:1,def_mode:'fields',fields_json:JSON.stringify(fields)};
    const response={...request,protocol_id:'a',name:'电话配置应答',msg_type:'Response',fields_json:JSON.stringify([{name:'流水号',len:2,enc:'hex',value:''},{name:'结果',len:1,enc:'hex',value:''}])};
    await import('/src/styles/globals.css');const {BusinessRuleEditor}=await import('/src/components/protocols/BusinessRuleEditor.tsx');
    const root=ReactDOM.createRoot(document.getElementById('root'));window.mount=()=>root.render(React.createElement(BusinessRuleEditor,{key:Date.now(),protocol:request,protocols:[request,response],groups:[],onClose:()=>{window.closed=true;}}));window.mount();
  </script></body></html>` }));
  try {
    await page.goto('http://127.0.0.1:1426/__business_fixture');
    await page.getByRole('checkbox').check();
    async function choose(label, name) { await page.getByRole('combobox', { name: label, exact: true }).click(); await page.getByRole('option', { name, exact: true }).click(); }
    await choose('应答协议', '电话配置应答'); await choose('保存到', '电话资料');
    await choose('请求流水号', '流水号 · 2 B'); await choose('应答流水号', '流水号 · 2 B');
    await choose('请求资料', '资料'); await choose('成功结果字段', '结果 · 1 B');
    await page.getByLabel('成功值', { exact: true }).fill('AA');
    await choose('唯一标识（请求字段）', '条目 / 电话号码 · 2 B');
    await choose('所属设备标识（必选）', '条目 / 所属设备 · 1 B');
    await page.getByRole('button', { name: '添加属性' }).click();
    await choose('属性名称 1', '电话号码');
    await choose('字段 1', '条目 / 电话号码 · 2 B');
    await page.getByRole('button', { name: '保存规则', exact: true }).click();
    const rule=await page.evaluate(()=>window.saved);
    assert.equal(rule.kind,'phone');assert.equal(rule.key_scope,'device');assert.equal(rule.device_field,'item.所属设备');assert.equal(rule.success_hex,'AA');assert.equal(rule.serial,false);
    assert.deepEqual(rule.mappings,[{property:'电话号码',source:'request',field:'item.电话号码'}]);
    await page.evaluate(()=>window.mount());
    await page.getByRole('combobox',{name:'保存到',exact:true}).filter({hasText:'电话资料'}).waitFor();
    assert.equal(await page.getByLabel('成功值',{exact:true}).inputValue(),'AA');
    await choose('对应方式','无流水号 · 单请求模式');
    await page.getByText(/无法区分上一笔迟到/).waitFor();
    fs.mkdirSync('artifacts/business-save',{recursive:true});
    await page.waitForTimeout(250);
    await page.screenshot({path:'artifacts/business-save/rule-editor.png',fullPage:true});
    await page.getByRole('checkbox').uncheck();await page.getByRole('button',{name:'保存规则',exact:true}).click();
    assert.equal(await page.evaluate(()=>window.saved),null);assert.deepEqual(errors,[]);
    console.log('PASS: phone mapping, explicit success, rule reload, serial warning, disable');
  } finally { await browser.close(); }
})().catch(e=>{console.error(e);process.exit(1);});
