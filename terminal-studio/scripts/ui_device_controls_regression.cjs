async function controlValue(locator) {
  const tag = await locator.evaluate(e => e.tagName);
  return tag === 'BUTTON' ? (await locator.innerText()).split(' · ')[0] : locator.inputValue();
}
async function chooseControl(page, label, name) {
  await page.getByLabel(label, {exact:true}).click();
  await page.getByRole('option', {name}).click();
}
// Real React components; only native IPC/event delivery is replaced for the browser.
const { chromium } = require('playwright');
const assert = require('node:assert/strict');
const path = require('node:path');
const root = path.resolve(__dirname, '..');
const f = (name, len, value, extra = {}) => ({ name, len, value, enc: 'hex', role: 'none', ...extra });
const header = domain => [f('业务域', 1, domain), f('消息码', 1, '12', { role: 'type_code' }), f('源终端', 4, '{node_id}', { role: 'src_id' }), f('序号', 2, '{seq}', { role: 'correlation' }), f('会话', 4, '{session}', { role: 'session' })];
const fields = [f('安装区域', 2, '0001', { profile_key: 'zone' }), f('业务模式', 1, '01', { profile_key: 'mode', dict: 'mode' }), f('条目数', 1, '02', { profile_key: 'count', role: 'device_count', when: { field: '业务模式', value: '01' } }), f('测点列表', 3, '{vlen}', { profile_key: 'points', role: 'device_list', when: { field: '业务模式', value: '01' }, unit_fields: [f('编号', 2, '0001'), f('启用', 1, '01', { dict: 'enabled' })] }), f('备用地址', 4, '7F000001', { profile_key: 'backup', when: { field: '业务模式', value: '02' } }), f('尾标', 2, '55AA')];
const systems = [{ id: 1, name: '环境采集协议', description: null, header_enabled: false, header_fields_json: null, created_at: '' }];
const groups = [{ id: 11, system_ref: 1, name: '参数配置', header_enabled: true, header_fields_json: JSON.stringify(header('02')), created_at: '' }, { id: 12, system_ref: 1, name: '业务上报', header_enabled: true, header_fields_json: JSON.stringify(header('03')), created_at: '' }];
const protocol = (id, protocol_id, name, group_ref, body) => ({ id, protocol_id, name, group_ref, group_name: groups.find(g => g.id === group_ref).name, system_ref: 1, system_name: systems[0].name, format: 'hex', def_mode: 'fields', msg_type: 'Request', fields_json: JSON.stringify(body), payload_template: '', match_rule: null, match_fields_json: '[]', auto_respond: null, respond_delay_ms: 0, auto_periodic: null, auto_periodic_interval_ms: 0, description: null, created_at: '', updated_at: '' });
const protocols = [protocol(1, 'configure', '批量配置请求', 11, fields), protocol(2, 'report', '状态上报', 12, [f('消息码', 1, '21', { hdr_ref: '消息码' }), f('所在区域', 2, '0001', { profile_key: 'zone' }), f('业务值', 2, '0000')])];
const dictionaries = [{ id: 1, key: 'mode', name: '业务模式', byte_len: 1, group_ref: null, system_ref: null, system_name: null, group_name: null, description: null, created_at: '', items: [{ id: 1, set_ref: 1, value: '01', label: '测点', sort_order: 0 }, { id: 2, set_ref: 1, value: '02', label: '备用', sort_order: 1 }] }, { id: 2, key: 'enabled', name: '启用', byte_len: 1, group_ref: null, system_ref: null, system_name: null, group_name: null, description: null, created_at: '', items: [{ id: 3, set_ref: 2, value: '00', label: '关闭', sort_order: 0 }, { id: 4, set_ref: 2, value: '01', label: '启用', sort_order: 1 }] }];
const profiles = [{ id: 1, name: '设备 A', node_ref: 1, system_ref: 1, protocol_ref: 'configure', device_key: '01000001', fields_json: JSON.stringify({ zone: '00A1', mode: '01', count: '02', points: '00A10100A200' }), session_hex: 'A1A1A1A1' }, { id: 2, name: '设备 B', node_ref: 1, system_ref: 1, protocol_ref: 'configure', device_key: '01000002', fields_json: JSON.stringify({ zone: '00B1', mode: '02', backup: 'C0A80102' }), session_hex: 'B2B2B2B2' }];

async function realProfiles(browser) {
  const { DatabaseSync } = require('node:sqlite');
  const db = new DatabaseSync(process.env.DEVICE_FIXTURE_DB || path.join(root, 'artifacts/device-workspace-validation.db'), { readOnly: true });
  const data = {
    systems: db.prepare('SELECT * FROM protocol_systems').all(),
    groups: db.prepare('SELECT * FROM protocol_groups').all(),
    nodes: db.prepare('SELECT * FROM nodes').all(),
    profiles: db.prepare("SELECT * FROM terminal_profiles WHERE node_ref=(SELECT id FROM nodes WHERE name='设备模拟通道') ORDER BY id").all(),
    protocols: db.prepare('SELECT p.*,s.name system_name,g.name group_name FROM protocols p JOIN protocol_systems s ON s.id=p.system_ref JOIN protocol_groups g ON g.id=p.group_ref').all(),
    dictionaries: db.prepare('SELECT * FROM dict_sets').all(),
  };
  for (const d of data.dictionaries) d.items = db.prepare('SELECT * FROM dict_items WHERE set_ref=? ORDER BY sort_order,id').all(d.id);
  db.close();
  assert.equal(data.profiles.length, 18);
  const page = await browser.newPage({ viewport: { width: 1500, height: 1000 } });
  page.setDefaultTimeout(7000);
  const errors = [];
  page.on('pageerror', e => errors.push(String(e)));
  await page.addInitScript(data => {
    window.calls = [];
    window.__TAURI_INTERNALS__ = { transformCallback: () => 1, unregisterCallback: () => {}, invoke: async (cmd, args) => {
      window.calls.push({ cmd, args });
      const key = ({ list_protocols: 'protocols', list_groups: 'groups', list_systems: 'systems', list_terminals: 'profiles', list_nodes: 'nodes', list_dict_sets: 'dictionaries' })[cmd];
      if (key) return structuredClone(data[key]);
      if (cmd === 'engine_status') return { listening_nodes: [], running_heartbeats: [], periodic_sends: [] };
      if (cmd === 'log_stats') return { today_in: 0, today_out: 0, total: 0 };
      if (cmd === 'save_terminal') { data.profiles = data.profiles.map(p => p.id === args.payload.id ? { ...args.payload, session_hex: p.session_hex } : p); return; }
      if (/^(upsert|update|create|delete|import|start|send)_/.test(cmd)) throw new Error('Unexpected IPC: ' + cmd);
      return [];
    } };
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
  }, data);
  try {
    await page.goto('http://127.0.0.1:1422/devices');
    await page.getByRole('button', { name: '编辑设备：' + data.profiles[0].name, exact: true }).waitFor();
    assert.equal(await page.getByRole('button', { name: /^编辑设备：/ }).count(), 10);
    await page.getByRole('button', { name: '下一页设备', exact: true }).click();
    assert.equal(await page.getByRole('button', { name: /^编辑设备：/ }).count(), 8);
    for (const profile of data.profiles) {
      await page.getByLabel('搜索设备', { exact: true }).fill(profile.device_key);
      await page.getByRole('button', { name: '编辑设备：' + profile.name, exact: true }).click();
      const body = JSON.parse(data.protocols.find(p => p.protocol_id === profile.protocol_ref).fields_json);
      const properties = JSON.parse(profile.fields_json);
      for (const field of body.filter(f => f.profile_key && properties[f.profile_key] !== undefined)) {
        if (field.role !== 'device_list') {
          assert.equal(await controlValue(page.getByLabel(field.name, { exact: true })), properties[field.profile_key], `${profile.name}: ${field.name}`);
          continue;
        }
        const raw = properties[field.profile_key];
        const recordSize = field.unit_fields.reduce((sum, f) => sum + f.len * 2, 0);
        for (let i = 0; i < raw.length / recordSize; i++) {
          let offset = i * recordSize;
          for (const unit of field.unit_fields) {
            const hex = raw.slice(offset, offset + unit.len * 2); offset += unit.len * 2;
            const value = unit.enc === 'uint' && !unit.dict ? BigInt('0x' + hex).toString() : unit.enc === 'ascii' && !unit.dict ? Buffer.from(hex, 'hex').toString('utf8').replace(/\0+$/, '') : hex;
            assert.equal(await controlValue(page.getByLabel(`${field.name} 第${i + 1}条 ${unit.name}`, { exact: true })), value, `${profile.name}: ${field.name} #${i + 1} ${unit.name}`);
          }
        }
      }
      await page.getByRole('button', { name: '保存设备', exact: true }).click();
      try {
        await page.getByRole('region', { name: '设备配置表单', exact: true }).waitFor({ state: 'detached', timeout: 5000 });
      } catch {
        console.log('SAVE-BLOCKED', profile.name, JSON.stringify(await page.locator('[data-sonner-toast]').allInnerTexts()));
        throw new Error('save blocked');
      }
      const saved = await page.evaluate(() => window.calls.findLast(c => c.cmd === 'save_terminal').args.payload);
      assert.deepEqual(JSON.parse(saved.fields_json), properties, `${profile.name}: editing never overwrites stored lists or inactive branches`);
    }
    await page.getByLabel('搜索设备', { exact: true }).fill('');
    try { await page.screenshot({ timeout: 30000, path: path.join(root, 'artifacts/device-workspace-real-profiles.png') }); } catch { /* 截图失败不影响功能断言 */ }
    assert.deepEqual(errors, []);
    console.log('PASS real dataset: 18 devices across 3 protocol families; every saved scalar/list unit restored and saved losslessly; device pagination 10+8');
  } catch (e) {
    console.log('REAL-ERROR:', String(e).slice(0, 400));
    throw e;
  } finally { await page.close(); }
}

(async () => {
  const browser = await chromium.launch({ channel: 'chrome', headless: true });
  const page = await browser.newPage({ viewport: { width: 1500, height: 1000 } });
  page.setDefaultTimeout(7000);
  const errors = [];
  page.on('pageerror', e => errors.push(String(e)));
  await page.addInitScript(data => {
    window.calls = [];
    window.profiles = data.profiles;
    const callbacks = new Map(), listeners = new Map();
    let id = 0;
    window.emitTestEvent = event => { for (const [eventId, listener] of listeners) if (listener.event === event) callbacks.get(listener.handler)?.({ event, id: eventId, payload: {} }); };
    window.__TAURI_INTERNALS__ = {
      transformCallback: callback => { callbacks.set(++id, callback); return id; }, unregisterCallback: id => callbacks.delete(id),
      invoke: async (cmd, args) => {
        window.calls.push({ cmd, args });
        if (cmd === 'plugin:event|listen') { listeners.set(++id, args); return id; }
        if (cmd === 'plugin:event|unlisten') { listeners.delete(args.eventId); return; }
        if (cmd === 'list_protocols') return data.protocols;
        if (cmd === 'list_dict_sets') return data.dictionaries;
        if (cmd === 'list_systems') return data.systems;
        if (cmd === 'list_groups') return data.groups;
        if (cmd === 'list_terminals') return structuredClone(window.profiles);
        if (cmd === 'list_nodes') return [{ id: 1, name: '物联网通道', role: 'simulator', local_ip: '127.0.0.1', local_port: 31991, remote_ip: '127.0.0.1', remote_port: 31992, node_id_hex: '01000001', session_hex: null, group_bind_json: '[11,12]', enabled: true, created_at: '', updated_at: '' }];
        if (cmd === 'engine_status') return { listening_nodes: [], running_heartbeats: [], periodic_sends: [] };
        if (cmd === 'log_stats') return { today_in: 0, today_out: 0, total: 0 };
        if (cmd === 'save_terminal') { const payload = args.payload; if (payload.id) window.profiles = window.profiles.map(p => p.id === payload.id ? { ...payload, session_hex: p.session_hex } : p); else window.profiles.push({ ...payload, id: 3, session_hex: null }); return; }
        if (cmd === 'delete_terminal') { window.profiles = window.profiles.filter(p => p.id !== args.id); return; }
        if (cmd === 'send_udp_message') return { bytes: 1, hex: 'AA' };
        if (cmd === 'upsert_protocol') { const old = data.protocols.find(p => p.protocol_id === args.payload.protocol_id); Object.assign(old, args.payload, { fields_json: JSON.stringify(args.payload.fields) }); return old; }
        if (cmd === 'update_group_header') { const g = data.groups.find(g => g.id === args.id); Object.assign(g, { header_enabled: args.payload.enabled, header_fields_json: JSON.stringify(args.payload.fields) }); return g; }
        if (/^(upsert|update|create|delete|import|start)_/.test(cmd)) throw new Error('Unexpected IPC: ' + cmd);
        return [];
      },
    };
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: id => listeners.delete(id) };
  }, { systems, groups, protocols, dictionaries, profiles });
  const active = () => page.getByRole('tabpanel');
  const sidebar = name => page.locator('.app-sidebar').getByRole('link', { name, exact: true });
  async function choose(index, name) { await active().getByRole('combobox').nth(index).click(); await page.getByRole('option', { name, exact: true }).click(); }
  try {
    await page.goto('http://127.0.0.1:1422/devices');
    await page.getByRole('button', { name: '新建设备', exact: true }).waitFor();
    assert.equal(await page.getByText('初始化设备档案', { exact: true }).count(), 0);
    assert.equal(await page.getByRole('table', { name: '设备档案' }).count(), 1);
    await page.getByRole('button', { name: '编辑设备：设备 A', exact: true }).click();
    assert.equal(await controlValue(page.getByLabel('安装区域', { exact: true })), '00A1');
    assert.equal(await controlValue(page.getByLabel('测点列表 第1条 编号', { exact: true })), '00A1');
    assert.equal(await controlValue(page.getByLabel('测点列表 第2条 启用', { exact: true })), '00');
    assert.equal(await page.getByLabel('业务域', { exact: true }).count(), 0, 'packet fields are not device properties');
    // 按协议分区: 全字段协议默认展开, 键集合被覆盖的协议默认折叠但可展开补配
    assert.equal(await page.getByRole('button', { name: '收起协议分区：批量配置请求', exact: true }).count(), 1, 'superset protocol section expanded');
    await page.getByRole('button', { name: '展开协议分区：状态上报', exact: true }).click();
    assert.equal(await controlValue(page.getByLabel('所在区域', { exact: true })), '00A1', 'shared key renders the same stored value in every protocol section');
    // 同一属性 Key 跨分区联动: 改配置分区, 上报分区同 Key 字段立即同值
    await page.getByLabel('安装区域', { exact: true }).fill('00AF');
    assert.equal(await controlValue(page.getByLabel('所在区域', { exact: true })), '00AF', 'editing one section syncs the shared property key everywhere');
    await page.getByRole('button', { name: '收起协议分区：状态上报', exact: true }).click();
    await sidebar('字典管理').click();
    await page.getByRole('tab', { name: '设备管理', exact: true }).click();
    assert.equal(await controlValue(page.getByLabel('安装区域', { exact: true })), '00AF');
    await page.getByRole('button', { name: '保存设备', exact: true }).click();
    await page.getByRole('button', { name: '新建设备', exact: true }).waitFor();
    const saved = await page.evaluate(() => window.calls.findLast(c => c.cmd === 'save_terminal').args.payload);
    assert.deepEqual(JSON.parse(saved.fields_json), { zone: '00AF', mode: '01', count: '02', points: '00A10100A200' });
    await page.getByRole('button', { name: '新建设备', exact: true }).click();
    await page.getByLabel('设备名称', { exact: true }).fill('设备 C');
    await page.getByLabel('设备标识 Hex', { exact: true }).fill('01000003');
    assert.equal(await controlValue(page.getByLabel('条目数', { exact: true })), '02');
    await page.getByLabel('条目数', { exact: true }).fill('00');
    await page.getByLabel('测点列表 第1条 编号', { exact: true }).waitFor({ state: 'detached' });
    await chooseControl(page, '业务模式', /^02/);
    assert.equal(await page.getByLabel('条目数', { exact: true }).count(), 0);
    await page.getByLabel('备用地址', { exact: true }).fill('0A000003');
    await page.getByRole('button', { name: '保存设备', exact: true }).click();
    await page.getByRole('button', { name: '编辑设备：设备 C', exact: true }).waitFor();
    const created = await page.evaluate(() => window.calls.findLast(c => c.cmd === 'save_terminal').args.payload);
    assert.deepEqual(JSON.parse(created.fields_json), { zone: '0001', mode: '02', backup: '0A000003' });
    await sidebar('交互控制台').click();
    await choose(0, '环境采集协议'); await choose(1, '参数配置'); await choose(2, '批量配置请求');
    await chooseControl(page, '设备', /^设备 A/);
    assert.equal(await controlValue(page.locator('#f-安装区域')), '00AF');
    assert.equal(await controlValue(page.getByLabel('测点列表 第1条 编号', { exact: true })), '00A1');
    await page.getByRole('button', { name: '公共头', exact: true }).click();
    assert.equal(await controlValue(page.locator('#f-业务域')), '02');
    assert.equal(await controlValue(page.locator('#f-会话')), 'A1A1A1A1');
    assert.equal(await page.locator('#f-会话').isEditable(), false);
    assert.equal(await page.locator('#f-源终端').isEditable(), false);
    await page.locator('#f-安装区域').fill('00EE');
    await chooseControl(page, '设备', /^设备 B/);
    assert.equal(await controlValue(page.locator('#f-安装区域')), '00B1');
    assert.equal(await controlValue(page.locator('#f-备用地址')), 'C0A80102');
    assert.equal(await controlValue(page.locator('#f-会话')), 'B2B2B2B2');
    await chooseControl(page, '设备', /^设备 A/);
    assert.equal(await controlValue(page.locator('#f-安装区域')), '00AF');
    await choose(1, '业务上报'); await choose(2, '状态上报');
    await chooseControl(page, '设备', /^设备 A/);
    assert.equal(await controlValue(page.locator('#f-所在区域')), '00AF', 'stable property key survives renamed command field');
    assert.equal(await controlValue(page.locator('#f-业务域')), '03');
    assert.equal(await controlValue(page.locator('#f-消息码')), '21');
    await page.locator('#f-业务值').fill('1234');
    await page.evaluate(() => { window.profiles[0].session_hex = 'CCCCCCCC'; window.emitTestEvent('session-updated'); });
    await page.waitForFunction(() => document.querySelector('#f-会话')?.value === 'CCCCCCCC');
    assert.equal(await controlValue(page.locator('#f-业务值')), '1234', 'session refresh preserves unsaved command edits');
    await page.evaluate(() => { const p = window.profiles[0]; p.fields_json = JSON.stringify({ ...JSON.parse(p.fields_json), zone: '00FA' }); window.emitTestEvent('devices-updated'); });
    await page.waitForFunction(() => document.querySelector('#f-所在区域')?.value === '00FA');
    assert.equal(await controlValue(page.locator('#f-业务值')), '1234');
    await page.getByRole('button', { name: '发送报文', exact: true }).click();
    const sent = await page.evaluate(() => window.calls.findLast(c => c.cmd === 'send_udp_message').args.params);
    assert.equal(sent.__terminal_id, '1'); assert.equal(sent.业务值, '1234');
    await chooseControl(page, '设备', '手动填写');
    assert.equal(await controlValue(page.locator('#f-所在区域')), '0001');
    await sidebar('设备管理').click();
    page.once('dialog', dialog => dialog.accept());
    await page.getByRole('button', { name: '删除设备：设备 C', exact: true }).click();
    await page.getByRole('button', { name: '编辑设备：设备 C', exact: true }).waitFor({ state: 'detached' });
    await page.screenshot({ path: path.join(root, 'artifacts/device-workspace.png') });
    await sidebar('协议管理').click();
    await active().getByRole('row').filter({ hasText: '批量配置请求' }).getByTitle('编辑', { exact: true }).click();
    await page.getByRole('dialog').locator('input[value="安装区域"]').locator('..').getByTitle('字段设置', { exact: true }).click();
    await page.getByLabel('设备属性 Key：安装区域', { exact: true }).fill('zone_v2');
    await page.getByRole('dialog').getByRole('button', { name: '保存', exact: true }).click();
    const mapping = await page.evaluate(() => window.calls.findLast(c => c.cmd === 'upsert_protocol').args.payload.fields);
    assert.equal(mapping.find(f => f.name === '安装区域').profile_key, 'zone_v2');
    assert.equal(mapping.find(f => f.name === '测点列表').profile_key, 'points');
    await active().getByRole('button', { name: '参数配置', exact: true }).click();
    await active().getByRole('button', { name: /分组公共 UDP 头/ }).click();
    await active().getByRole('button', { name: '公共头字段设置：业务域', exact: true }).click();
    await active().getByLabel('公共头设备属性 Key：业务域', { exact: true }).fill('domain');
    await active().getByRole('button', { name: '保存', exact: true }).click();
    const headerSaved = await page.evaluate(() => window.calls.findLast(c => c.cmd === 'update_group_header').args.payload.fields);
    assert.equal(headerSaved.find(f => f.name === '业务域').profile_key, 'domain');
    assert.equal(headerSaved.find(f => f.name === '序号').role, 'correlation');
    assert.deepEqual(errors, []);
    console.log('PASS device create/edit/delete, explicit mappings, branch/list restoration, zero counts, A/B sessions, command codes, events, tab draft retention');
    await realProfiles(browser);
  } catch (e) { await page.screenshot({ path: path.join(root, 'artifacts/device-workspace-failure.png'), fullPage: true }).catch(() => {}); throw e; }
  finally { await browser.close(); }
})().catch(e => { console.error(e); process.exitCode = 1; });
