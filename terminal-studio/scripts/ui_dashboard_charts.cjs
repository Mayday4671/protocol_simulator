// Isolated UI fixtures only; production statistics always come from Rust/SQLite.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
(async () => {
  const browser = await chromium.launch({ executablePath: 'C:/Program Files/Google/Chrome/Application/chrome.exe', headless: true });
  const page = await browser.newPage({ viewport: { width: 1360, height: 860 } });
  page.setDefaultTimeout(8000);
  const out = path.resolve('artifacts/dashboard-charts');
  fs.mkdirSync(out, { recursive: true });
  const errors = [], checks = [];
  page.on('pageerror', e => errors.push(e.message));
  await page.addInitScript(() => {
    window.calls = [];
    window.fail = false;
    window.hold = false;
    const stats = range => {
      const n = range === '1h' ? 12 : range === '24h' ? 24 : 7;
      const buckets = Array.from({ length: n }, (_, i) => ({
        start: `2026-10-20 ${String(i).padStart(2, '0')}:00:00`,
        end: `2026-10-20 ${String(i + 1).padStart(2, '0')}:00:00`,
        received: range === '1h' ? 0 : (i % 5) * 2,
        sent: range === '1h' ? 0 : (i % 3) * 3,
      }));
      const received = buckets.reduce((a, b) => a + b.received, 0), sent = buckets.reduce((a, b) => a + b.sent, 0);
      return { start: buckets[0].start, end: buckets.at(-1).end, buckets, received, sent,
        channels: received + sent ? [{ node_id: 1, name: '测试通道', received, sent }] : [] };
    };
    window.__TAURI_INTERNALS__ = { invoke: async (cmd, args) => {
      if (cmd !== 'traffic_stats') throw Error(`unexpected ${cmd}`);
      window.calls.push(args.range);
      if (window.fail) throw Error('isolated query failure');
      if (window.hold) { window.hold = false; await new Promise(resolve => window.release = resolve); }
      return stats(args.range);
    }};
  });
  await page.route('**/__charts_fixture', route => route.fulfill({ contentType: 'text/html', body: `<!doctype html><html><body><div id="root"></div><script type="module">
    import RefreshRuntime from '/@react-refresh';RefreshRuntime.injectIntoGlobalHook(window);window.$RefreshReg$=()=>{};window.$RefreshSig$=()=>type=>type;window.__vite_plugin_react_preamble_installed__=true;
    const {default:React}=await import('/node_modules/.vite/deps/react.js');const {default:ReactDOM}=await import('/node_modules/.vite/deps/react-dom_client.js');
    await import('/src/styles/globals.css');const {TrafficCharts}=await import('/src/components/dashboard/TrafficCharts.tsx');
    ReactDOM.createRoot(document.getElementById('root')).render(React.createElement('main',{className:'p-6'},React.createElement(TrafficCharts)));
  </script></body></html>` }));
  try {
    await page.goto('http://127.0.0.1:1424/__charts_fixture');
    await page.getByRole('img', { name: /^测试通道/ }).waitFor();
    assert.equal(await page.locator('svg rect[tabindex]').count(), 24);
    await page.locator('svg rect[tabindex]').nth(4).focus();
    assert.match(await page.locator('[aria-live]').innerText(), /接收 8 条/);
    checks.push('24-hour data, channel counts and keyboard point details');
    await page.getByRole('button', { name: '最近 1 小时', exact: true }).click();
    await page.waitForFunction(() => document.querySelectorAll('svg rect[tabindex]').length === 12);
    assert.equal(await page.getByText('所选时段暂无报文', { exact: true }).count(), 2);
    checks.push('empty range shows zero counts and no invented channel bars');
    await page.getByRole('button', { name: '最近 7 天', exact: true }).click();
    await page.waitForFunction(() => document.querySelectorAll('svg rect[tabindex]').length === 7);
    await page.screenshot({ path: path.join(out, 'fixture-light.png'), animations: 'disabled' });
    await page.evaluate(() => document.documentElement.classList.add('dark'));
    await page.screenshot({ path: path.join(out, 'fixture-dark.png'), animations: 'disabled' });
    await page.setViewportSize({ width: 680, height: 850 });
    assert(await page.evaluate(() => document.body.scrollWidth <= innerWidth));
    await page.screenshot({ path: path.join(out, 'fixture-narrow.png'), animations: 'disabled' });
    checks.push('7-day range, light/dark themes and narrow layout');
    await page.evaluate(() => window.fail = true);
    await page.getByRole('button', { name: '刷新通信统计' }).click();
    await page.getByRole('alert').waitFor();
    assert.equal(await page.locator('svg rect[tabindex]').count(), 7);
    await page.evaluate(() => window.fail = false);
    await page.getByRole('button', { name: '重试', exact: true }).click();
    await page.getByRole('alert').waitFor({ state: 'hidden' });
    checks.push('query failure keeps prior data with warning; retry recovers');
    await page.evaluate(() => window.hold = true);
    await page.getByRole('button', { name: '最近 24 小时', exact: true }).click();
    await page.waitForFunction(() => typeof window.release === 'function');
    await page.getByRole('button', { name: '最近 1 小时', exact: true }).click();
    await page.waitForFunction(() => document.querySelectorAll('svg rect[tabindex]').length === 12);
    await page.evaluate(() => window.release());
    await page.waitForTimeout(150);
    assert.equal(await page.locator('svg rect[tabindex]').count(), 12);
    checks.push('late response cannot overwrite a newer range');
    assert.deepEqual(errors, []);
    fs.writeFileSync(path.join(out, 'ui-results.json'), JSON.stringify({ checks, pageErrors: errors }, null, 2));
    console.log(JSON.stringify({ checks, pageErrors: errors }));
  } finally { await browser.close(); }
})().catch(e => { console.error(e); process.exitCode = 1; });
