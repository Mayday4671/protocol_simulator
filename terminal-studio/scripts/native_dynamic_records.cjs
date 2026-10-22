// A fresh isolated SIMTERM_DB and native app CDP port 9259 are required.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const dgram = require('node:dgram');
const fs = require('node:fs');
(async () => {
  const peer = dgram.createSocket('udp4');
  await new Promise(resolve => peer.bind(0, '127.0.0.1', resolve));
  const reservation = dgram.createSocket('udp4'); await new Promise(resolve => reservation.bind(0, '127.0.0.1', resolve));
  const port = reservation.address().port; await new Promise(resolve => reservation.close(resolve));
  const wires = [];
  peer.on('message', (msg, from) => {
    wires.push(msg.toString('hex'));
    const bytes = msg[0] === 0x21 ? [0x22, msg[1], msg[2], ...Array.from({ length: msg[2] }, (_, i) => [msg[3 + i * 3], msg[1] === 1 && i === 1 ? 0 : 0xAA]).flat()] : [0x32, msg[1], msg[1] === 5 ? 0 : 0xAA];
    peer.send(Buffer.from(bytes), from.port, from.address);
  });
  const browser = await chromium.connectOverCDP('http://127.0.0.1:9259');
  const page = browser.contexts()[0].pages()[0]; page.setDefaultTimeout(15000); await page.waitForLoadState('domcontentloaded'); await page.waitForFunction(() => Boolean(window.__TAURI_INTERNALS__));
  const invoke = (c, a = {}) => page.evaluate(([c, a]) => window.__TAURI_INTERNALS__.invoke(c, a), [c, a]);
  const waitRecords = async expected => { for (let i = 0; i < 70; i++) { if ((await invoke('list_business_records')).length === expected) return; await new Promise(r => setTimeout(r, 80)); } throw Error(`Expected ${expected} records`); };
  const choose = async (label, name) => { await page.getByRole('combobox', { name: label, exact: true }).click(); await page.getByRole('option', { name, exact: true }).click(); };
  const mapping = async (name, field) => { await page.getByRole('button', { name: '添加属性', exact: true }).click(); const index = await page.getByRole('combobox', { name: /^属性名称 / }).count(); await choose(`属性名称 ${index}`, name); await choose(`字段 ${index}`, field); };
  const openRule = async name => { await page.getByRole('row').filter({ has: page.getByRole('cell', { name, exact: true }) }).getByRole('button', { name: /应答保存规则/ }).click(); await page.waitForFunction(() => document.querySelector('[role=dialog] input[type=checkbox]')?.disabled === false); await page.getByRole('checkbox').check(); };
  const saveRule = async () => { await page.getByRole('button', { name: '保存规则', exact: true }).click(); await page.getByRole('heading', { name: /应答保存规则/ }).waitFor({ state: 'hidden' }); };
  try {
    assert.deepEqual(await invoke('list_protocols'), [], 'Use a fresh isolated database');
    const system = await invoke('create_system', { payload: { name: '动态资料验收', description: '' } });
    const group = await invoke('create_group', { payload: { name: '配置', system_ref: system.id } });
    const f = (name, len, value = '', role = 'none', extra = {}) => ({ name, len, value, role, enc: 'hex', ...extra });
    for (const [id, name, type, code, fields] of [
      ['device_req', '设备配置', 'Request', '21', [f('流水号', 1, '01'), f('数量', 1, '02', 'device_count'), f('条目', 3, '011234025678', 'device_list', { unit_fields: [f('设备地址', 1), f('端口', 2)] })]],
      ['device_ack', '设备配置应答', 'Response', '22', [f('流水号', 1), f('数量', 1, '', 'device_count'), f('结果条目', 2, '', 'device_list', { unit_fields: [f('设备地址', 1), f('结果', 1)] })]],
      ['port_req', '端口配置', 'Request', '31', [f('流水号', 1, '03'), f('端口号', 1, '01'), f('所属设备', 1, '01'), f('配置值', 2, '1234')]],
      ['port_ack', '端口配置应答', 'Response', '32', [f('流水号', 1), f('结果', 1)]],
    ]) await invoke('upsert_protocol', { payload: { protocol_id: id, name, msg_type: type, format: 'hex', def_mode: 'fields', system_ref: system.id, group_ref: group.id, fields: [f('信令码', 1, code), ...fields], match_fields: [{ field: '信令码', value: code }] } });
    const node = await invoke('create_node', { payload: { name: '动态资料本机联调', role: 'simulator', local_ip: '127.0.0.1', local_port: port, remote_ip: '127.0.0.1', remote_port: peer.address().port, node_id_hex: null, enabled: true, group_binds: [group.id] } });
    await page.getByText('协议管理', { exact: true }).first().click();
    await openRule('设备配置');
    await page.getByRole('button', { name: '管理资料类型', exact: true }).click();
    const manager = page.getByRole('dialog', { name: '资料类型', exact: true });
    for (const [i, name] of ['设备地址', '端口'].entries()) { await manager.getByRole('button', { name: '添加属性', exact: true }).click(); await manager.getByLabel(`资料属性 ${i + 1}`, { exact: true }).fill(name); }
    await manager.getByRole('button', { name: '保存类型', exact: true }).click(); await manager.getByRole('button', { name: '保存类型', exact: true }).isDisabled();
    await manager.getByRole('button', { name: '新增类型', exact: true }).click();
    await manager.getByLabel('类型名称', { exact: true }).fill('端口资料');
    for (const [i, name] of ['端口号', '配置值'].entries()) { await manager.getByRole('button', { name: '添加属性', exact: true }).click(); await manager.getByLabel(`资料属性 ${i + 1}`, { exact: true }).fill(name); }
    await manager.getByRole('button', { name: '保存类型', exact: true }).click();
    await manager.getByRole('button', { name: '完成', exact: true }).click();
    const type = (await invoke('list_record_types', { system: system.id })).find(t => t.name === '端口资料');
    assert.equal(type.key_scope, 'device'); assert.equal(type.device_required, true);
    await choose('应答协议', '设备配置应答'); await choose('请求流水号', '流水号 · 1 B'); await choose('应答流水号', '流水号 · 1 B');
    await choose('请求资料', '条目'); await choose('应答结果', '结果条目'); await choose('成功结果字段', '条目 / 结果 · 1 B');
    await page.getByLabel('成功值', { exact: true }).fill('AA'); await choose('唯一标识（请求字段）', '条目 / 设备地址 · 1 B');
    await mapping('设备地址', '条目 / 设备地址 · 1 B'); await mapping('端口', '条目 / 端口 · 2 B'); await saveRule();
    await openRule('端口配置'); await choose('应答协议', '端口配置应答'); await choose('保存到', '端口资料');
    await choose('请求流水号', '流水号 · 1 B'); await choose('应答流水号', '流水号 · 1 B'); await choose('成功结果字段', '结果 · 1 B');
    await page.getByLabel('成功值', { exact: true }).fill('AA'); await choose('唯一标识（请求字段）', '端口号 · 1 B'); await choose('所属设备标识（必选）', '所属设备 · 1 B');
    await mapping('端口号', '端口号 · 1 B'); await mapping('配置值', '配置值 · 2 B'); await saveRule();
    const send = (id, params) => invoke('send_udp_message', { nodeId: node.id, protocolId: id, params });
    await send('device_req', {}); await waitRecords(1);
    await send('device_req', { '流水号': '02', '数量': '01', '条目': '025678' }); await waitRecords(2);
    await send('port_req', {}); await waitRecords(3);
    await send('port_req', { '流水号': '04', '所属设备': '02', '配置值': '5678' }); await waitRecords(4);
    let ports = (await invoke('list_business_records')).filter(r => r.kind === type.kind);
    assert.equal(ports.length, 2); assert.equal(new Set(ports.map(r => r.device_ref)).size, 2);
    await assert.rejects(() => invoke('delete_record_type', { system: system.id, kind: type.kind }), /规则/);
    await openRule('端口配置'); await choose('成功后操作', '删除对应资料'); await saveRule();
    await send('port_req', { '流水号': '05' });
    for (let i = 0; i < 40; i++) { const logs = await invoke('query_logs', { filter: { limit: 20, offset: 0 } }).catch(() => []); if (JSON.stringify(logs).includes('320500')) break; await new Promise(r => setTimeout(r, 60)); }
    // A failed ACK completes the request but leaves both related records intact.
    assert.equal((await invoke('list_business_records')).length, 4);
    await send('port_req', { '流水号': '06' }); await waitRecords(3);
    ports = (await invoke('list_business_records')).filter(r => r.kind === type.kind);
    const devices = await invoke('list_terminals');
    assert.equal(ports[0].device_ref, devices.find(d => d.device_key === '02').id);
    assert.equal(devices.length, 2);
    const xml = await invoke('export_protocols_xml_text', { protocolIds: ['device_req', 'port_req'] });
    assert.ok(xml.includes('<record-type')); assert.ok(xml.includes('operation="delete"'));
    assert.deepEqual((await invoke('import_protocols_xml_text', { xml })).failed, []);
    const rules = await invoke('list_save_rules'); assert.equal(rules.find(r => r.request === 'port_req').operation, 'delete');
    await page.getByText('设备管理', { exact: true }).first().click(); await page.getByRole('button', { name: '业务资料', exact: true }).click();
    await choose('资料类型筛选', '端口资料 · 动态资料验收');
    await page.locator('summary').filter({ hasText: '端口资料' }).click();
    await page.getByText('所属设备：02', { exact: true }).waitFor();
    fs.mkdirSync('artifacts/dynamic-records', { recursive: true }); await page.screenshot({ path: 'artifacts/dynamic-records/native-records.png' });
    await page.getByRole('button', { name: '资料类型', exact: true }).click();
    await page.getByRole('button', { name: '端口资料', exact: true }).click();
    await page.screenshot({ path: 'artifacts/dynamic-records/native-types.png' });
    fs.writeFileSync('artifacts/dynamic-records/native-result.json', JSON.stringify({ wires, ports, devices, type, rules, xmlRoundtrip: true }, null, 2));
    await invoke('stop_all');
    console.log('PASS native: client-defined type + mapped rules, real UDP partial success, same ID on two devices, failed delete ignored, successful scoped delete, XML roundtrip, filters');
  } finally { peer.close(); await browser.close(); }
})().catch(e => { console.error(e); process.exit(1); });
