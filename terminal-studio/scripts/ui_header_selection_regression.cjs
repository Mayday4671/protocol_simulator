// Isolated UI fixtures: real React components, mocked Tauri persistence; no database or UDP.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

(async () => {
  const browser = await chromium.launch({ channel: 'chrome', headless: true });
  const page = await browser.newPage({ viewport: { width: 1580, height: 1080 } });
  page.setDefaultTimeout(8000);
  const errors = [];
  page.on('pageerror', error => errors.push(String(error)));
  const out = process.env.UI_ARTIFACT_DIR || path.resolve('artifacts/header-selection');
  fs.mkdirSync(out, { recursive: true });
  await page.addInitScript(() => {
    const field = (name, value, extra = {}) => ({ name, len: 1, enc: 'hex', value, role: 'none', ...extra });
    window.__headerGroups = [{ id: 1, system_ref: 1, name: '测试分组', header_enabled: false, header_fields_json: null, header_profiles_json: JSON.stringify([
      { id: 'head-a', name: '公共头 A', fields: [field('类型', 'A1', { profile_key: 'kindA', section: 'header', section_name: '链路头' }), field('标志', '01')] },
      { id: 'head-b', name: '公共头 B', fields: [field('类型', 'B2'), field('编号', '02')] },
    ]) }];
    window.__headerProtocol = { id: 1, protocol_id: 'header-probe', name: '头组合验收', def_mode: 'fields', format: 'hex', msg_type: 'Request', fields_json: JSON.stringify([field('净荷', 'DD')]), payload_template: '', system_ref: 1, group_ref: 1, header_refs_json: '[]', system_name: '测试系统', group_name: '测试分组', respond_delay_ms: 0, auto_periodic_interval_ms: 1000, match_fields_json: null };
    window.__headerCalls = [];
    window.__TAURI_INTERNALS__ = { invoke: async (command, args) => {
      window.__headerCalls.push({ command, args: structuredClone(args) });
      if (command === 'list_protocols') return [structuredClone(window.__headerProtocol)];
      if (command === 'list_systems') return [{ id: 1, name: '测试系统' }];
      if (command === 'list_groups') return structuredClone(window.__headerGroups);
      if (command === 'list_dict_sets') return [];
      if (command === 'upsert_protocol') {
        Object.assign(window.__headerProtocol, args.payload, { fields_json: JSON.stringify(args.payload.fields), header_refs_json: JSON.stringify(args.payload.header_refs) });
        return structuredClone(window.__headerProtocol);
      }
      if (command === 'update_group_headers') {
        window.__headerGroups[0].header_profiles_json = JSON.stringify(args.payload.headers);
        return structuredClone(window.__headerGroups[0]);
      }
      throw new Error(`Unexpected fixture command: ${command}`);
    } };
  });
  await page.route('**/__header_selection_fixture', route => route.fulfill({ contentType: 'text/html', body: `<!doctype html><html><head><meta charset="UTF-8"></head><body><div id="root"></div><script type="module">
    import RefreshRuntime from '/@react-refresh';
    RefreshRuntime.injectIntoGlobalHook(window); window.$RefreshReg$ = () => {}; window.$RefreshSig$ = () => type => type; window.__vite_plugin_react_preamble_installed__ = true;
    const {default: React} = await import('/node_modules/.vite/deps/react.js');
    const {default: ReactDOM} = await import('/node_modules/.vite/deps/react-dom_client.js');
    const {Toaster} = await import('/node_modules/.vite/deps/sonner.js');
    await import('/src/styles/globals.css');
    const {ProtocolManager} = await import('/src/pages/ProtocolManager.tsx');
    function Fixture() {
      const [revision, setRevision] = React.useState(0);
      window.__refreshHeaderFixture = () => setRevision(value => value + 1);
      return React.createElement('main', {style: {height: '100vh'}}, React.createElement(ProtocolManager, {key: revision}), React.createElement(Toaster));
    }
    ReactDOM.createRoot(document.getElementById('root')).render(React.createElement(Fixture));
  </script></body></html>` }));
  const saved = () => page.evaluate(() => window.__headerCalls.filter(call => call.command === 'upsert_protocol').at(-1)?.args.payload);
  const openEditor = () => page.getByRole('row').filter({ hasText: '头组合验收' }).getByTitle('编辑', { exact: true }).click();
  const check = (name, checked) => page.getByRole('checkbox', { name: `使用公共头：${name}`, exact: true }).setChecked(checked);
  const save = () => page.getByRole('dialog').getByRole('button', { name: '保存', exact: true }).click();
  const order = () => page.locator('[data-header-order]').evaluateAll(rows => rows.map(row => row.dataset.headerOrder));
  try {
    await page.goto((process.env.UI_BASE_URL || 'http://127.0.0.1:1422') + '/__header_selection_fixture');
    await openEditor();
    assert.equal(await page.getByRole('checkbox', { name: '使用公共头：公共头 A' }).isChecked(), false);
    assert.equal(await page.getByRole('checkbox', { name: '使用公共头：公共头 B' }).isChecked(), false);
    await check('公共头 A', true);
    await save();
    assert.deepEqual((await saved()).header_refs, ['head-a']);
    await openEditor();
    await check('公共头 A', false);
    await check('公共头 B', true);
    await save();
    assert.deepEqual((await saved()).header_refs, ['head-b']);
    await openEditor();
    await check('公共头 A', true);
    assert.deepEqual(await order(), ['head-b', 'head-a']);
    await page.getByRole('button', { name: '前移公共头：公共头 A' }).click();
    assert.deepEqual(await order(), ['head-a', 'head-b']);
    await save();
    assert.deepEqual((await saved()).header_refs, ['head-a', 'head-b']);
    await openEditor();
    await page.getByRole('button', { name: '前移公共头：公共头 B' }).click();
    await save();
    assert.deepEqual((await saved()).header_refs, ['head-b', 'head-a']);
    await openEditor();
    await page.getByRole('button', { name: '全部取消', exact: true }).click();
    await save();
    assert.deepEqual((await saved()).header_refs, [], 'empty selection stays explicit, not null legacy inheritance');

    // Same-named fields in two heads require a qualified target; dropping that head blocks save.
    await page.evaluate(() => {
      window.__headerProtocol.header_refs_json = '["head-a","head-b"]';
      window.__headerProtocol.fields_json = JSON.stringify([{ name: '类型取值', len: 1, enc: 'hex', value: 'C3', role: 'none', hdr_ref: '类型' }]);
      window.__refreshHeaderFixture();
    });
    await openEditor();
    assert.match(await page.getByRole('alert').innerText(), /重名/);
    await page.getByRole('combobox', { name: '覆盖公共头：类型取值' }).click();
    await page.getByRole('option', { name: '公共头 B / 类型', exact: true }).click();
    assert.equal(await page.getByRole('dialog').getByRole('alert').count(), 0);
    await check('公共头 B', false);
    assert.match(await page.getByRole('dialog').getByRole('alert').innerText(), /未选用或不存在/);
    const before = await page.evaluate(() => window.__headerCalls.filter(call => call.command === 'upsert_protocol').length);
    await save();
    assert.equal(await page.evaluate(() => window.__headerCalls.filter(call => call.command === 'upsert_protocol').length), before);
    await check('公共头 B', true);
    await page.screenshot({ path: path.join(out, 'protocol-header-selection.png'), fullPage: true });
    await save();
    assert.equal((await saved()).fields[0].hdr_ref, 'head-b::类型');

    // Legacy records continue to select their original head and preserve old bare overrides.
    await page.evaluate(() => {
      const group = window.__headerGroups[0];
      group.header_profiles_json = null;
      group.header_enabled = true;
      group.header_fields_json = JSON.stringify([{ name: '命令', len: 1, enc: 'hex', value: '01', role: 'none', profile_key: 'messageCode', section: 'header_extension', section_name: '命令头' }]);
      window.__headerProtocol.header_refs_json = null;
      window.__headerProtocol.fields_json = JSON.stringify([{ name: '命令取值', len: 1, enc: 'hex', value: '03', role: 'none', hdr_ref: '命令' }]);
      window.__refreshHeaderFixture();
    });
    await openEditor();
    assert.equal(await page.getByRole('checkbox', { name: '使用公共头：测试分组 公共头' }).isChecked(), true);
    assert.match(await page.getByRole('combobox', { name: '覆盖公共头：命令取值' }).innerText(), /测试分组 公共头 \/ 命令/);
    await page.getByRole('combobox', { name: '覆盖公共头：命令取值' }).click();
    assert.equal(await page.getByRole('option', { name: '测试分组 公共头 / 命令', exact: true }).count(), 1, 'legacy alias cannot add a duplicate menu option');
    await page.keyboard.press('Escape');
    await save();
    assert.deepEqual((await saved()).header_refs, ['legacy-default']);
    assert.equal((await saved()).fields[0].hdr_ref, '命令');

    // Library editing: create, copy, rename, remove a draft, and preserve field metadata.
    await page.getByText('测试分组', { exact: true }).click();
    await page.getByRole('button', { name: /^公共头/ }).click();
    await page.getByRole('button', { name: '复制公共头：测试分组 公共头' }).click();
    await page.getByRole('textbox', { name: '公共头名称', exact: true }).fill('扩展头 C');
    await page.getByRole('textbox', { name: '公共头第1个字段名称' }).fill('扩展标志');
    await page.getByRole('textbox', { name: '公共头第1个字段值' }).fill('FE');
    await page.getByRole('button', { name: '新增公共头', exact: true }).click();
    await page.getByRole('textbox', { name: '公共头名称', exact: true }).fill('删除测试');
    await page.getByRole('button', { name: '删除公共头：删除测试' }).click();
    await page.getByRole('button', { name: '确认删除公共头', exact: true }).click();
    await page.getByRole('button', { name: '保存公共头', exact: true }).click();
    const library = await page.evaluate(() => window.__headerCalls.filter(call => call.command === 'update_group_headers').at(-1)?.args.payload.headers);
    assert.equal(library.length, 2);
    assert.equal(library[0].id, 'legacy-default');
    assert.equal(library[0].fields[0].name, '命令', 'copy edits do not change source header');
    assert.equal(library[0].fields[0].value, '01');
    assert.equal(library[1].name, '扩展头 C');
    assert.equal(library[1].fields[0].name, '扩展标志');
    assert.equal(library[1].fields[0].value, 'FE');
    assert.equal(library[1].fields[0].profile_key, 'messageCode');
    assert.equal(library[1].fields[0].section, 'header_extension');
    assert.equal(library[1].fields[0].section_name, '命令头');
    assert.notEqual(library[1].id, library[0].id);
    await page.screenshot({ path: path.join(out, 'header-library.png'), fullPage: true });

    await page.getByRole('button', { name: '编辑公共头：扩展头 C' }).click();
    await page.getByRole('textbox', { name: '公共头名称', exact: true }).fill('已保存的扩展头');
    await page.getByRole('button', { name: /^全部协议/ }).click();
    await page.getByRole('dialog', { name: '公共头尚未保存' }).getByRole('button', { name: '继续编辑', exact: true }).click();
    assert.equal(await page.getByRole('textbox', { name: '公共头名称', exact: true }).inputValue(), '已保存的扩展头');
    await page.getByRole('button', { name: /^全部协议/ }).click();
    await page.getByRole('dialog', { name: '公共头尚未保存' }).getByRole('button', { name: '保存并切换', exact: true }).click();
    await page.getByRole('dialog', { name: '公共头尚未保存' }).waitFor({ state: 'hidden' });
    assert.equal((await page.evaluate(() => JSON.parse(window.__headerGroups[0].header_profiles_json)))[1].name, '已保存的扩展头');
    await page.getByText('测试分组', { exact: true }).click();
    await page.getByRole('button', { name: /^公共头/ }).click();
    await page.getByRole('button', { name: '编辑公共头：已保存的扩展头' }).click();
    await page.getByRole('textbox', { name: '公共头名称', exact: true }).fill('将被放弃的修改');
    await page.getByRole('button', { name: /^全部协议/ }).click();
    await page.getByRole('dialog', { name: '公共头尚未保存' }).getByRole('button', { name: '放弃并切换', exact: true }).click();
    await page.getByText('测试分组', { exact: true }).click();
    await page.getByRole('button', { name: /^公共头/ }).click();
    await page.getByRole('button', { name: '编辑公共头：已保存的扩展头' }).click();
    assert.equal(await page.getByRole('textbox', { name: '公共头名称', exact: true }).inputValue(), '已保存的扩展头');

    await page.getByRole('button', { name: '新增协议', exact: true }).click();
    assert.equal(await page.getByRole('checkbox', { name: '使用公共头：测试分组 公共头' }).isChecked(), false, 'new protocol requires explicit choice even in legacy-enabled group');
    await page.getByRole('textbox', { name: '协议名称 *', exact: true }).fill('新协议显式无头');
    await save();
    assert.deepEqual((await saved()).header_refs, []);

    // A legacy header may contain repeated field names. Local qualified keys must skip literal suffix collisions.
    await page.evaluate(() => {
      const group = window.__headerGroups[0];
      group.header_profiles_json = null;
      group.header_enabled = true;
      group.header_fields_json = JSON.stringify([
        { name: '保留', len: 1, enc: 'hex', value: '00', role: 'none' },
        { name: '保留', len: 1, enc: 'hex', value: '01', role: 'none' },
        { name: '保留#2', len: 1, enc: 'hex', value: '02', role: 'none' },
      ]);
      window.__headerProtocol.name = '头组合验收';
      window.__headerProtocol.header_refs_json = null;
      window.__headerProtocol.fields_json = JSON.stringify([{ name: '保留取值', len: 1, enc: 'hex', value: '03', role: 'none', hdr_ref: '保留' }]);
      window.__refreshHeaderFixture();
    });
    await openEditor();
    assert.equal(await page.getByRole('dialog').getByRole('alert').count(), 0, 'legacy same-head duplicate bare refs retain first-field behavior');
    assert.match(await page.getByRole('combobox', { name: '覆盖公共头：保留取值' }).innerText(), /保留 #1/);
    await page.getByRole('combobox', { name: '覆盖公共头：保留取值' }).click();
    assert.equal(await page.getByRole('option').count(), 4, 'one option per physical header field plus no override');
    assert.equal(await page.getByRole('option', { name: '测试分组 公共头 / 保留 #1', exact: true }).count(), 1);
    assert.equal(await page.getByRole('option', { name: '测试分组 公共头 / 保留 #2', exact: true }).count(), 1);
    assert.equal(await page.getByRole('option', { name: '测试分组 公共头 / 保留#2', exact: true }).count(), 1);
    await page.keyboard.press('Escape');
    await save();
    assert.equal((await saved()).fields[0].hdr_ref, '保留', 'opening a legacy draft cannot silently rewrite its reference');
    await openEditor();
    await page.getByRole('combobox', { name: '覆盖公共头：保留取值' }).click();
    await page.getByRole('option', { name: '测试分组 公共头 / 保留 #2', exact: true }).click();
    await save();
    assert.equal((await saved()).fields[0].hdr_ref, 'legacy-default::保留#3', 'second physical duplicate skips a preexisting literal #2 field name');
    await page.getByText('测试分组', { exact: true }).click();
    await page.getByRole('button', { name: /^公共头/ }).click();
    await page.getByRole('textbox', { name: '公共头名称', exact: true }).fill('旧头兼容');
    await page.getByRole('button', { name: '保存公共头', exact: true }).click();
    const legacySaved = await page.evaluate(() => window.__headerCalls.filter(call => call.command === 'update_group_headers').at(-1)?.args.payload.headers);
    assert.equal(legacySaved[0].id, 'legacy-default');
    assert.deepEqual(legacySaved[0].fields.map(field => field.name), ['保留', '保留', '保留#2']);
    await page.evaluate(() => {
      const definitions = JSON.parse(window.__headerGroups[0].header_profiles_json);
      definitions.push({ id: 'another-head', name: '另一个头', fields: [{ name: '保留', len: 1, enc: 'hex', value: '04', role: 'none' }] });
      window.__headerGroups[0].header_profiles_json = JSON.stringify(definitions);
      window.__headerProtocol.header_refs_json = '["legacy-default","another-head"]';
      window.__headerProtocol.fields_json = JSON.stringify([{ name: '保留取值', len: 1, enc: 'hex', value: '03', role: 'none', hdr_ref: '保留' }]);
      window.__refreshHeaderFixture();
    });
    await openEditor();
    assert.match(await page.getByRole('dialog').getByRole('alert').innerText(), /重名/, 'cross-head duplicate bare refs stay ambiguous');
    await check('另一个头', false);
    assert.equal(await page.getByRole('dialog').getByRole('alert').count(), 0);
    await save();
    assert.deepEqual(errors, []);
    const report = { status: 'passed', scope: 'isolated React UI with mocked persistence; not a real UDP test', checks: ['A', 'B', 'A+B', 'B+A', 'no head', 'qualified duplicate-name override', 'unselected referenced head blocks save', 'legacy inheritance', 'copy/edit/delete library', 'copy preserves field metadata', 'continue/save/discard group-switch guards', 'new protocol explicit no-head selection', 'legacy duplicate field names and first-match bare references', 'qualified duplicate refs skip literal suffix collisions', 'no duplicate alias menu items', 'cross-head ambiguity remains blocked'], pageErrors: errors };
    fs.writeFileSync(path.join(out, 'ui-report.json'), JSON.stringify(report, null, 2));
    console.log(JSON.stringify(report));
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
