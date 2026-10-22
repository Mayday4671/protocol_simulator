// Run against Vite with isolated IPC fixtures: no database changes or UDP sends.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');

(async () => {
  const browser = await chromium.launch({ headless: true, channel: 'chrome' });
  const page = await browser.newPage({ viewport: { width: 1440, height: 1000 } });
  const errors = [];
  page.on('pageerror', error => errors.push(String(error)));
  await page.addInitScript(() => {
    const field = (name, len, value, role = 'none', extra = {}) => ({ name, len, enc: 'hex', value, role, ...extra });
    const units = [field('设备编号', 1, '09'), field('参数', 1, '99')];
    const fields = [
      field('总长', 2, '{tlen}'),
      field('配置模式', 1, '01'),
      field('条目数', 1, '05', 'device_count'),
      field('设备配置条目', 2, '01110222033304440555', 'device_list', { unit_fields: units, when: { field: '配置模式', value: '01' } }),
      field('通道配置条目', 2, 'A1B1A2B2A3B3A4B4A5B5', 'device_list', { unit_fields: units, when: { field: '配置模式', value: '02' } }),
      field('数据长度', 2, '{byte_length:配置模式:通道配置条目}'),
      field('校验', 2, '{crc16_ccitt_false}'),
    ];
    const protocol = { id: 1, protocol_id: 'array-test', name: '动态数组回归', def_mode: 'fields', format: 'hex', msg_type: 'Request', fields_json: JSON.stringify(fields), system_ref: 1, group_ref: 1, system_name: '回归系统', group_name: '回归分组', respond_delay_ms: 0, auto_periodic_interval_ms: 1000 };
    window.__testCalls = [];
    window.__TAURI_INTERNALS__ = { transformCallback: () => 1, unregisterCallback: () => {}, invoke: async (cmd, args) => {
      window.__testCalls.push({ cmd, args });
      if (cmd === 'list_protocols') return [protocol];
      if (cmd === 'list_nodes') return [{ id: 1, name: '回归通道', role: 'simulator', local_ip: '127.0.0.1', local_port: 19001, remote_ip: '127.0.0.1', remote_port: 19002, node_id_hex: '01', group_bind_json: '[1]', enabled: true }];
      if (cmd === 'list_systems') return [{ id: 1, name: '回归系统' }];
      if (cmd === 'list_groups') return [{ id: 1, system_ref: 1, name: '回归分组', header_enabled: false }];
      if (cmd === 'engine_status') return { listening_nodes: [], heartbeats: [], periodic_sends: [] };
      if (cmd === 'log_stats') return { total: 0, in_count: 0, out_count: 0, error_count: 0 };
      if (cmd === 'send_udp_message') return { bytes: 1, hex: 'AA' };
      if (['list_dict_sets', 'query_logs', 'list_heartbeats', 'list_terminals', 'list_flows'].includes(cmd)) return [];
      return null;
    } };
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
  });
  const input = name => page.locator(`[id="f-${name}"]`);
  const row = (index, name = '参数', list = '设备配置条目') => page.getByLabel(`${list} 第${index}条 ${name}`, { exact: true });
  const send = async () => {
    await page.getByRole('button', { name: '发送报文', exact: true }).click();
    return page.evaluate(() => window.__testCalls.filter(c => c.cmd === 'send_udp_message').at(-1)?.args.params);
  };
  try {
    await page.goto(process.env.UI_BASE_URL || 'http://127.0.0.1:1426/console');
    await page.locator('#console-协议').click();
    await page.getByRole('option', { name: '动态数组回归', exact: true }).click();
    await row(5).waitFor();
    assert.equal(await row(5).inputValue(), '55');
    await row(5).fill('EE');
    await input('条目数').fill('01');
    assert.equal(await page.locator('fieldset').count(), 1);
    await row(1).fill('CC');
    assert.equal((await send())['设备配置条目'], '01CC');
    await input('条目数').fill('05');
    assert.equal(await row(5).inputValue(), 'EE', 'shrinking and editing must preserve hidden rows');
    assert.equal((await send())['设备配置条目'], '01CC02220333044405EE');

    await input('条目数').fill('00');
    assert.equal(await page.locator('fieldset').count(), 0);
    assert.equal((await send())['设备配置条目'], '');
    await input('条目数').fill('01');
    await input('配置模式').fill('02');
    assert.equal(await row(1, '参数', '通道配置条目').inputValue(), 'B1');
    await row(1, '参数', '通道配置条目').fill('DD');
    let params = await send();
    assert.equal(params['通道配置条目'], 'A1DD');
    assert.equal(params['设备配置条目'], undefined, 'inactive arrays must not enter send parameters');
    await input('配置模式').fill('01');
    await input('条目数').fill('05');
    assert.equal(await row(5).inputValue(), 'EE', 'branch switches must preserve the complete independent draft');
    assert.equal(await row(1).inputValue(), 'CC');
    await input('条目数').fill('06');
    assert.equal(await row(6).inputValue(), '99');
    await row(6).fill('AA');
    assert.equal((await send())['设备配置条目'], '01CC02220333044405EE09AA');

    for (const name of ['总长', '数据长度', '校验']) {
      assert.equal(await input(name).getAttribute('readonly'), '', `${name} must be read only`);
      assert.match(await input(name).inputValue(), /自动计算/);
    }
    params = await send();
    for (const name of ['总长', '数据长度', '校验']) assert.equal(params[name], undefined);
    for (const invalid of ['GG', '1', 'FF']) {
      await input('条目数').fill(invalid);
      const sentBefore = await page.evaluate(() => window.__testCalls.filter(c => c.cmd === 'send_udp_message').length);
      await send();
      assert.equal(await page.evaluate(() => window.__testCalls.filter(c => c.cmd === 'send_udp_message').length), sentBefore, `invalid count ${invalid} must block send`);
      assert.ok(await page.getByRole('alert').count());
    }
    await input('条目数').fill('01');
    assert.equal((await send())['设备配置条目'], '01CC');

    await page.getByRole('link', { name: '协议管理', exact: true }).click();
    await page.getByTitle('编辑', { exact: true }).click();
    await page.getByRole('button', { name: '保存', exact: true }).click();
    const saved = await page.evaluate(() => window.__testCalls.filter(c => c.cmd === 'upsert_protocol').at(-1));
    assert.ok(saved, 'literal array defaults must survive protocol save validation');
    assert.equal(saved.args.payload.fields.find(f => f.name === '设备配置条目').value, '01110222033304440555');
    await page.getByTitle('编辑', { exact: true }).click();
    const definitionValue = name => page.getByLabel(`字段值：${name}`, { exact: true });
    const savesBefore = await page.evaluate(() => window.__testCalls.filter(c => c.cmd === 'upsert_protocol').length);
    await definitionValue('条目数').fill('0102');
    await page.getByRole('button', { name: '保存', exact: true }).click();
    assert.equal(await page.evaluate(() => window.__testCalls.filter(c => c.cmd === 'upsert_protocol').length), savesBefore, 'scalar defaults must still have the exact declared byte length');
    await definitionValue('条目数').fill('05');
    await page.getByLabel('展开默认条目数据：设备配置条目', { exact: true }).click();
    await page.getByLabel('默认条目数据（HEX）：设备配置条目', { exact: true }).fill('011');
    await page.getByRole('button', { name: '保存', exact: true }).click();
    assert.equal(await page.evaluate(() => window.__testCalls.filter(c => c.cmd === 'upsert_protocol').length), savesBefore, 'array defaults must contain complete records');
    assert.deepEqual(errors, []);
    console.log('PASS: dynamic array counts, send parameters, zero/invalid counts, defaults, hidden edits, independent branches, computed fields, protocol save');
  } finally {
    await browser.close();
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
