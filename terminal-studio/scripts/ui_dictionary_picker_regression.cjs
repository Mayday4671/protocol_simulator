// Real workspace and forms with isolated IPC persistence. No user database or UDP changes.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

(async () => {
  const browser = await chromium.launch({ channel: 'chrome', headless: true });
  const page = await browser.newPage({ viewport: { width: 1600, height: 1060 } });
  page.setDefaultTimeout(10000);
  const errors = [];
  page.on('pageerror', error => errors.push(String(error)));
  const out = process.env.UI_ARTIFACT_DIR || path.resolve('artifacts/dictionary-picker');
  fs.mkdirSync(out, { recursive: true });
  await page.addInitScript(() => {
    const field = (name, len, value, extra = {}) => ({ name, len, enc: 'hex', value, role: 'none', ...extra });
    const groups = [
      { id: 1, system_ref: 1, name: '设备配置', header_enabled: false, header_profiles_json: JSON.stringify([{ id: 'head', name: '公共头', fields: [field('头部类型', 1, '01')] }]) },
      { id: 2, system_ref: 1, name: '远端控制', header_enabled: false },
    ];
    const dictionary = (id, key, name, group_ref, byte_len, value, label) => ({ id, key, name, group_ref, system_ref: group_ref ? 1 : null, system_name: group_ref ? '物联网' : null, group_name: groups.find(g => g.id === group_ref)?.name ?? null, byte_len, description: null, created_at: '', items: [{ id: id * 10, set_ref: id, value, label, sort_order: 0 }] });
    let dictionaries = [
      dictionary(1, 'scope_mode', '全局模式', null, 1, 'EE', '全局默认'),
      dictionary(2, 'scope_mode', '本组模式', 1, 2, 'AABB', '本组默认'),
      dictionary(3, 'allow_result', '许可结果', 1, 1, 'AA', '允许接入'),
      dictionary(4, 'connect_result', '许可结果', null, 1, 'BB', '连接完成'),
      dictionary(5, 'remote_only', '远端专属状态', 2, 1, 'CC', '远端故障'),
    ];
    let protocol = { id: 1, protocol_id: 'dictionary-test', name: '字典绑定回归', def_mode: 'fields', format: 'hex', msg_type: 'Request', fields_json: JSON.stringify([
      field('联网权限', 1, '00'),
      field('黑名单', 1, '00'),
      field('设备列表', 1, '', { role: 'device_list', unit_fields: [field('子状态', 1, '00')] }),
    ]), header_refs_json: '[]', system_ref: 1, group_ref: 1, system_name: '物联网', group_name: '设备配置', respond_delay_ms: 0, auto_periodic_interval_ms: 1000, match_fields_json: null, payload_template: '' };
    let nextSet = 10;
    let nextItem = 100;
    window.__dictionaryCalls = [];
    window.__TAURI_INTERNALS__ = { transformCallback: () => 1, unregisterCallback: () => {}, invoke: async (command, args) => {
      window.__dictionaryCalls.push({ command, args: structuredClone(args) });
      if (command === 'list_protocols') return [structuredClone(protocol)];
      if (command === 'list_systems') return [{ id: 1, name: '物联网' }];
      if (command === 'list_groups') return structuredClone(groups);
      if (command === 'list_dict_sets') return structuredClone(dictionaries);
      if (command === 'upsert_dict_set') {
        const existing = dictionaries.find(d => d.id === args.id);
        const id = existing?.id ?? nextSet++;
        const payload = args.payload;
        const saved = { ...payload, id, key: payload.key || `created_dict_${id}`, system_name: payload.group_ref ? '物联网' : null, group_name: groups.find(g => g.id === payload.group_ref)?.name ?? null, created_at: '', items: existing?.items ?? [] };
        dictionaries = [...dictionaries.filter(d => d.id !== id), saved];
        return structuredClone(saved);
      }
      if (command === 'upsert_dict_item') {
        const saved = { ...args.payload, id: args.id ?? nextItem++ };
        const owner = dictionaries.find(d => d.id === saved.set_ref);
        owner.items = [...owner.items.filter(i => i.id !== saved.id), saved].sort((a, b) => a.sort_order - b.sort_order);
        return structuredClone(saved);
      }
      if (command === 'upsert_protocol') {
        protocol = { ...protocol, ...args.payload, fields_json: JSON.stringify(args.payload.fields), header_refs_json: JSON.stringify(args.payload.header_refs) };
        return structuredClone(protocol);
      }
      if (command === 'update_group_headers') {
        const group = groups.find(g => g.id === args.id);
        group.header_profiles_json = JSON.stringify(args.payload.headers);
        return structuredClone(group);
      }
      if (command === 'engine_status') return { listening_nodes: [], heartbeats: [], periodic_sends: [] };
      if (command === 'log_stats') return { total: 0, in_count: 0, out_count: 0, error_count: 0 };
      if (['list_nodes', 'query_logs', 'list_heartbeats', 'list_terminals', 'list_flows', 'dict_set_refs'].includes(command)) return [];
      return null;
    } };
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
  });
  const navigate = name => page.getByRole('link', { name, exact: true }).click();
  const editor = () => page.getByRole('dialog', { name: '协议配置', exact: true });
  const row = index => editor().locator(`[data-field-index="${index}"]`);
  const picker = name => page.getByRole('combobox', { name: `字典：${name}`, exact: true });
  const pickerPanel = () => page.locator('[data-radix-popper-content-wrapper] [data-state="open"]:has(input[aria-label="搜索字典"])');
  const search = () => pickerPanel().getByRole('textbox', { name: '搜索字典', exact: true });
  const option = text => pickerPanel().getByRole('option').filter({ hasText: text });
  const saved = () => page.evaluate(() => window.__dictionaryCalls.filter(c => c.command === 'upsert_protocol').at(-1)?.args.payload);
  const addItem = async (name, value, label) => {
    const card = page.locator('section').filter({ has: page.getByRole('button', { name: new RegExp(`^(展开|折叠)字典：${name}$`) }) });
    await card.getByRole('button', { name: '添加条目', exact: true }).click();
    const modal = page.getByRole('dialog', { name: '添加条目', exact: true });
    await modal.locator('#di-value').fill(value);
    await modal.locator('#di-label').fill(label);
    await modal.getByRole('button', { name: '保存', exact: true }).click();
    await modal.waitFor({ state: 'hidden' });
  };
  const createDictionary = async (name, length, local) => {
    await page.getByRole('button', { name: '新增字典', exact: true }).click();
    const modal = page.getByRole('dialog', { name: '新增字典', exact: true });
    await modal.locator('#ds-name').fill(name);
    await modal.locator('#ds-len').fill(String(length));
    await modal.locator('#ds-scope').click();
    await page.getByRole('option', { name: local ? '物联网' : '全局共享', exact: true }).click();
    if (local) {
      await modal.locator('#ds-group').click();
      await page.getByRole('option', { name: '设备配置', exact: true }).click();
    }
    await modal.getByRole('button', { name: '保存', exact: true }).click();
    await modal.waitFor({ state: 'hidden' });
  };
  try {
    await page.goto((process.env.UI_BASE_URL || 'http://127.0.0.1:1422') + '/protocols');
    await page.getByRole('row').filter({ hasText: '字典绑定回归' }).getByTitle('编辑', { exact: true }).click();
    await editor().locator('#p-name').fill('字典绑定未保存草稿');
    await navigate('字典管理');
    await createDictionary('新联网权限', 2, true);
    await addItem('新联网权限', '00A1', '允许联网');
    await createDictionary('新黑名单开关', 1, false);
    await addItem('新黑名单开关', '01', '加入黑名单');
    await navigate('协议管理');
    assert.equal(await editor().locator('#p-name').inputValue(), '字典绑定未保存草稿', 'catalog refresh must retain the unsaved protocol form');

    await picker('联网权限').click();
    await search().fill('允许联网');
    await option('新联网权限').click();
    assert.equal(await row(0).getByPlaceholder('长度', { exact: true }).inputValue(), '2', 'dictionary binding aligns the field byte length');
    assert.match(await row(0).innerText(), /00A1.*允许联网/);

    await picker('黑名单').click();
    await search().fill('新黑名单');
    await search().dispatchEvent('keydown', { key: 'Enter', code: 'Enter', keyCode: 229, isComposing: true, bubbles: true });
    assert.equal(await search().isVisible(), true, 'IME composition Enter must not select a dictionary');
    assert.match(await picker('黑名单').innerText(), /不绑定/);
    await search().press('ArrowDown');
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('Enter');
    await search().waitFor({ state: 'hidden' });
    assert.match(await row(1).innerText(), /01.*加入黑名单/, 'keyboard selection applies the default item');

    // A later item edit must refresh an already mounted protocol tab as well.
    await navigate('字典管理');
    await addItem('新黑名单开关', '02', '移出黑名单');
    await navigate('协议管理');
    await picker('黑名单').click();
    await search().fill('移出黑名单');
    assert.equal(await option('新黑名单开关').count(), 1, 'new item meaning must be searchable without reopening the protocol draft');
    await search().press('Escape');
    await search().waitFor({ state: 'hidden' });
    assert.equal(await editor().isVisible(), true, 'Escape closes only the picker, not the protocol editor');

    await picker('联网权限').click();
    await search().fill('scope_mode');
    assert.equal(await option('本组模式').count(), 1);
    assert.equal(await option('全局模式').count(), 0, 'same-key local definition replaces its global fallback');
    await option('本组模式').click();
    assert.equal(await row(0).getByPlaceholder('长度', { exact: true }).inputValue(), '2');
    assert.match(await row(0).innerText(), /AABB.*本组默认/);

    await picker('联网权限').click();
    assert.equal(await option('远端专属状态').count(), 0, 'other groups are collapsed by default');
    await search().fill('远端故障');
    const foreign = option('远端专属状态');
    assert.equal(await foreign.count(), 1);
    assert.equal(await foreign.isDisabled(), true, 'a dictionary from another group cannot be bound');
    assert.match(await foreign.innerText(), /远端控制/);
    await search().fill('许可结果');
    const sameName = pickerPanel().getByRole('option').filter({ hasText: '许可结果' });
    assert.equal(await sameName.count(), 2);
    assert.match(await sameName.filter({ hasText: 'allow_result' }).innerText(), /允许接入/);
    assert.match(await sameName.filter({ hasText: 'connect_result' }).innerText(), /连接完成/);
    await page.waitForFunction(() => {
      const panel = document.querySelector('[data-radix-popper-content-wrapper] [data-state="open"]:has(input[aria-label="搜索字典"])');
      return panel && getComputedStyle(panel).opacity === '1';
    });
    await page.screenshot({ path: path.join(out, 'dictionary-options.png'), fullPage: true });
    await search().press('Escape');
    await page.getByTitle('切换到明亮模式', { exact: true }).click();
    await page.setViewportSize({ width: 1280, height: 900 });
    await picker('联网权限').click();
    await search().fill('许可结果');
    await page.waitForFunction(() => {
      const panel = document.querySelector('[data-radix-popper-content-wrapper] [data-state="open"]:has(input[aria-label="搜索字典"])');
      return panel && getComputedStyle(panel).opacity === '1';
    });
    const bounds = await pickerPanel().boundingBox();
    assert.ok(bounds.x >= 0 && bounds.y >= 0 && bounds.x + bounds.width <= 1280 && bounds.y + bounds.height <= 900, 'narrow-window dictionary popup stays within the viewport');
    await page.screenshot({ path: path.join(out, 'dictionary-options-light-1280.png'), fullPage: true });
    await search().fill('connect_result');
    await option('连接完成').click();
    assert.equal(await row(0).getByPlaceholder('长度', { exact: true }).inputValue(), '1');
    assert.match(await row(0).innerText(), /BB.*连接完成/);
    await picker('联网权限').click();
    await page.getByRole('option', { name: '不绑定', exact: true }).click();
    assert.match(await picker('联网权限').innerText(), /不绑定/);

    // Rebind after clearing, then prove persisted payload contains actual catalog keys.
    await picker('联网权限').click();
    await search().fill('新联网权限');
    await option('新联网权限').click();
    await page.getByRole('combobox', { name: '设备列表 第1个子字段 字典', exact: true }).click();
    await search().fill('新联网权限');
    await option('新联网权限').click();
    assert.equal(await page.getByRole('spinbutton', { name: '设备列表 第1个子字段 长度', exact: true }).inputValue(), '2');
    await editor().getByRole('button', { name: '保存', exact: true }).click();
    await editor().waitFor({ state: 'hidden' });
    const payload = await saved();
    assert.equal(payload.name, '字典绑定未保存草稿');
    assert.deepEqual(payload.fields.slice(0, 2).map(f => [f.dict, f.len, f.value]), [['created_dict_10', 2, '00A1'], ['created_dict_11', 1, '01']]);
    assert.deepEqual(payload.fields[2].unit_fields.map(f => [f.dict, f.len, f.value]), [['created_dict_10', 2, '00A1']], 'array unit dictionaries align both length and default value');

    await page.getByRole('button', { name: '设备配置', exact: true }).click();
    await page.getByRole('button', { name: '公共头 1 套', exact: true }).click();
    await page.getByRole('combobox', { name: '公共头字典：头部类型', exact: true }).click();
    await search().fill('新联网权限');
    await option('新联网权限').click();
    assert.equal(await page.getByRole('spinbutton', { name: '公共头第1个字段长度', exact: true }).inputValue(), '2');
    await page.getByRole('button', { name: '保存公共头', exact: true }).click();
    const header = await page.evaluate(() => window.__dictionaryCalls.filter(c => c.command === 'update_group_headers').at(-1)?.args.payload.headers[0].fields[0]);
    assert.deepEqual([header.dict, header.len, header.value], ['created_dict_10', 2, '00A1']);
    assert.equal(await page.evaluate(() => window.__dictionaryCalls.filter(c => c.command === 'upsert_dict_set').length), 2, 'both new dictionaries were created through the UI');
    assert.equal(await page.evaluate(() => window.__dictionaryCalls.filter(c => c.command === 'upsert_dict_item').length), 3, 'all new items were created through the UI');
    assert.deepEqual(errors, []);
    console.log('PASS: live dictionary and item creation, preserved protocol draft, scoped precedence, duplicate names and previews, disabled foreign scope, search, Escape, keyboard selection, clear, ordinary/array/header byte length, default value and saved bindings');
  } finally {
    await browser.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
