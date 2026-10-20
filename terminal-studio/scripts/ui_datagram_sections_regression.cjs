// Isolated component fixtures only; the real Tauri client is verified separately.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');

(async () => {
  const browser = await chromium.launch({ headless: true, channel: 'chrome' });
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  const errors = [];
  page.on('pageerror', error => { errors.push(String(error)); console.error('Fixture page error:', error); });
  const output = process.env.UI_ARTIFACT_DIR || path.resolve('artifacts/datagram-ui');
  fs.mkdirSync(output, { recursive: true });
  await page.addInitScript(() => {
    let offset = 0;
    const field = (name, len, extra = {}) => {
      const value = { name, field_name: name, start: offset, len, hex: `0x${'FF'.repeat(len)}`, dec: len <= 8 ? '255' : null, meaning: '无符号整数 · 大端', dict_label: null, is_header: false, section: 'payload', ...extra };
      offset += len;
      return value;
    };
    const fields = [
      field('报文标志', 2, { section: 'header', is_header: true }),
      field('协议版本', 1, { section: 'header', is_header: true, dict_label: '版本 1' }),
      field('报文总长', 2, { section: 'header', is_header: true }),
      field('消息类型', 1, { section: 'header', is_header: true, dict_label: '设备配置应答' }),
      field('源地址', 4, { section: 'header', is_header: true }),
      field('目标地址', 4, { section: 'header', is_header: true }),
      field('发送序号', 8, { section: 'header', is_header: true, dec: '18446744073709551615' }),
      field('会话标识', 4, { section: 'header_extension', section_name: '认证扩展' }),
      field('配置模式', 1, { dict_label: '增加设备' }),
      field('条目数', 1, { dec: '2' }),
      field('设备条目[1].设备编号', 4, { field_name: '设备编号', record_name: '设备条目', record_index: 1 }),
      field('设备条目[1].类型', 1, { field_name: '设备类型', record_name: '设备条目', record_index: 1, dict_label: '温度传感器' }),
      field('设备条目[1].状态', 1, { field_name: '处理结果', record_name: '设备条目', record_index: 1, dict_label: '成功' }),
      field('设备条目[2].设备编号', 4, { field_name: '设备编号', record_name: '设备条目', record_index: 2 }),
      field('设备条目[2].类型', 1, { field_name: '设备类型', record_name: '设备条目', record_index: 2, dict_label: '湿度传感器' }),
      field('设备条目[2].状态', 1, { field_name: '处理结果', record_name: '设备条目', record_index: 2, dict_label: '成功' }),
      field('校验值', 2, { section: 'trailer' }),
      field('结束标志', 1, { section: 'trailer' }),
    ];
    const base = { log_id: 1, ts: '2026-10-19 14:32:08.426', direction: 'in', node_name: '测试终端', local_addr: '127.0.0.1:39021', remote_addr: '127.0.0.1:39022', protocol_id: 'fixture-only', protocol_name: '设备配置应答', msg_type_label: '应答', total_len: offset, header_len: 22, defined_len: offset, raw_hex: 'FF'.repeat(offset), fields };
    offset = 0;
    const longFields = [field('零条目数组', 0, { dec: null, meaning: '0 条记录' }), field('这是没有公共头的另一种协议而且字段名非常长需要自动换行不能挤出窗口', 160, { dec: null }), field('末尾', 2, { section: 'trailer', section_name: '校验段', expected_len: 4 })];
    const other = { ...base, log_id: 2, protocol_name: '无公共头 · 长字段', direction: 'out', total_len: offset + 32, header_len: 0, defined_len: offset, raw_hex: 'AB'.repeat(offset + 32), fields: longFields };
    offset = 0;
    const repeatedSections = [
      field('头 A 首段字段', 2, { section: 'header', section_name: '公共头 A', is_header: true }),
      field('头 B 字段', 2, { section: 'header', section_name: '公共头 B', is_header: true }),
      field('头 A 后段字段', 2, { section: 'header', section_name: '公共头 A', is_header: true }),
      field('数据', 1),
    ];
    const repeated = { ...base, log_id: 5, protocol_name: '分段公共头 A/B/A', total_len: offset, header_len: 6, defined_len: offset, raw_hex: 'FF'.repeat(offset), fields: repeatedSections };
    window.__fixtureData = { 1: base, 2: other, 3: { ...base, log_id: 3, protocol_name: '过期结果' }, 5: repeated };
    window.__copied = [];
    Object.defineProperty(navigator, 'clipboard', { value: { writeText: async text => window.__copied.push(text) } });
    window.__TAURI_INTERNALS__ = { invoke: async (command, { logId }) => {
      if (command !== 'parse_datagram') return null;
      if (logId === 3) await new Promise(resolve => setTimeout(resolve, 250));
      if (logId === 4) throw new Error('解析失败 fixture');
      return window.__fixtureData[logId];
    } };
  });
  await page.route('**/__datagram_fixture', route => route.fulfill({ contentType: 'text/html', body: `<!doctype html><html><head><meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1"></head><body><div id="root"></div><script type="module">
    import RefreshRuntime from '/@react-refresh';
    RefreshRuntime.injectIntoGlobalHook(window); window.$RefreshReg$ = () => {}; window.$RefreshSig$ = () => type => type; window.__vite_plugin_react_preamble_installed__ = true;
    const {default: React} = await import('/node_modules/.vite/deps/react.js');
    const {default: ReactDOM} = await import('/node_modules/.vite/deps/react-dom_client.js');
    await import('/src/styles/globals.css');
    const {DatagramDetail} = await import('/src/components/monitor/DatagramDetail.tsx');
    function Fixture() { const [id, setId] = React.useState(1); const [open, setOpen] = React.useState(true); window.__setLogId = value => { setOpen(true); setId(value); }; return open ? React.createElement(DatagramDetail, {logId: id, onClose: () => setOpen(false)}) : React.createElement('div', null, 'fixture closed'); }
    ReactDOM.createRoot(document.getElementById('root')).render(React.createElement(Fixture));
  </script></body></html>` }));
  const assertExpanded = async (expected, message) => {
    await page.waitForFunction(expected => {
      const sections = [...document.querySelectorAll('.datagram-section')];
      return sections.length === expected.length && sections.every((section, index) => {
        const heading = section.querySelector('.datagram-section-heading');
        const content = section.querySelector('.datagram-section-content');
        return heading?.tagName === 'BUTTON' && heading.getAttribute('aria-expanded') === String(expected[index]) && content?.hidden === !expected[index];
      });
    }, expected);
    for (const [index, expanded] of expected.entries()) {
      assert.equal(await page.locator('.datagram-section-content').nth(index).isVisible(), expanded, `${message}: section ${index + 1} visibility`);
    }
  };
  const assertThemeContrast = async theme => {
    await page.evaluate(theme => document.documentElement.classList.toggle('dark', theme === 'dark'), theme);
    // Wait for actual computed colors to settle, including theme transitions.
    await page.waitForFunction(() => {
      const surface = document.querySelector('.datagram-detail');
      if (!surface || surface.getAnimations({ subtree: true }).some(animation => animation.playState === 'running' || animation.pending)) return false;
      const sample = [...surface.querySelectorAll('.datagram-field, .datagram-section-heading, .datagram-actions button')].map(element => {
        const style = getComputedStyle(element);
        return [style.color, style.backgroundColor, style.borderColor].join('|');
      }).join(';');
      const previous = window.__datagramStyleSample;
      window.__datagramStyleSample = { sample, count: previous?.sample === sample ? previous.count + 1 : 0 };
      return window.__datagramStyleSample.count >= 3;
    });
    const failures = await page.evaluate(() => {
      const canvas = document.createElement('canvas');
      canvas.width = canvas.height = 1;
      const context = canvas.getContext('2d', { willReadFrequently: true });
      const rgba = value => {
        context.clearRect(0, 0, 1, 1);
        context.fillStyle = value;
        context.fillRect(0, 0, 1, 1);
        const color = [...context.getImageData(0, 0, 1, 1).data];
        return [...color.slice(0, 3), color[3] / 255];
      };
      const composite = (foreground, background) => foreground.slice(0, 3).map((channel, index) => channel * foreground[3] + background[index] * (1 - foreground[3]));
      const luminance = color => color.map(channel => {
        const srgb = channel / 255;
        return srgb <= 0.04045 ? srgb / 12.92 : ((srgb + 0.055) / 1.055) ** 2.4;
      }).reduce((sum, channel, index) => sum + channel * [0.2126, 0.7152, 0.0722][index], 0);
      return [...document.querySelectorAll('.datagram-field *, .datagram-section-heading *, .datagram-section-links button, .datagram-section-links button > span, .datagram-actions button')]
        .filter(element => element.checkVisibility() && (element.matches('.datagram-field-pin') || [...element.childNodes].some(node => node.nodeType === Node.TEXT_NODE && node.textContent.trim())))
        .flatMap(element => {
          const chain = [];
          for (let current = element; current; current = current.parentElement) chain.unshift(current);
          const background = chain.reduce((color, ancestor) => {
            const style = getComputedStyle(ancestor);
            let next = composite(rgba(style.backgroundColor), color);
            // Cards use a constant-color gradient as a translucent tint over their base.
            if (style.backgroundImage !== 'none') {
              const stops = style.backgroundImage.match(/rgba?\([^)]*\)/g) || [];
              if (!stops.length || !stops.every(stop => stop === stops[0])) throw new Error(`Contrast fixture needs sampling for nonuniform background: ${style.backgroundImage}`);
              next = composite(rgba(stops[0]), next);
            }
            return next;
          }, [255, 255, 255]);
          const foreground = rgba(getComputedStyle(element).color);
          foreground[3] *= chain.reduce((opacity, ancestor) => opacity * Number(getComputedStyle(ancestor).opacity), 1);
          const lightness = [luminance(composite(foreground, background)), luminance(background)].sort((a, b) => b - a);
          const ratio = (lightness[0] + 0.05) / (lightness[1] + 0.05);
          const minimum = element.matches('.datagram-field-pin') ? 3 : 4.5;
          return ratio >= minimum ? [] : [{ text: element.textContent.trim() || '锁定图钉', className: element.getAttribute('class') || element.parentElement.getAttribute('class'), contrast: Number(ratio.toFixed(2)), minimum }];
        });
    });
    assert.deepEqual(failures, [], `${theme} theme contrast must reach 4.5:1 for text and 3:1 for pin markers`);
  };
  try {
    await page.goto((process.env.UI_BASE_URL || 'http://127.0.0.1:1427') + '/__datagram_fixture');
    await page.locator('.datagram-section').first().waitFor();
    assert.deepEqual(await page.locator('.datagram-section-title').allTextContents(), ['公共头', '认证扩展', '净荷', '尾部']);
    await assertExpanded([true, true, true, true], 'sections default to expanded');
    assert.equal(await page.locator('.datagram-record-title').count(), 2);
    const headerTones = await page.locator('.datagram-section').first().locator('.datagram-field').evaluateAll(cards => cards.map(card => card.getAttribute('data-field-tone')));
    assert.ok(headerTones.every(tone => /^[0-5]$/.test(tone)), 'fields use one of the six supported tones');
    assert.ok(new Set(headerTones).size >= 3, 'a multi-field group must contain at least three tones');
    assert.ok(headerTones.every((tone, index) => index === 0 || tone !== headerTones[index - 1]), 'adjacent new fields retain distinct tones across the sixth/seventh field boundary');
    const recordTones = await page.locator('.datagram-record').evaluateAll(records => records.map(record =>
      [...record.querySelectorAll('.datagram-field')].map(card => ({ name: card.querySelector('.datagram-field-name').textContent, tone: card.getAttribute('data-field-tone') })),
    ));
    assert.equal(recordTones[0].length, 3);
    assert.ok(recordTones.flat().every(field => /^[0-5]$/.test(field.tone)), 'array fields also have explicit tones');
    assert.deepEqual(recordTones[1], recordTones[0], 'same named fields retain their tone across array records');
    assert.equal(await page.locator('.datagram-field-pin:visible').count(), 0, 'unlocked cards show no pin marker');
    assert.match(await page.locator('.datagram-route').innerText(), /127\.0\.0\.1:39022.*→.*127\.0\.0\.1:39021/s);
    const sequence = page.getByRole('button', { name: /发送序号.*字节/ });
    assert.match(await sequence.innerText(), /18446744073709551615/);
    const type = page.getByRole('button', { name: /设备类型.*字节/ }).first();
    assert.match(await type.innerText(), /设备类型/);
    assert.match(await type.innerText(), /温度传感器/);
    await sequence.focus();
    await page.keyboard.press('Enter');
    await page.locator('.datagram-inspector').waitFor();
    assert.equal(await sequence.getAttribute('aria-pressed'), 'true');
    await sequence.locator('.datagram-field-pin').waitFor({ state: 'visible' });
    assert.equal(await sequence.locator('.datagram-field-pin').getAttribute('aria-hidden'), 'true', 'pin is decorative; the card exposes its pressed state');
    assert.equal(await page.locator('.datagram-field-pin:visible').count(), 1, 'only the locked card has a pin marker');
    const mainBox = await page.locator('.datagram-scroll').boundingBox();
    const inspectorBox = await page.locator('.datagram-inspector').boundingBox();
    assert.ok(mainBox.y + mainBox.height <= inspectorBox.y + 1, 'inspector cannot cover the card viewport');
    await page.keyboard.press('Escape');
    assert.equal(await page.locator('.datagram-inspector').count(), 0);
    assert.equal(await sequence.getAttribute('aria-pressed'), 'false');
    assert.equal(await page.locator('.datagram-field-pin:visible').count(), 0, 'unlocking removes the pin marker');
    await sequence.click();
    await page.locator('.datagram-inspector').waitFor();
    await page.locator('.datagram-section-heading').first().click();
    await assertExpanded([false, true, true, true], 'a heading collapses only its section');
    assert.equal(await page.locator('.datagram-inspector').count(), 0, 'collapsing the pinned field section closes its inspector');
    assert.equal(await page.locator('.datagram-field[aria-pressed="true"]').count(), 0, 'collapsed fields cannot remain pinned');
    assert.equal(await page.locator('.datagram-field-pin:visible').count(), 0, 'collapsing the locked section clears its pin marker');
    await page.locator('.datagram-section-links button').first().click();
    await assertExpanded([true, true, true, true], 'navigation reopens its collapsed target');
    await page.waitForFunction(() => document.activeElement === document.querySelectorAll('.datagram-section-heading')[0]);
    await page.locator('.datagram-section-heading').nth(2).focus();
    await page.keyboard.press('Space');
    await assertExpanded([true, true, false, true], 'section headings support keyboard activation');
    await page.getByRole('button', { name: '复制报文', exact: true }).click();
    await page.getByRole('button', { name: '复制解析', exact: true }).click();
    const copied = await page.evaluate(() => window.__copied);
    assert.equal(copied.length, 2);
    assert.match(copied[1], /127\.0\.0\.1:39022 → 127\.0\.0\.1:39021/);
    assert.match(copied[1], /设备条目\[1\]\.类型/, 'copy includes fields in collapsed sections');
    await page.getByRole('button', { name: '全部折叠', exact: true }).click();
    await assertExpanded([false, false, false, false], 'collapse all');
    assert.equal(await page.locator('.datagram-field:visible').count(), 0);
    await page.screenshot({ path: path.join(output, 'sections-collapsed.png') });
    await page.locator('.datagram-section-links button').nth(2).click();
    await assertExpanded([false, false, true, false], 'navigation opens only the requested section');
    await page.waitForFunction(() => document.activeElement === document.querySelectorAll('.datagram-section-heading')[2]);
    await page.getByRole('button', { name: '全部展开', exact: true }).click();
    await assertExpanded([true, true, true, true], 'expand all');
    for (const theme of ['light', 'dark']) {
      await assertThemeContrast(theme);
      await page.screenshot({ path: path.join(output, `sections-${theme}.png`) });
    }
    await page.getByRole('button', { name: '全部折叠', exact: true }).click();
    await assertExpanded([false, false, false, false], 'collapse before switching packets');
    await page.evaluate(() => window.__setLogId(5));
    await page.getByText('分段公共头 A/B/A', { exact: true }).waitFor();
    assert.deepEqual(await page.locator('.datagram-section-title').allTextContents(), ['公共头 A', '公共头 B', '公共头 A', '净荷']);
    await assertExpanded([true, true, true, true], 'switching packets resets expansion');
    await page.locator('.datagram-section-heading').first().click();
    await assertExpanded([false, true, true, true], 'first A segment does not collapse later A segment');
    await page.locator('.datagram-section-heading').nth(2).click();
    await assertExpanded([false, true, false, true], 'repeated section names have independent state');
    await page.locator('.datagram-section-links button').nth(2).click();
    await assertExpanded([false, true, true, true], 'navigation targets the correct repeated segment');
    await page.waitForFunction(() => document.activeElement === document.querySelectorAll('.datagram-section-heading')[2]);
    await page.screenshot({ path: path.join(output, 'sections-repeated.png') });
    await page.locator('.datagram-section-heading').first().focus();
    await page.keyboard.press('Enter');
    await assertExpanded([true, true, true, true], 'Enter reopens the focused segment');
    await page.getByRole('button', { name: '全部折叠', exact: true }).click();
    await assertExpanded([false, false, false, false], 'collapse repeated segments together');
    await page.evaluate(() => window.__setLogId(2));
    await page.getByText('无公共头 · 长字段', { exact: true }).waitFor();
    assert.deepEqual(await page.locator('.datagram-section-title').allTextContents(), ['净荷', '校验段', '未解析']);
    await assertExpanded([true, true, true], 'different packet layout starts fully expanded');
    assert.match(await page.getByRole('button', { name: /零条目数组.*字节/ }).innerText(), /0 B/);
    assert.match(await page.getByRole('button', { name: /末尾.*字节/ }).innerText(), /缺 2 B/);
    await page.getByRole('button', { name: /这是没有公共头.*字节/ }).click();
    assert.match(await page.locator('.datagram-inspector code').first().innerText(), new RegExp('AB(?: AB){159}'));
    await page.keyboard.press('Escape');
    const unparsed = page.getByRole('button', { name: /未解析字节.*字节/ });
    assert.equal(await unparsed.getAttribute('data-field-tone'), null, 'unparsed bytes use the neutral color fallback');
    assert.equal(await unparsed.locator('.datagram-expand').innerText(), '查看全部 32 B');
    for (const theme of ['light', 'dark']) {
      await unparsed.click();
      await unparsed.locator('.datagram-field-pin').waitFor({ state: 'visible' });
      assert.equal(await unparsed.getAttribute('aria-pressed'), 'true');
      assert.equal(await unparsed.locator('.datagram-field-pin').getAttribute('aria-hidden'), 'true');
      await assertThemeContrast(theme);
      assert.equal(await page.locator('.datagram-inspector code').first().innerText(), Array(32).fill('AB').join(' '), 'long unparsed bytes remain complete in the inspector');
      await page.locator('.datagram-section[data-kind="unparsed"]').screenshot({ path: path.join(output, `unparsed-${theme}.png`) });
      await page.keyboard.press('Escape');
      assert.equal(await unparsed.getAttribute('aria-pressed'), 'false');
    }
    await page.locator('.datagram-scroll').evaluate(element => { element.scrollTop = 0; });
    for (const width of [900, 390]) {
      await page.setViewportSize({ width, height: 844 });
      assert.ok(await page.evaluate(() => document.querySelector('.datagram-detail').scrollWidth <= innerWidth), `no overlay overflow at ${width}`);
      assert.ok(await page.evaluate(() => [...document.querySelectorAll('.datagram-field')].every(card => card.scrollWidth <= card.clientWidth)), `cards wrap at ${width}`);
      await page.screenshot({ path: path.join(output, `long-field-${width}.png`) });
    }
    await page.evaluate(() => window.__setLogId(3));
    await page.getByText('解析中…', { exact: true }).waitFor();
    assert.equal(await page.locator('.datagram-section').count(), 0, 'previous packet disappears while loading');
    await page.evaluate(() => window.__setLogId(1));
    await page.locator('.datagram-section').first().waitFor();
    await page.waitForTimeout(350);
    assert.equal(await page.getByText('过期结果', { exact: true }).count(), 0);
    await page.evaluate(() => window.__setLogId(4));
    await page.getByRole('alert').waitFor();
    assert.equal(await page.locator('.datagram-section').count(), 0);
    await page.evaluate(() => window.__setLogId(1));
    await page.locator('.datagram-section').first().waitFor();
    assert.equal(await page.getByRole('alert').count(), 0);
    await page.keyboard.press('Escape');
    await page.getByText('fixture closed', { exact: true }).waitFor();
    assert.deepEqual(errors, []);
    console.log('PASS: component fixtures, varied adjacent field tones, consistent array tones, visible pin state, independent section collapse, collapse/expand all, navigation reopening, repeated A/B/A segments, pinned-field cleanup, keyboard activation, full clipboard export, long unparsed text/pin contrast, theme contrast, narrow wrapping, packet reset and request race');
    console.log(`Screenshots: ${output}`);
  } catch (error) {
    await page.screenshot({ path: path.join(output, 'sections-failure.png') });
    console.error('Fixture DOM:', (await page.locator('body').innerText()).slice(0, 1500));
    throw error;
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
