// Isolated ProtocolManager and FieldBranches fixtures; no real Tauri database or UDP.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

(async () => {
  const browser = await chromium.launch({ channel: 'chrome', headless: true });
  const page = await browser.newPage({ viewport: { width: 1600, height: 1100 } });
  page.setDefaultTimeout(10000);
  const errors = [];
  page.on('pageerror', error => errors.push(String(error)));
  const output = process.env.UI_ARTIFACT_DIR || path.resolve('artifacts/protocol-structure');
  fs.mkdirSync(output, { recursive: true });
  await page.addInitScript(() => {
    const f = (name, value, when, extra = {}) => ({ name, len: 1, enc: 'hex', value, role: 'none', ...(when ? { when } : {}), ...extra });
    const mode = value => ({ field: 'mode', value });
    const units = [f('entry_id', '01020304', null, { len: 4 }), f('entry_state', '01')];
    const definitions = {
      1: [f('mode', '01'), f('target01', 'A1', mode('01')), f('target02', 'A2', mode('02')), f('version', '10'), f('reserved', '00'), f('array01', 'AB'.repeat(50), mode('01'), { len: 5, role: 'device_list', unit_fields: structuredClone(units) }), f('array02', 'CD'.repeat(50), mode('02'), { len: 5, role: 'device_list', unit_fields: structuredClone(units) }), f('wire_tail', 'EE')],
      2: [f('mode', '01'), f('alias_a', 'A1', mode('01')), f('alias_b', 'A2', mode('0 1')), f('common', '10'), f('alias_c', 'A3', mode('01')), f('wire_tail', 'EE')],
      3: [f('mode', '01'), f('target01', 'AA', mode('01')), f('version', '10'), f('nested', 'BB', { field: 'target01', value: 'AA' }), f('wire_tail', 'EE')],
    };
    window.__structureOriginal = structuredClone(definitions);
    window.__structureProtocols = Object.fromEntries(Object.entries(definitions).map(([id, fields]) => [id, {
      id: Number(id), protocol_id: `structure-${id}`, name: `结构验收 ${id}`, def_mode: 'fields', format: 'hex', msg_type: 'Request',
      fields_json: JSON.stringify(fields), payload_template: '', system_ref: 1, group_ref: 1, system_name: '测试系统', group_name: '配置',
      respond_delay_ms: 0, auto_periodic_interval_ms: 1000, match_fields_json: null,
    }]));
    window.__structureId = 1;
    window.__structureCalls = [];
    window.__TAURI_INTERNALS__ = { invoke: async (command, args) => {
      window.__structureCalls.push({ command, args: structuredClone(args) });
      if (command === 'list_protocols') return [structuredClone(window.__structureProtocols[window.__structureId])];
      if (command === 'list_systems') return [{ id: 1, name: '测试系统' }];
      if (command === 'list_groups') return [{ id: 1, system_ref: 1, name: '配置', header_enabled: false, header_fields_json: null }];
      if (command === 'list_dict_sets') return [];
      if (command === 'upsert_protocol') {
        const protocol = window.__structureProtocols[window.__structureId];
        Object.assign(protocol, args.payload, { fields_json: JSON.stringify(args.payload.fields) });
        return structuredClone(protocol);
      }
      throw new Error(`Unexpected fixture command: ${command}`);
    } };
  });
  await page.route('**/__protocol_structure_fixture', route => route.fulfill({ contentType: 'text/html', body: `<!doctype html><html><head><meta charset="UTF-8"></head><body><div id="root"></div><script type="module">
    import RefreshRuntime from '/@react-refresh';
    RefreshRuntime.injectIntoGlobalHook(window); window.$RefreshReg$ = () => {}; window.$RefreshSig$ = () => type => type; window.__vite_plugin_react_preamble_installed__ = true;
    const {default: React} = await import('/node_modules/.vite/deps/react.js');
    const {default: ReactDOM} = await import('/node_modules/.vite/deps/react-dom_client.js');
    await import('/src/styles/globals.css');
    const {ProtocolManager} = await import('/src/pages/ProtocolManager.tsx');
    const {FieldBranches} = await import('/src/components/protocols/FieldBranches.tsx');
    const {fieldIsActive} = await import('/src/lib/deviceProfiles.ts');
    function Fixture() {
      const [state, setState] = React.useState({id: 1, view: 'editor'});
      window.__setStructureFixture = (id, view = 'editor') => { window.__structureId = id; setState({id, view}); };
      const fields = JSON.parse(window.__structureProtocols[state.id].fields_json);
      const values = Object.fromEntries(fields.map(field => [field.name, field.value]));
      return React.createElement('main', {'data-fixture-id': state.id, 'data-fixture-view': state.view, style: {height: '100vh'}}, state.view === 'editor'
        ? React.createElement(ProtocolManager, {key: state.id})
        : React.createElement(FieldBranches, {fields: fields.map(field => ({...field, whenField: field.when?.field, whenValue: field.when?.value})), activeWhen: index => fieldIsActive(fields[index], fields, values, []), renderField: index => React.createElement('div', {'data-runtime-field': fields[index].name}, fields[index].name)}));
    }
    ReactDOM.createRoot(document.getElementById('root')).render(React.createElement(Fixture));
  </script></body></html>` }));
  const order = () => page.getByPlaceholder('字段名', { exact: true }).evaluateAll(inputs => inputs.map(input => input.value));
  const branch = (value, index = 0) => page.locator(`[data-branch-group="mode"][data-branch-value="${value}"]`).nth(index);
  const switchFixture = async (id, view = 'editor') => {
    await page.evaluate(({ id, view }) => window.__setStructureFixture(id, view), { id, view });
    await page.locator(`[data-fixture-id="${id}"][data-fixture-view="${view}"]`).waitFor();
    if (view === 'editor') await page.getByTitle('编辑', { exact: true }).click();
  };
  const runtimeOrder = () => page.locator('[data-runtime-field]').evaluateAll(elements => elements.map(element => element.dataset.runtimeField));
  try {
    await page.goto((process.env.UI_BASE_URL || 'http://127.0.0.1:1422') + '/__protocol_structure_fixture');
    await page.getByTitle('编辑', { exact: true }).click();
    const expectedOrder = ['mode', 'target01', 'target02', 'version', 'reserved', 'array01', 'array02', 'wire_tail'];
    assert.deepEqual(await order(), expectedOrder, 'editor must preserve wire order across common fields and separated branches');
    const arrayRow = page.locator('[data-field-index="5"]');
    const structure = arrayRow.getByRole('button', { name: '条目结构：array01', exact: true });
    const defaultData = arrayRow.getByLabel('默认条目数据（HEX）：array01', { exact: true });
    assert.equal(await structure.getAttribute('aria-expanded'), 'true', 'array structure starts expanded');
    assert.equal(await arrayRow.getByLabel('单条字节数：array01', { exact: true }).inputValue(), '5');
    assert.equal(await arrayRow.getByLabel('array01 第1个子字段 名称', { exact: true }).inputValue(), 'entry_id');
    assert.equal(await arrayRow.getByLabel('array01 第2个子字段 名称', { exact: true }).isVisible(), true);
    assert.equal(await arrayRow.locator('details').evaluate(element => element.open), false, 'raw default data starts collapsed');
    assert.equal(await defaultData.isVisible(), false);
    await arrayRow.getByLabel('展开默认条目数据：array01', { exact: true }).click();
    assert.equal(await defaultData.inputValue(), 'AB'.repeat(50), 'the full 100-character default remains editable');
    assert.equal(await defaultData.isVisible(), true);
    await arrayRow.getByLabel('array01 第1个子字段 长度', { exact: true }).fill('3');
    assert.equal(await arrayRow.getByLabel('单条字节数：array01', { exact: true }).inputValue(), '4', 'single-record size follows subfield lengths');
    assert.equal(await defaultData.inputValue(), 'AB'.repeat(50), 'editing structure cannot rewrite default bytes');
    assert.match(await structure.innerText(), /已设置默认数据/, 'partial records are not reported as a false integer count');
    await arrayRow.getByLabel('array01 第1个子字段 长度', { exact: true }).fill('4');
    assert.match(await structure.innerText(), /10 条默认数据/);
    await arrayRow.getByRole('button', { name: '添加子字段：array01', exact: true }).click();
    assert.equal(await arrayRow.getByLabel('单条字节数：array01', { exact: true }).inputValue(), '6');
    await arrayRow.getByRole('button', { name: 'array01 删除第3个子字段', exact: true }).click();
    assert.equal(await arrayRow.getByLabel('单条字节数：array01', { exact: true }).inputValue(), '5');
    assert.equal(await defaultData.inputValue(), 'AB'.repeat(50));
    assert.equal(await page.getByLabel('单条字节数：array02', { exact: true }).inputValue(), '5', 'editing one array does not change another');
    await arrayRow.getByLabel('展开默认条目数据：array01', { exact: true }).click();
    await structure.click();
    assert.equal(await structure.getAttribute('aria-expanded'), 'false');
    assert.equal(await arrayRow.getByLabel('array01 第1个子字段 名称', { exact: true }).isVisible(), false);
    await structure.click();
    assert.equal(await page.locator('[data-branch-group="mode"][data-branch-value="01"]').count(), 2, 'separated conditions form independent segments');
    assert.equal(await page.locator('[data-branch-group="mode"][data-branch-value="02"]').count(), 2);
    await branch('01').getByRole('button', { name: '收起分支组：mode = 01', exact: true }).click();
    assert.equal(await branch('01').getByRole('button', { name: '展开分支组：mode = 01', exact: true }).getAttribute('aria-expanded'), 'false');
    assert.equal(await branch('01', 1).getByRole('button', { name: '收起分支组：mode = 01', exact: true }).getAttribute('aria-expanded'), 'true');
    assert.equal(await page.locator('input[placeholder="字段名"]:visible').count(), 7);
    await branch('01').getByRole('button', { name: '展开分支组：mode = 01', exact: true }).click();
    await branch('01').getByRole('button', { name: '编辑分支条件：mode = 01', exact: true }).click();
    await branch('01').getByLabel('分支匹配值', { exact: true }).fill('03');
    await branch('01').getByRole('button', { name: '应用分支条件', exact: true }).click();
    assert.equal(await branch('03').getByPlaceholder('字段名', { exact: true }).inputValue(), 'target01');
    assert.equal(await branch('01').getByPlaceholder('字段名', { exact: true }).inputValue(), 'array01', 'editing first segment must not alter later equal conditions');
    assert.deepEqual(await order(), expectedOrder);
    await page.getByRole('button', { name: '保存', exact: true }).click();
    await page.getByRole('dialog').waitFor({ state: 'hidden' });
    const saved = await page.evaluate(() => window.__structureCalls.filter(call => call.command === 'upsert_protocol').at(-1).args.payload.fields);
    const original = await page.evaluate(() => window.__structureOriginal[1]);
    assert.deepEqual(saved.map(field => field.name), expectedOrder);
    assert.deepEqual(saved.map(field => field.value), original.map(field => field.value), 'saving must retain values including 100-character HEX defaults');
    assert.equal(saved.find(field => field.name === 'target01').when.value, '03');
    assert.equal(saved.find(field => field.name === 'array01').when.value, '01');
    for (const name of ['array01', 'array02']) {
      const array = saved.find(field => field.name === name);
      const before = original.find(field => field.name === name);
      assert.equal(array.len, 5);
      assert.equal(array.value.length, 100);
      assert.deepEqual(array.unit_fields.map(({ name, len, enc, value }) => ({ name, len, enc, value })), before.unit_fields.map(({ name, len, enc, value }) => ({ name, len, enc, value })));
    }
    await page.getByTitle('编辑', { exact: true }).click();
    assert.deepEqual(await order(), expectedOrder, 'reopening preserves the saved wire order');
    assert.equal(await branch('03').getByPlaceholder('字段名', { exact: true }).inputValue(), 'target01');
    for (const [index, name, byte] of [[5, 'array01', 'AB'], [6, 'array02', 'CD']]) {
      const row = page.locator(`[data-field-index="${index}"]`);
      assert.equal(await row.getByLabel(`单条字节数：${name}`, { exact: true }).inputValue(), '5');
      assert.equal(await row.getByLabel(`${name} 第1个子字段 名称`, { exact: true }).inputValue(), 'entry_id');
      assert.equal(await row.getByLabel(`${name} 第1个子字段 默认值`, { exact: true }).inputValue(), '01020304');
      await row.getByLabel(`展开默认条目数据：${name}`, { exact: true }).click();
      assert.equal(await row.getByLabel(`默认条目数据（HEX）：${name}`, { exact: true }).inputValue(), byte.repeat(50), 'reopening retains every default HEX character');
      await row.getByLabel(`展开默认条目数据：${name}`, { exact: true }).click();
    }
    await page.screenshot({ path: path.join(output, 'editor-ordered.png') });
    await switchFixture(2);
    assert.deepEqual(await order(), ['mode', 'alias_a', 'alias_b', 'common', 'alias_c', 'wire_tail']);
    assert.equal(await page.locator('[data-branch-group="mode"][data-branch-value="01"]').count(), 2);
    assert.deepEqual(await branch('01').getByPlaceholder('字段名', { exact: true }).evaluateAll(inputs => inputs.map(input => input.value)), ['alias_a', 'alias_b'], 'adjacent canonical 01 and 0 1 merge');
    await switchFixture(3);
    assert.deepEqual(await order(), ['mode', 'target01', 'version', 'nested', 'wire_tail'], 'dependent conditions cannot move rows before common fields');
    await switchFixture(3, 'runtime');
    assert.deepEqual(await runtimeOrder(), ['mode', 'target01', 'version', 'nested', 'wire_tail'], 'runtime also keeps nested dependencies in wire order');
    await switchFixture(2, 'runtime');
    assert.deepEqual(await runtimeOrder(), ['mode', 'alias_a', 'alias_b', 'common', 'alias_c', 'wire_tail']);
    await switchFixture(1, 'runtime');
    assert.deepEqual(await runtimeOrder(), ['mode', 'version', 'reserved', 'array01', 'wire_tail'], 'runtime keeps active segments after common fields');
    await page.screenshot({ path: path.join(output, 'runtime-ordered.png') });
    assert.deepEqual(errors, []);
    console.log('PASS: editor/runtime wire order, separated branch segments, independent collapse, local condition edits, canonical adjacent merging, nested dependency order, expanded array structure, computed record size, saved values/units/100-character HEX');
    console.log(`Screenshots: ${output}`);
  } catch (error) {
    await page.screenshot({ path: path.join(output, 'structure-failure.png') });
    console.error('Fixture field order:', await order());
    throw error;
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
