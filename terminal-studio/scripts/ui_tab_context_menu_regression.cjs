// Real WorkspaceTabs + close guards; isolated in-memory pages, no database or UDP.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

const labels = ['总览', '通道管理', '协议管理', '设备管理', '交互控制台'];
const actions = ['关闭当前', '关闭其他', '关闭左侧', '关闭右侧', '关闭全部'];
const fixture = `<!doctype html><html><head><meta charset="UTF-8"></head><body><div id="root"></div><script type="module">
  import RefreshRuntime from '/@react-refresh';
  RefreshRuntime.injectIntoGlobalHook(window); window.$RefreshReg$ = () => {}; window.$RefreshSig$ = () => type => type; window.__vite_plugin_react_preamble_installed__ = true;
  const {default: React} = await import('/node_modules/.vite/deps/react.js');
  const {default: ReactDOM} = await import('/node_modules/.vite/deps/react-dom_client.js');
  const workspaceSource = await (await fetch('/src/components/layout/WorkspaceTabs.tsx')).text();
  const routerModule = workspaceSource.match(/from "([^"]*react-router-dom.js[^\"]*)"/)[1];
  const {MemoryRouter, useNavigate} = await import(routerModule);
  await import('/src/styles/globals.css');
  const {WorkspaceTabs} = await import('/src/components/layout/WorkspaceTabs.tsx');
  const {useWorkspaceCloseGuard} = await import('/src/components/layout/WorkspaceActivity.tsx');
  const h = React.createElement;
  const definitions = [ ['/', '总览'], ['/nodes', '通道管理'], ['/protocols', '协议管理'], ['/devices', '设备管理'], ['/console', '交互控制台'] ];
  function DraftPage({label}) {
    const [value, setValue] = React.useState('');
    useWorkspaceCloseGuard(Boolean(value));
    return h('section', {className:'p-8'}, h('h1', {className:'text-xl font-semibold mb-6'}, label),
      h('label', {className:'grid gap-2 max-w-md'}, '编辑内容', h('input', { 'aria-label': label + '草稿', className:'rounded border bg-background px-3 py-2', value, onChange:e=>setValue(e.target.value) })));
  }
  const pages = definitions.map(([path, label])=>({path, label, element:h(DraftPage,{label})}));
  function Fixture() {
    const navigate = useNavigate();
    return h('div', {className:'flex h-screen overflow-hidden'},
      h('aside', {className:'w-48 shrink-0 border-r p-4 bg-card'}, h('strong', {className:'block mb-8'}, '多协议模拟系统'),
        ...definitions.map(([path,label])=>h('button',{key:path, className:'block w-full p-3 text-left hover:bg-accent rounded', 'aria-label':'打开：'+label, onClick:()=>navigate(path)},label))),
      h(WorkspaceTabs,{pages}));
  }
  ReactDOM.createRoot(document.getElementById('root')).render(h(MemoryRouter, null, h(Fixture)));
</script></body></html>`;

(async () => {
  const browser = await chromium.launch({ channel: 'chrome', headless: true });
  const page = await browser.newPage({ viewport: { width: 1580, height: 960 } });
  page.setDefaultTimeout(8000);
  const errors = [];
  page.on('pageerror', error => errors.push(String(error)));
  const out = process.env.UI_ARTIFACT_DIR || path.resolve('artifacts/tab-context-menu');
  fs.mkdirSync(out, { recursive: true });
  const results = [];
  const pass = name => { results.push(name); console.log('PASS: ' + name); };
  const url = (process.env.UI_BASE_URL || 'http://127.0.0.1:1422') + '/__tab_context_fixture';
  await page.route('**/__tab_context_fixture', route => route.fulfill({ contentType: 'text/html', body: fixture }));
  await page.addInitScript(() => { window.confirm = () => { throw new Error('Use the in-app unsaved editor confirmation'); }; });
  const tab = name => page.getByRole('tab', { name, exact: true });
  const menu = () => page.getByRole('menu', { name: '标签页操作', exact: true });
  const item = name => menu().getByRole('menuitem', { name, exact: true });
  const guard = () => page.getByRole('dialog', { name: '编辑尚未保存', exact: true });
  const open = name => page.getByRole('button', { name: '打开：' + name, exact: true }).click();
  const expectTabs = async expected => {
    await page.waitForFunction(names => JSON.stringify(Array.from(document.querySelectorAll('[role="tab"]'), el => el.textContent)) === JSON.stringify(names), expected);
    assert.deepEqual(await page.locator('[role="tab"]').allTextContents(), expected);
  };
  const active = async name => assert.equal(await tab(name).getAttribute('aria-selected'), 'true');
  const reset = async (all = true) => {
    await page.goto(url);
    await expectTabs(['总览']);
    if (all) for (const label of labels.slice(1)) await open(label);
    if (all) await expectTabs(labels);
  };
  const menuReady = async () => {
    await menu().waitFor();
    await page.waitForFunction(() => {
      const el = document.querySelector('[role="menu"][aria-label="标签页操作"]');
      return el && Number(getComputedStyle(el).opacity) === 1 && !el.getAnimations().some(animation => animation.playState === 'running');
    });
  };
  const rightClick = async name => { await tab(name).click({ button: 'right' }); await menuReady(); };
  try {
    const expected = {
      '关闭当前': ['总览', '通道管理', '设备管理', '交互控制台'],
      '关闭其他': ['协议管理'],
      '关闭左侧': ['协议管理', '设备管理', '交互控制台'],
      '关闭右侧': ['总览', '通道管理', '协议管理'],
      '关闭全部': ['总览'],
    };
    for (const action of actions) {
      await reset();
      await rightClick('协议管理');
      await active('交互控制台');
      assert.deepEqual(await menu().getByRole('menuitem').allTextContents(), actions);
      await item(action).click();
      await expectTabs(expected[action]);
      assert.equal(await menu().count(), 0);
      await active(action === '关闭全部' ? '总览' : ['关闭其他', '关闭右侧'].includes(action) ? '协议管理' : '交互控制台');
      pass(action + ' uses the clicked tab without activating it first');
    }

    await reset(false);
    await rightClick('总览');
    for (const action of actions) assert.equal(await item(action).isDisabled(), true, action + ' is disabled for the only home tab');
    await page.keyboard.press('Escape');
    await expectTabs(['总览']);
    await open('通道管理');
    await rightClick('总览');
    assert.equal(await item('关闭左侧').isDisabled(), true);
    assert.equal(await item('关闭右侧').isEnabled(), true);
    await page.keyboard.press('Escape');
    await rightClick('通道管理');
    assert.equal(await item('关闭右侧').isDisabled(), true);
    assert.equal(await item('关闭左侧').isEnabled(), true);
    await item('关闭左侧').click();
    await expectTabs(['通道管理']);
    await rightClick('通道管理');
    assert.equal(await item('关闭当前').isEnabled(), true);
    assert.equal(await item('关闭其他').isDisabled(), true);
    await item('关闭当前').click();
    await expectTabs(['总览']);
    pass('edge actions disable correctly; closing the last non-home tab restores home');

    // Two independent drafts must use one confirmation; cancel retains mounted editors.
    await reset();
    await open('协议管理');
    await page.getByRole('textbox', { name: '协议管理草稿', exact: true }).fill('协议字段编辑尚未保存');
    await open('设备管理');
    await page.getByRole('textbox', { name: '设备管理草稿', exact: true }).fill('设备参数编辑尚未保存');
    await open('交互控制台');
    await rightClick('协议管理');
    await item('关闭全部').click();
    await guard().waitFor();
    assert.equal(await guard().count(), 1);
    assert.match(await guard().innerText(), /协议管理/);
    assert.match(await guard().innerText(), /设备管理/);
    await expectTabs(labels);
    await guard().getByRole('button', { name: '继续编辑', exact: true }).click();
    await expectTabs(labels);
    await active('交互控制台');
    await page.waitForFunction(() => document.activeElement === document.querySelector('[role="tab"][aria-selected="true"]'));
    await open('协议管理');
    assert.equal(await page.getByRole('textbox', { name: '协议管理草稿', exact: true }).inputValue(), '协议字段编辑尚未保存');
    await open('设备管理');
    assert.equal(await page.getByRole('textbox', { name: '设备管理草稿', exact: true }).inputValue(), '设备参数编辑尚未保存');
    pass('one batch confirmation; cancel preserves all tabs and both drafts');

    // Keep a dirty tab and close only its right-hand neighbours, including another dirty tab.
    await rightClick('协议管理');
    await item('关闭右侧').click();
    await guard().waitFor();
    assert.match(await guard().innerText(), /设备管理/);
    assert.doesNotMatch(await guard().innerText(), /协议管理/);
    await guard().getByRole('button', { name: '放弃并关闭', exact: true }).click();
    await expectTabs(['总览', '通道管理', '协议管理']);
    await active('协议管理');
    await page.waitForFunction(() => document.activeElement === document.querySelector('[role="tab"][aria-selected="true"]'));
    assert.equal(await page.getByRole('textbox', { name: '协议管理草稿', exact: true }).inputValue(), '协议字段编辑尚未保存');
    assert.equal(await guard().count(), 0);
    await rightClick('协议管理');
    await item('关闭全部').click();
    await guard().getByRole('button', { name: '放弃并关闭', exact: true }).click();
    await expectTabs(['总览']);
    assert.equal(await guard().count(), 0);
    pass('confirm closes only selected targets; retained dirty tab stays intact; all returns home');

    await reset();
    await tab('交互控制台').focus();
    await page.keyboard.press('Shift+F10');
    await menu().waitFor();
    assert.equal(await item('关闭当前').evaluate(el => el === document.activeElement), true);
    await page.keyboard.press('ArrowDown');
    assert.equal(await item('关闭其他').evaluate(el => el === document.activeElement), true);
    await page.keyboard.press('ArrowUp');
    assert.equal(await item('关闭当前').evaluate(el => el === document.activeElement), true);
    await page.keyboard.press('End');
    assert.equal(await item('关闭全部').evaluate(el => el === document.activeElement), true);
    await page.keyboard.press('Home');
    assert.equal(await item('关闭当前').evaluate(el => el === document.activeElement), true);
    await page.keyboard.press('Escape');
    await menu().waitFor({ state: 'hidden' });
    assert.equal(await tab('交互控制台').evaluate(el => el === document.activeElement), true);
    await page.keyboard.press('ContextMenu');
    await menu().waitFor();
    await page.keyboard.press('ArrowDown');
    await page.keyboard.press('ArrowDown');
    assert.equal(await item('关闭左侧').evaluate(el => el === document.activeElement), true);
    await page.keyboard.press('ArrowDown');
    assert.equal(await item('关闭全部').evaluate(el => el === document.activeElement), true, 'navigation skips disabled close-right');
    await page.keyboard.press('Home');
    await page.keyboard.press('Enter');
    await expectTabs(['总览', '通道管理', '协议管理', '设备管理']);
    pass('Shift+F10/ContextMenu, arrows, Home/End, Escape focus restore and Enter selection');

    // Presentation checks use the real theme and tab chrome styles.
    await reset();
    for (const theme of ['light', 'dark']) {
      await page.evaluate(dark => document.documentElement.classList.toggle('dark', dark), theme === 'dark');
      await open('协议管理');
      await rightClick('协议管理');
      const style = await page.locator('.workspace-tab-active').evaluate(el => ({ shadow: getComputedStyle(el).boxShadow, color: getComputedStyle(el).color }));
      assert.match(style.shadow, /0px -2px 0px 0px inset/, 'selected accent is at the bottom');
      const box = await menu().boundingBox();
      assert.ok(box.x >= 0 && box.y >= 0 && box.x + box.width <= 1580 && box.y + box.height <= 960);
      await page.screenshot({ path: path.join(out, theme + '-tab-menu.png'), fullPage: true });
      await page.keyboard.press('Escape');
    }
    await page.setViewportSize({ width: 720, height: 480 });
    // Synthetic corner position isolates collision behaviour independent of tab strip scrolling.
    await tab('协议管理').dispatchEvent('contextmenu', { clientX: 718, clientY: 478, bubbles: true, cancelable: true });
    await menuReady();
    const corner = await menu().boundingBox();
    assert.ok(corner.x >= 0 && corner.y >= 0 && corner.x + corner.width <= 720 && corner.y + corner.height <= 480, JSON.stringify(corner));
    await page.screenshot({ path: path.join(out, 'narrow-corner-menu.png'), fullPage: true });
    await page.keyboard.press('Escape');
    pass('light/dark bottom accent and menu placement remain inside narrow viewport');
    assert.deepEqual(errors, []);
    fs.writeFileSync(path.join(out, 'results.json'), JSON.stringify({ passed: results, pageErrors: errors }, null, 2));
    console.log('PASS: ' + results.length + ' workspace tab menu scenarios; no browser errors');
  } catch (error) {
    console.error('Browser errors:', errors);
    console.error('Page body:', (await page.locator('body').innerText()).slice(0, 1600));
    await page.screenshot({ path: path.join(out, 'failure.png'), fullPage: true });
    throw error;
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
