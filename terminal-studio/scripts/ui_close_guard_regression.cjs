// Isolated React fixture; native window close is separately checked on the built EXE.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
(async () => {
  const browser = await chromium.launch({ channel: 'chrome', headless: true });
  try {
    const page = await browser.newPage();
    page.setDefaultTimeout(8000);
    const errors = [];
    page.on('pageerror', e => errors.push(String(e)));
    await page.addInitScript(() => {
      window.confirm = async () => { throw new Error('Do not coerce asynchronous native confirm to boolean'); };
      window.__TAURI_INTERNALS__ = { transformCallback: () => 1, unregisterCallback: () => {}, invoke: async cmd => {
        if (cmd === 'engine_status') return { listening_nodes: [], running_heartbeats: [], heartbeats: [], periodic_sends: [] };
        if (cmd === 'log_stats') return { total: 0, in_count: 0, out_count: 0, error_count: 0 };
        return [];
      } };
      window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
    });
    await page.goto('http://127.0.0.1:1422/nodes');
    await page.getByRole('button', { name: '新增通道', exact: true }).click();
    await page.getByLabel('通道名称 *', { exact: true }).fill('未保存的通道');
    await page.getByRole('button', { name: '关闭标签页：通道管理', exact: true }).click();
    const guard = page.getByRole('dialog', { name: '编辑尚未保存' });
    await guard.getByRole('button', { name: '继续编辑', exact: true }).click();
    assert.equal(await page.getByLabel('通道名称 *', { exact: true }).inputValue(), '未保存的通道');
    await page.getByRole('button', { name: '关闭标签页：通道管理', exact: true }).click();
    await guard.getByRole('button', { name: '放弃并关闭', exact: true }).click();
    assert.equal(await page.getByRole('tab', { name: '通道管理', exact: true }).count(), 0);
    assert.deepEqual(errors, []);
    console.log('PASS: in-app close confirmation; cancel preserves draft; confirm closes; no async window.confirm coercion');
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
