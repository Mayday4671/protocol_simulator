// Run against `vite preview --host 127.0.0.1 --port 1422`.
// Python Playwright is unavailable on this host; reuse its installed Node runtime.
// Only the native IPC bridge is stubbed. Every protocol/dictionary comes from
// the delivered dataset's actual SQLite import, never a simplified UI fixture.
const { DatabaseSync } = require('node:sqlite');
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const path = require('node:path');

const root = path.resolve(__dirname, '..');
const db = new DatabaseSync(path.join(root, 'artifacts/iot-dataset-validation.db'), { readOnly: true });
const systems = db.prepare('SELECT * FROM protocol_systems ORDER BY id').all();
const groups = db.prepare('SELECT * FROM protocol_groups ORDER BY id').all();
const protocols = db.prepare('SELECT p.*,s.name system_name,g.name group_name FROM protocols p JOIN protocol_systems s ON s.id=p.system_ref JOIN protocol_groups g ON g.id=p.group_ref ORDER BY p.id').all();
const dictionaries = db.prepare('SELECT d.*,g.name group_name,s.name system_name FROM dict_sets d LEFT JOIN protocol_groups g ON g.id=d.group_ref LEFT JOIN protocol_systems s ON s.id=g.system_ref ORDER BY d.id').all();
for (const d of dictionaries) d.items = db.prepare('SELECT * FROM dict_items WHERE set_ref=? ORDER BY sort_order,id').all(d.id);
db.close();
assert.equal(protocols.length, 54);
assert.equal(dictionaries.length, 130);

(async () => {
  const browser = await chromium.launch({ channel: 'chrome', headless: true });
  const page = await browser.newPage({ viewport: { width: 1600, height: 1100 } });
  const errors = [];
  page.on('pageerror', e => errors.push(String(e)));
  await page.addInitScript(data => {
    window.calls = [];
    window.__TAURI_INTERNALS__ = {
      transformCallback: () => 1, unregisterCallback: () => {},
      invoke: async (cmd, args) => {
        window.calls.push({ cmd, args });
        if (cmd === 'list_protocols') return data.protocols;
        if (cmd === 'list_dict_sets') return data.dictionaries;
        if (cmd === 'list_systems') return data.systems;
        if (cmd === 'list_groups') return data.groups;
        if (cmd === 'list_nodes') return [{ id: 90001, name: '仅页面验收通道', role: 'simulator', local_ip: '127.0.0.1', local_port: 31991, remote_ip: '127.0.0.1', remote_port: 31992, node_id_hex: '01000001', group_bind_json: JSON.stringify(data.groups.map(g => g.id)), enabled: true }];
        if (cmd === 'engine_status') return { listening_nodes: [], heartbeats: [], periodic_sends: [] };
        if (cmd === 'log_stats') return { total: 0, in_count: 0, out_count: 0, error_count: 0 };
        if (cmd === 'send_udp_message') return null; // Inspect submitted parameters; Rust harness tests actual UDP.
        if (/^(upsert|delete|import|start)_/.test(cmd)) throw new Error('Unexpected mutation in read-only UI acceptance: ' + cmd);
        return [];
      },
    };
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
  }, { systems, groups, protocols, dictionaries });
  async function choose(index, name) {
    await page.getByRole('combobox').nth(index).click();
    await page.getByRole('option', { name, exact: true }).click();
  }
  async function selectDictionaryField(name, value) {
    await page.locator(`label[for="f-${name}"]`).locator('..').getByRole('combobox').click();
    await page.getByRole('option').filter({ hasText: new RegExp('^' + value + '\\s') }).click();
  }
  try {
    await page.goto('http://127.0.0.1:1422/console');
    await page.waitForLoadState('networkidle');
    await page.screenshot({ path: path.join(root, 'artifacts/iot-ui-initial.png') });
    console.log('Initial UI:', (await page.locator('body').innerText()).slice(0, 260));
    const families = [
      ['环境采集协议', ['测点', '环境阈值', '联动策略']],
      ['门禁安防协议', ['门锁', '凭证', '通行规则']],
      ['边缘网关协议', ['下挂设备', '路由', '转发策略']],
    ];
    for (const [name, branches] of families) {
      await choose(0, name);
      await choose(1, '参数配置');
      await choose(2, '批量配置请求');
      for (const [i, prefix] of branches.entries()) {
        await selectDictionaryField('业务模式', `0${i + 1}`);
        const count = [3, 4, 8][i];
        await page.getByLabel(`${prefix}列表 第${count}条 设备编号`, { exact: true }).waitFor();
        assert.equal(await page.locator(`[id="f-${prefix}数量"]`).inputValue(), `0${count}`);
        assert.equal(await page.locator('[data-branch-group="业务模式"]').count(), 1);
        assert.equal(await page.locator('[data-branch-group]').count(), 2, 'one parent branch and only its active nested branch');
        for (const other of branches.filter(p => p !== prefix)) assert.equal(await page.locator(`[id="f-${other}数量"]`).count(), 0);
        await page.getByLabel('扩展列表 第2条 参数编码', { exact: true }).waitFor();
        await page.getByLabel(`${prefix}列表 第1条 设备编号`, { exact: true }).fill('010000A1');
        await page.getByRole('button', { name: /^收起分支组：业务模式 = / }).click();
        assert.equal(await page.getByLabel(`${prefix}列表 第1条 设备编号`, { exact: true }).isVisible(), false);
        await page.getByRole('button', { name: /^展开分支组：业务模式 = / }).click();
        assert.equal(await page.getByLabel(`${prefix}列表 第1条 设备编号`, { exact: true }).inputValue(), '010000A1');
        await selectDictionaryField(`${prefix}扩展模式`, '02');
        await page.locator(`[id="f-${prefix}备用地址"]`).waitFor();
        assert.equal(await page.locator(`[id="f-${prefix}高级阈值"]`).count(), 0);
        await selectDictionaryField(`${prefix}扩展模式`, '03');
        await page.locator(`[id="f-${prefix}离线容量"]`).waitFor();
        await page.locator(`[id="f-${prefix}数量"]`).fill('00');
        await page.getByLabel(`${prefix}列表 第1条 设备编号`, { exact: true }).waitFor({ state: 'detached' });
        assert.equal(await page.getByLabel('扩展列表 第2条 参数编码', { exact: true }).count(), 1);
        await page.locator(`[id="f-${prefix}数量"]`).fill(`0${count}`);
      }
      await page.screenshot({ path: path.join(root, `artifacts/iot-ui-${name}.png`), fullPage: true });
      console.log('PASS UI family', name, '3 branches / nested choices / defaults 3,4,8 / shared list 2 / zero count / collapse preserves input');
    }
    await page.goto('http://127.0.0.1:1422/dict');
    await page.waitForLoadState('networkidle');
    const tree = page.getByRole('navigation', { name: '字典树' });
    await tree.getByText('环境采集协议', { exact: true }).click();
    const systemBlock = tree.getByRole('button', { name: '收起系统：环境采集协议' }).locator('../..');
    await systemBlock.getByRole('button', { name: /^参数配置/ }).click();
    assert.equal(await tree.getByText('错误编码', { exact: true }).count(), 0, 'dictionary names do not appear as third tree level');
    const seen = new Set();
    let pageCount = 0;
    for (;;) {
      pageCount++;
      const panels = page.locator('[id^="dict-panel-"]');
      for (const id of await panels.evaluateAll(elements => elements.map(e => e.id))) seen.add(id);
      const size = await panels.first().locator('..').evaluate(el => ({ scroll: el.scrollHeight, client: el.clientHeight }));
      assert.ok(size.scroll <= size.client + 1, 'collapsed dictionary pages fit the available height');
      if (await page.getByRole('button', { name: '下一页字典', exact: true }).isDisabled()) break;
      await page.getByRole('button', { name: '下一页字典', exact: true }).click();
    }
    assert.equal(seen.size, 14);
    for (let i = 1; i < pageCount; i++) await page.getByRole('button', { name: '上一页字典', exact: true }).click();
    const gid = groups.find(g => g.system_ref === systems.find(s => s.name === '环境采集协议').id && g.name === '参数配置').id;
    const d = dictionaries.find(d => d.group_ref === gid && d.key === 'error_code');
    const panel = page.locator('#dict-panel-' + d.id);
    while (await panel.count() === 0) await page.getByRole('button', { name: '下一页字典', exact: true }).click();
    await panel.getByRole('button', { name: '展开字典：错误编码', exact: true }).click();
    assert.equal(await panel.locator('tbody tr').count(), 20);
    const first = await panel.locator('tbody tr').first().innerText();
    await panel.getByRole('button', { name: '下一页', exact: true }).click();
    assert.notEqual(await panel.locator('tbody tr').first().innerText(), first);
    for (let i = 0; i < 3; i++) await panel.getByRole('button', { name: '下一页', exact: true }).click();
    assert.equal(await panel.locator('tbody tr').count(), 16);
    assert.equal(await panel.getByRole('button', { name: '下一页', exact: true }).isDisabled(), true);
    await page.screenshot({ path: path.join(root, 'artifacts/iot-ui-dictionary.png'), fullPage: true });
    assert.deepEqual(errors, []);
    console.log('PASS UI dictionary: two-level tree, 14 sets adaptively paginated without collapsed overflow, 96 entries paginated 20+20+20+20+16; zero page errors');
  } catch (e) {
    console.error((await page.locator('body').innerText()).slice(-12000));
    await page.screenshot({ path: path.join(root, 'artifacts/iot-ui-failure.png'), fullPage: true });
    throw e;
  } finally {
    await browser.close();
  }
})().catch(e => { console.error(e); process.exitCode = 1; });
