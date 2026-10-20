// Run with NODE_PATH pointing to the installed Playwright runtime, against
// `node node_modules/vite/bin/vite.js preview --host 127.0.0.1 --port 1422`.
// The sole database read is the isolated, generated acceptance fixture.
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
assert.equal(dictionaries.length, 130);
assert.equal(groups.length, 9);

(async () => {
  const browser = await chromium.launch({ channel: 'chrome', headless: true });
  const failures = [];
  const sizes = [];
  async function scenario(name, viewport, test) {
    const page = await browser.newPage({ viewport });
    page.setDefaultTimeout(6000);
    const errors = [];
    page.on('pageerror', error => errors.push(String(error)));
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
          if (cmd === 'engine_status') return { listening_nodes: [], running_heartbeats: [], heartbeats: [], periodic_sends: [] };
          if (cmd === 'log_stats') return { total: 0, in_count: 0, out_count: 0, error_count: 0 };
          if (/^(upsert|update|create|delete|import|start|send)_/.test(cmd)) throw new Error('Unexpected mutation in workspace UI regression: ' + cmd);
          return [];
        },
      };
      window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
    }, { systems, groups, protocols, dictionaries });
    try {
      await test(page);
      assert.deepEqual(errors, [], 'no page errors');
      console.log('PASS', name);
    } catch (error) {
      failures.push(name + ': ' + error.message);
      await page.screenshot({ path: path.join(root, 'artifacts/workspace-' + name + '-failure.png'), fullPage: true });
      console.error('FAIL', name, error.message);
    } finally {
      await page.close();
    }
  }
  const sidebar = (page, name) => page.locator('.app-sidebar').getByRole('link', { name, exact: true });
  const tab = (page, name) => page.getByRole('tab', { name, exact: true });
  const activePanel = page => page.getByRole('tabpanel');
  const selectedTab = async (page, name) => {
    await tab(page, name).waitFor();
    assert.equal(await tab(page, name).getAttribute('aria-selected'), 'true');
    assert.equal(await page.getByRole('tabpanel').count(), 1, 'only the active panel is interactive');
  };
  async function choose(page, index, name) {
    await activePanel(page).getByRole('combobox').nth(index).click();
    await page.getByRole('option', { name, exact: true }).click();
  }
  await scenario('tabs', { width: 1366, height: 768 }, async page => {
    await page.goto('http://127.0.0.1:1422/monitor');
    await page.getByPlaceholder('关键字：报文 / 协议 / 地址').waitFor();
    assert.equal(await page.getByRole('tablist', { name: '工作区标签页' }).count(), 1, 'the workspace has a visible tab bar');
    await page.getByPlaceholder('关键字：报文 / 协议 / 地址').fill('保留的筛选条件');
    await sidebar(page, '字典管理').click();
    await selectedTab(page, '字典管理');
    await page.getByPlaceholder('查询字典名 / Key / 条目').fill('错误编码');
    await sidebar(page, '实时监控').click();
    assert.equal(await page.getByPlaceholder('关键字：报文 / 协议 / 地址').inputValue(), '保留的筛选条件');
    await sidebar(page, '实时监控').click();
    assert.equal(await page.getByRole('tab').count(), 2, 'repeated menu visits reuse one tab');
    await tab(page, '字典管理').click();
    assert.equal(await page.getByPlaceholder('查询字典名 / Key / 条目').inputValue(), '错误编码');
    await page.goBack();
    await selectedTab(page, '实时监控');
    await page.goForward();
    await selectedTab(page, '字典管理');
    await tab(page, '字典管理').press('Home');
    await selectedTab(page, '实时监控');
    await tab(page, '实时监控').press('ArrowRight');
    await selectedTab(page, '字典管理');
    let prompted = false;
    page.on('dialog', async dialog => { prompted = true; await dialog.dismiss(); });
    await page.getByRole('button', { name: '关闭标签页：实时监控', exact: true }).click();
    assert.equal(prompted, false, 'read-only filtering never prompts on close');
    assert.equal(await tab(page, '实时监控').count(), 0);
    await sidebar(page, '实时监控').click();
    assert.equal(await page.getByPlaceholder('关键字：报文 / 协议 / 地址').inputValue(), '', 'closed pages reopen fresh');
    await page.getByRole('button', { name: '关闭标签页：实时监控', exact: true }).click();
    await selectedTab(page, '字典管理');
    await page.getByRole('button', { name: '关闭标签页：字典管理', exact: true }).click();
    await selectedTab(page, '总览');
    await page.screenshot({ path: path.join(root, 'artifacts/workspace-tabs.png') });
  });

  for (const viewport of [{ width: 1700, height: 1075 }, { width: 1366, height: 768 }]) {
    await scenario('dictionary-' + viewport.width, viewport, async page => {
      await page.goto('http://127.0.0.1:1422/dict');
      await page.locator('[id^="dict-panel-"]').first().waitFor();
      const panels = page.locator('[id^="dict-panel-"]');
      const initiallyExpanded = await page.getByRole('button', { name: /^折叠字典：/ }).count();
      await page.getByRole('button', { name: '收起本页', exact: true }).click();
      const list = panels.first().locator('..');
      const assertFits = async () => {
        const metrics = await list.evaluate(el => ({ client: el.clientHeight, scroll: el.scrollHeight }));
        assert.ok(metrics.scroll <= metrics.client + 1, `collapsed cards must fit without a vertical scrollbar (${metrics.scroll}/${metrics.client})`);
        const overflow = await page.locator('main:visible').evaluate(el => el.scrollHeight - el.clientHeight);
        assert.ok(overflow <= 1, 'dictionary page must fit the workspace viewport');
      };
      await assertFits();
      assert.equal(initiallyExpanded, 0, 'all dictionary cards start collapsed');
      const firstPageSize = await panels.count();
      sizes.push(firstPageSize);
      const seen = new Set();
      for (let i = 0; i < 140; i++) {
        for (const id of await panels.evaluateAll(elements => elements.map(el => el.id))) seen.add(id);
        await assertFits();
        const next = page.getByRole('button', { name: '下一页字典', exact: true });
        if (await next.isDisabled()) break;
        await next.click();
      }
      assert.equal(seen.size, 130, 'all dictionary sets remain reachable through pagination');
      const tree = page.getByRole('navigation', { name: '字典树' });
      await tree.getByText('环境采集协议', { exact: true }).click();
      const systemBlock = tree.getByRole('button', { name: '收起系统：环境采集协议' }).locator('../..');
      await systemBlock.getByRole('button', { name: /^参数配置/ }).click();
      assert.equal(await tree.getByText('错误编码', { exact: true }).count(), 0, 'dictionary names are not a third tree level');
      const groupId = groups.find(g => g.system_ref === systems.find(s => s.name === '环境采集协议').id && g.name === '参数配置').id;
      const dictionary = dictionaries.find(d => d.group_ref === groupId && d.key === 'error_code');
      const card = page.locator('#dict-panel-' + dictionary.id);
      while (await card.count() === 0) await page.getByRole('button', { name: '下一页字典', exact: true }).click();
      await card.getByRole('button', { name: '展开字典：错误编码', exact: true }).click();
      assert.equal(await card.locator('tbody tr').count(), 20);
      await card.getByRole('button', { name: '下一页', exact: true }).click();
      const itemPage = await card.locator('tbody tr').first().innerText();
      const scrollBefore = await list.evaluate(el => { el.scrollTop = 200; return el.scrollTop; });
      await sidebar(page, '实时监控').click();
      await tab(page, '字典管理').click();
      assert.equal(await card.locator('tbody tr').first().innerText(), itemPage, 'per-dictionary item page survives tab navigation');
      assert.equal(await list.evaluate(el => el.scrollTop), scrollBefore, 'each page retains its scroll position');
      for (let i = 0; i < 3; i++) await card.getByRole('button', { name: '下一页', exact: true }).click();
      assert.equal(await card.locator('tbody tr').count(), 16);
      assert.equal(await card.getByRole('button', { name: '下一页', exact: true }).isDisabled(), true);
      await page.getByRole('button', { name: '收起本页', exact: true }).click();
      await assertFits();
      await page.screenshot({ path: path.join(root, 'artifacts/workspace-dictionary-' + viewport.width + '.png') });
      await page.setViewportSize({ width: viewport.width, height: viewport.height === 768 ? 1075 : 768 });
      await page.waitForFunction(() => {
        const el = document.querySelector('[aria-label="字典列表"]');
        return el && el.scrollHeight <= el.clientHeight + 1;
      });
      await assertFits();
      console.log('Dictionary capacity at', viewport.width, 'x', viewport.height, ':', firstPageSize, 'cards');
    });
  }
  if (sizes.length === 2) assert.ok(sizes[0] > sizes[1], 'card pagination adapts to available height');

  await scenario('console-state', { width: 1366, height: 768 }, async page => {
    await page.goto('http://127.0.0.1:1422/console');
    await page.getByRole('tablist', { name: '工作区标签页' }).waitFor();
    await choose(page, 0, '环境采集协议');
    await choose(page, 1, '参数配置');
    await choose(page, 2, '批量配置请求');
    await page.locator('label[for="f-业务模式"]').locator('..').getByRole('combobox').click();
    await page.getByRole('option').filter({ hasText: /^01\s/ }).click();
    const input = page.getByLabel('测点列表 第1条 设备编号', { exact: true });
    await input.fill('010000A1');
    await page.getByRole('button', { name: '收起分支组：业务模式 = 01', exact: true }).click();
    await sidebar(page, '字典管理').click();
    await tab(page, '交互控制台').click();
    assert.equal(await page.getByRole('button', { name: '展开分支组：业务模式 = 01', exact: true }).count(), 1);
    await page.getByRole('button', { name: '展开分支组：业务模式 = 01', exact: true }).click();
    assert.equal(await input.inputValue(), '010000A1', 'list and branch input survives tab navigation');
  });

  await scenario('draft-dialog', { width: 1366, height: 768 }, async page => {
    await page.goto('http://127.0.0.1:1422/dict');
    await page.locator('[id^="dict-panel-"]').first().waitFor();
    await sidebar(page, '通道管理').click();
    await page.getByRole('button', { name: '新增通道', exact: true }).click();
    const draft = page.getByPlaceholder('如: 1号模拟终端');
    await draft.fill('尚未保存的通道');
    await tab(page, '字典管理').click();
    await selectedTab(page, '字典管理');
    assert.equal(await page.getByRole('dialog').count(), 0, 'inactive dialogs release their portals and modal locks');
    await page.getByPlaceholder('查询字典名 / Key / 条目').fill('设备');
    await tab(page, '通道管理').click();
    assert.equal(await draft.inputValue(), '尚未保存的通道', 'open modal draft resumes with its values');
    await tab(page, '字典管理').click();
    await page.getByRole('button', { name: '关闭标签页：通道管理', exact: true }).click();
    const closeGuard = page.getByRole('dialog', { name: '编辑尚未保存' });
    await closeGuard.waitFor();
    await closeGuard.getByRole('button', { name: '继续编辑', exact: true }).click();
    assert.equal(await tab(page, '通道管理').count(), 1);
    await page.getByRole('button', { name: '关闭标签页：通道管理', exact: true }).click();
    await closeGuard.getByRole('button', { name: '放弃并关闭', exact: true }).click();
    assert.equal(await tab(page, '通道管理').count(), 0);
  });
  await scenario('selector-portal', { width: 1366, height: 768 }, async page => {
    await page.goto('http://127.0.0.1:1422/monitor');
    await sidebar(page, '交互控制台').click();
    await activePanel(page).getByRole('combobox').first().click();
    await page.getByRole('listbox').waitFor();
    await page.goBack();
    await selectedTab(page, '实时监控');
    assert.equal(await page.getByRole('listbox').count(), 0, 'inactive selectors release their portals');
    await page.getByPlaceholder('关键字：报文 / 协议 / 地址').fill('下拉层已释放');
    await tab(page, '交互控制台').click();
    await page.getByRole('listbox').waitFor();
    await page.keyboard.press('Escape');
    await selectedTab(page, '交互控制台');
  });
  await browser.close();
  if (failures.length) throw new Error(failures.join('\n'));
})().catch(error => { console.error(error); process.exitCode = 1; });
