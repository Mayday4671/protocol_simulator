const { chromium } = require('playwright');
const assert = require('node:assert/strict');
(async () => {
  const browser = await chromium.launch({ channel: 'chrome', headless: true });
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  const errors = [];
  page.on('pageerror', e => errors.push(String(e)));
  await page.addInitScript(() => {
    window.__TAURI_INTERNALS__ = { transformCallback: () => 1, unregisterCallback: () => {}, invoke: async cmd => {
      if (cmd === 'engine_status') return { listening_nodes: [], running_heartbeats: [], periodic_sends: [] };
      if (cmd === 'log_stats') return { total: 0, today_in: 0, today_out: 0, today_errors: 0 };
      return [];
    } };
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
  });
  try {
    const pages = [
      ['/', '总览', ['多协议模拟系统运行状态', 'Markdown 可批量导入', '监听中的通道将以绿色标识', '实时事件的最新 12 条']],
      ['/nodes', '通道管理', ['本地 IP 填 0.0.0.0', '配置本地模拟终端与远端目标系统']],
      ['/protocols', '协议管理', ['三级分层', '主动发起, 可配自动应答链']],
      ['/dict', '字典', ['左侧选择字典定位', '一张字典是一组同类取值的集合']],
      ['/devices', '终端资料与独立会话', ['按配置协议生成资料字段', '在「交互控制台」发送设备信息上报']],
      ['/monitor', '实时监控', ['收发报文实时流', '双击行查看解析详情']],
    ];
    for (const [route, heading, removed] of pages) {
      await page.goto('http://127.0.0.1:1422' + route);
      await page.getByRole('heading', { name: heading, exact: true }).waitFor();
      const text = await page.locator('main').innerText();
      for (const hint of removed) assert.ok(!text.includes(hint), `${route}: remove ${hint}`);
    }
    await page.goto('http://127.0.0.1:1422/dict');
    await page.getByTitle('新增字典', { exact: true }).click();
    assert.ok(!(await page.getByRole('dialog').innerText()).includes('中文名, 界面上显示'));
    await page.getByRole('button', { name: '保存', exact: true }).click();
    await page.getByText('请填写字典名称', { exact: true }).waitFor();
    assert.deepEqual(errors, []);
    console.log('PASS compact pages retain labels, empty states and validation');
  } finally { await browser.close(); }
})().catch(e => { console.error(e); process.exitCode = 1; });
