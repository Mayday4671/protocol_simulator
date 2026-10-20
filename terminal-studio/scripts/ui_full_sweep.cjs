/**
 * 全页面扫掠: 每页零外滚 + 无控制台错误; 关键弹窗底栏按钮在矮视口下完整可达。
 * 用法: node scripts/ui_full_sweep.cjs  (需 vite 1422)
 */
const { chromium } = require("playwright");
const path = require("path");
const root = path.resolve(__dirname, "..");

const f = (name, len, value, extra = {}) => ({ name, len, value, enc: "hex", role: "none", ...extra });
const header = domain => [f("业务域", 1, domain), f("消息码", 1, "12", { role: "type_code" }), f("源终端", 4, "{node_id}", { role: "src_id" }), f("序号", 2, "{seq}", { role: "correlation" }), f("会话", 4, "{session}", { role: "session" })];
const fields = [
  f("安装区域", 2, "0001", { profile_key: "zone" }),
  f("业务模式", 1, "01", { profile_key: "mode", dict: "mode" }),
  f("条目数", 1, "02", { profile_key: "count", role: "device_count", when: { field: "业务模式", value: "01" } }),
  f("测点列表", 3, "{vlen}", { profile_key: "points", role: "device_list", when: { field: "业务模式", value: "01" }, unit_fields: [f("编号", 2, "0001"), f("启用", 1, "01", { dict: "enabled" })] }),
  f("备用地址", 4, "7F000001", { profile_key: "backup", when: { field: "业务模式", value: "02" } }),
  f("尾标", 2, "55AA"),
];
const systems = [{ id: 1, name: "环境采集协议", description: null, header_enabled: false, header_fields_json: null, created_at: "" }];
const groups = [
  { id: 11, system_ref: 1, name: "参数配置", header_enabled: true, header_fields_json: JSON.stringify(header("02")), created_at: "" },
  { id: 12, system_ref: 1, name: "业务上报", header_enabled: true, header_fields_json: JSON.stringify(header("03")), created_at: "" },
];
const protocol = (id, protocol_id, name, group_ref, over = {}) => ({
  id, protocol_id, name, group_ref, group_name: groups.find(g => g.id === group_ref).name,
  system_ref: 1, system_name: systems[0].name, format: "hex", def_mode: "fields", msg_type: "Request",
  fields_json: JSON.stringify(fields), payload_template: "", match_rule: null, match_fields_json: "[]",
  auto_respond: null, respond_delay_ms: 0, auto_periodic: null, auto_periodic_interval_ms: 0,
  description: null, created_at: "", updated_at: "", ...over,
});
const protocols = [
  protocol(1, "configure", "批量配置请求", 11),
  protocol(2, "report", "状态上报", 12, { fields_json: JSON.stringify([f("消息码", 1, "21", { hdr_ref: "消息码" }), f("所在区域", 2, "0001", { profile_key: "zone" }), f("业务值", 2, "0000")]) }),
  protocol(3, "tpl", "原始模板协议", 12, { def_mode: "template", payload_template: "01 02 03\n{node_id}\nFF", fields_json: "[]" }),
];
const dictionaries = [
  { id: 1, key: "mode", name: "业务模式", byte_len: 1, group_ref: 11, system_ref: 1, system_name: "环境采集协议", group_name: "参数配置", description: null, created_at: "", items: [{ id: 1, set_ref: 1, value: "01", label: "测点", sort_order: 0 }, { id: 2, set_ref: 1, value: "02", label: "备用", sort_order: 1 }] },
  { id: 2, key: "enabled", name: "启用", byte_len: 1, group_ref: 11, system_ref: 1, system_name: "环境采集协议", group_name: "参数配置", description: null, created_at: "", items: [{ id: 3, set_ref: 2, value: "00", label: "关闭", sort_order: 0 }, { id: 4, set_ref: 2, value: "01", label: "启用", sort_order: 1 }] },
];
const profiles = [
  { id: 1, name: "设备 A", node_ref: 1, system_ref: 1, protocol_ref: "configure", device_key: "01000001", fields_json: JSON.stringify({ zone: "00A1", mode: "01", count: "02", points: "00A10100A200" }), session_hex: "A1A1A1A1" },
  { id: 2, name: "设备 B", node_ref: 1, system_ref: 1, protocol_ref: "configure", device_key: "01000002", fields_json: JSON.stringify({ zone: "00B1", mode: "02", backup: "C0A80102" }), session_hex: null },
  ...Array.from({ length: 10 }, (_, i) => ({ id: 3 + i, name: `设备 ${String.fromCharCode(67 + i)}`, node_ref: 1, system_ref: 1, protocol_ref: "configure", device_key: `0100${(3 + i).toString(16).toUpperCase().padStart(2, "0")}${i.toString(16).toUpperCase().padStart(2, "0")}`, fields_json: JSON.stringify({ zone: "00A1", mode: "01" }), session_hex: null })),
];
const nodes = [{ id: 1, name: "物联网通道", role: "simulator", local_ip: "127.0.0.1", local_port: 31991, remote_ip: "127.0.0.1", remote_port: 31992, node_id_hex: "01000001", session_hex: null, group_bind_json: "[11,12]", enabled: true, created_at: "", updated_at: "" }];

const VIEWPORTS = [{ width: 1366, height: 690 }, { width: 1440, height: 810 }];
const PAGES = [["总览", "/"], ["通道管理", "/nodes"], ["协议管理", "/protocols"], ["字典管理", "/dict"], ["设备管理", "/devices"], ["业务流程", "/flows"], ["交互控制台", "/console"], ["实时监控", "/monitor"]];
const issues = [];
const note = (vp, where, what) => issues.push(`${vp.height}高 · ${where}: ${what}`);

async function pageHealth(page, vp, where) {
  const overflow = await page.evaluate(() => {
    const el = document.scrollingElement;
    return { delta: el.scrollHeight - el.clientHeight, wide: el.scrollWidth - el.clientWidth };
  });
  if (overflow.delta > 1) note(vp, where, `页面出现外滚(纵向多 ${overflow.delta}px)`);
  if (overflow.wide > 1) note(vp, where, `页面横向溢出 ${overflow.wide}px`);
}

async function assertButtonsReachable(page, vp, where, names) {
  const bad = await page.evaluate((names) => {
    const out = [];
    for (const name of names) {
      const dlg = [...document.querySelectorAll('[role="dialog"][data-state="open"]')].pop();
    const btn = [...dlg.querySelectorAll('button')].find(b => b.textContent.trim() === name && b.offsetParent !== null);
      if (!btn) { out.push(`${name}(未找到)`); continue; }
      const r = btn.getBoundingClientRect();
      const off = Math.max(0, Math.round(r.bottom - innerHeight), Math.round(-r.top));
      const hit = document.elementFromPoint(r.left + r.width / 2, Math.min(r.top + r.height / 2, innerHeight - 1));
      if (off > 0 || !hit || hit.closest("button") !== btn) out.push(`${name}(裁切${off ? off + "px" : ""}${hit && hit.closest("button") === btn ? "" : "/点击被挡"})`);
    }
    return out;
  }, names);
  if (bad.length) note(vp, where, `弹窗底栏按钮不可达 → ${bad.join("、")}`);
}

async function openDialogAndCheck(page, vp, where, open, footerButtons, closeBy = "取消") {
  const base = await page.locator('[role="dialog"][data-state="open"]').count();
  try {
    await open();
    await page.waitForFunction(
      (b) => document.querySelectorAll('[role="dialog"][data-state="open"]').length > b,
      base,
      { timeout: 9000 },
    );
    await page.waitForTimeout(450); // 开合动画
    await pageHealth(page, vp, where);
    await assertButtonsReachable(page, vp, where, footerButtons);
    const closer = page.locator('[role="dialog"][data-state="open"]').last().locator("button", { hasText: closeBy }).last();
    if (await closer.count()) await closer.click();
    else await page.keyboard.press("Escape");
    await page.waitForFunction(
      (b) => document.querySelectorAll('[role="dialog"][data-state="open"]').length <= b,
      base,
      { timeout: 9000 },
    );
  } catch (e) {
    note(vp, where, `弹窗流程失败: ${String(e).split("\n")[0].slice(0, 160)}`);
    await page.keyboard.press("Escape").catch(() => {});
  }
}

(async () => {
  const browser = await chromium.launch({ channel: "chrome", headless: true });
  for (const vp of VIEWPORTS) {
    const page = await browser.newPage({ viewport: vp });
    page.setDefaultTimeout(9000);
    const errors = [];
    page.on("pageerror", e => errors.push(`pageerror: ${String(e).slice(0, 160)}`));
    page.on("console", m => { if (m.type() === "error") errors.push(`console: ${m.text().slice(0, 160)}`); });
    await page.addInitScript(() => {
      window.calls = [];
      const callbacks = new Map(), listeners = new Map();
      let id = 0;
      window.__TAURI_INTERNALS__ = {
        transformCallback: cb => { callbacks.set(++id, cb); return id; }, unregisterCallback: id => callbacks.delete(id),
        invoke: async (cmd, args) => {
          window.calls.push({ cmd, args });
          if (cmd === "plugin:event|listen") { listeners.set(++id, args); return id; }
          if (cmd === "plugin:event|unlisten") { listeners.delete(args.eventId); return; }
          if (cmd === "list_protocols") return structuredClone(window.__seed.protocols);
          if (cmd === "list_dict_sets") return structuredClone(window.__seed.dictionaries);
          if (cmd === "list_systems") return structuredClone(window.__seed.systems);
          if (cmd === "list_groups") return structuredClone(window.__seed.groups);
          if (cmd === "list_terminals") return structuredClone(window.__seed.profiles);
          if (cmd === "list_nodes") return structuredClone(window.__seed.nodes);
          if (cmd === "query_logs") return [];
          if (cmd === "list_flows") return [{ id: 1, name: "设备接入全流程", system_ref: 1, description: null, steps: [
            { id: "reg", name: "逐台开机注册", x: 0, y: 0, protocol_id: "configure", terminal_mode: "each", terminal_id: null, params: {}, expect_protocol: null, timeout_ms: 3000, delay_ms: 0, next: "beat", on_success: "beat", on_failure: null, on_timeout: null },
            { id: "beat", name: "心跳保活", x: 0, y: 0, protocol_id: "configure", terminal_mode: "single", terminal_id: null, params: {}, expect_protocol: null, timeout_ms: 3000, delay_ms: 0, next: null, on_success: null, on_failure: null, on_timeout: null },
          ], trigger_protocols: [], schedule_interval_ms: 0 }];
          if (cmd === "engine_status") return { listening_nodes: [], running_heartbeats: [], periodic_sends: [] };
          if (cmd === "log_stats") return { today_in: 0, today_out: 0, total: 0 };
          if (cmd === "export_protocols_xml_text") return "<simterm-import></simterm-import>";
          if (/^(upsert|update|create|delete|import|start|send|stop)_/.test(cmd)) return null;
          return [];
        },
      };
      window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
    });
    await page.addInitScript(seed => { window.__seed = seed; }, { protocols, groups, systems, dictionaries, profiles, nodes });

    for (const [label, route] of PAGES) {
      errors.length = 0;
      await page.goto("http://127.0.0.1:1422" + route);
      await page.waitForTimeout(900);
      await pageHealth(page, vp, label);
      for (const e of errors) note(vp, label, e);
    }

    // ---- 协议管理弹窗钻取 ----
    errors.length = 0;
    await page.goto("http://127.0.0.1:1422/protocols");
    await page.waitForTimeout(600);
    await openDialogAndCheck(page, vp, "协议编辑(字段模式·分支+列表)", () => page.getByTitle("编辑", { exact: true }).first().click(), ["保存", "取消"]);
    await openDialogAndCheck(page, vp, "协议编辑(模板模式)",
      () => page.getByTitle("编辑", { exact: true }).nth(2).click(), ["保存", "取消"]);
    // 批量/按分支/XML 弹窗都从编辑弹窗内部打开
    await page.getByTitle("编辑", { exact: true }).nth(0).click();
    await page.waitForTimeout(450);
    await openDialogAndCheck(page, vp, "批量添加", () => page.getByRole("button", { name: "批量添加", exact: true }).click(), ["添加", "取消"], "取消");
    await openDialogAndCheck(page, vp, "按分支添加", () => page.getByRole("button", { name: "按分支添加", exact: true }).click(), ["添加 整个分支", "取消"], "取消");
    try {
      await page.locator('[role="dialog"][data-state="open"]').last().locator("button", { hasText: "取消" }).last().click();
      await page.waitForFunction(() => document.querySelectorAll('[role="dialog"][data-state="open"]').length === 0, null, { timeout: 9000 });
    } catch (e) { note(vp, "关闭协议编辑", String(e).split("\n")[0].slice(0, 140)); }
    await openDialogAndCheck(page, vp, "XML 在线编辑", () => page.getByRole("button", { name: "XML 编辑", exact: true }).click(), ["应用(立即生效)", "取消"], "取消");
    for (const e of errors) note(vp, "协议管理弹窗", e);

    // ---- 业务流程画布 ----
    errors.length = 0;
    await page.goto("http://127.0.0.1:1422/flows");
    await page.waitForTimeout(800);
    try {
      await page.getByRole("button", { name: /设备接入全流程/ }).click();
      await page.getByRole("button", { name: "运行", exact: true }).waitFor();
      const nodeCount = await page.locator(".react-flow__node").count();
      if (nodeCount < 2) note(vp, "业务流程", `画布节点数异常: ${nodeCount}`);
      const pos = await page.evaluate(() => [...document.querySelectorAll(".react-flow__node")].map((n) => `${Math.round(n.getBoundingClientRect().x)},${Math.round(n.getBoundingClientRect().y)}`));
      if (new Set(pos).size < nodeCount) note(vp, "业务流程", "画布节点全部重叠(XML 导入无坐标未自动布局)");
      await page.locator(".react-flow__node").first().click();
      await page.waitForTimeout(300);
      if (!(await page.getByLabel("步骤发送协议").count())) note(vp, "业务流程", "点击节点未出现属性面板");
      const runBtn = page.getByRole("button", { name: "运行", exact: true });
      const r = await runBtn.boundingBox();
      if (!r || r.bottom > vp.height) note(vp, "业务流程", "运行按钮不可见");
    } catch (e) { note(vp, "业务流程", `流程失败: ${String(e).split(String.fromCharCode(10))[0].slice(0, 140)}`); }
    for (const e of errors) note(vp, "业务流程", e);

    // ---- 通道 / 字典 / 设备 ----
    errors.length = 0;
    await page.goto("http://127.0.0.1:1422/nodes");
    await page.waitForTimeout(500);
    await openDialogAndCheck(page, vp, "新增通道", () => page.getByRole("button", { name: "新增通道", exact: true }).click(), ["保存", "取消"]);
    // 真实数据量(12台→分页出现): 分页栏必须钉底可见
    await page.goto("http://127.0.0.1:1422/devices");
    await page.getByRole("button", { name: /^编辑设备：/ }).first().waitFor();
    await page.waitForTimeout(400);
    const pag = await page.evaluate(() => {
      const b = [...document.querySelectorAll("button")].find(x => (x.getAttribute("aria-label") || "").includes("下一页设备"));
      if (!b) return "分页按钮不存在";
      const r = b.getBoundingClientRect();
      return r.top >= 0 && r.bottom <= innerHeight ? null : `分页按钮越界(bottom=${Math.round(r.bottom)})`;
    });
    if (pag) note(vp, "设备列表", pag);
    for (const e of errors) note(vp, "通道管理", e);

    errors.length = 0;
    await page.goto("http://127.0.0.1:1422/dict");
    await page.getByRole("navigation", { name: "字典树" }).getByText("环境采集协议", { exact: true }).waitFor();
    await page.getByRole("navigation", { name: "字典树" }).getByRole("button", { name: /参数配置/ }).click();
    await page.waitForTimeout(400);
    await openDialogAndCheck(page, vp, "新增字典", () => page.getByTitle("新增字典", { exact: true }).click(), ["保存", "取消"]);
    for (const e of errors) note(vp, "字典管理", e);

    errors.length = 0;
    await page.goto("http://127.0.0.1:1422/devices");
    await page.waitForTimeout(600);
    try {
      await page.getByRole("button", { name: "编辑设备：设备 A", exact: true }).click();
      await page.getByRole("region", { name: "设备配置表单", exact: true }).waitFor();
      await page.waitForTimeout(500);
      await pageHealth(page, vp, "设备编辑器");
      const save = page.getByRole("button", { name: "保存设备", exact: true });
      if (!(await save.isVisible())) note(vp, "设备编辑器", "保存设备按钮不可见");
      await page.getByRole("button", { name: "取消设备编辑", exact: true }).click();
      await page.getByRole("region", { name: "设备配置表单", exact: true }).waitFor({ state: "detached" });
    } catch (e) { note(vp, "设备编辑器", `流程失败: ${String(e).split("\n")[0].slice(0, 140)}`); }
    for (const e of errors) note(vp, "设备管理", e);

    // ---- 控制台表单(分支联动+设备列表) ----
    errors.length = 0;
    await page.goto("http://127.0.0.1:1422/console");
    await page.waitForTimeout(800);
    try {
      await page.getByRole("combobox").nth(1).click();
      await page.getByRole("option", { name: "参数配置", exact: true }).click();
      await page.getByRole("combobox").nth(2).click();
      await page.getByRole("option", { name: "批量配置请求", exact: true }).click();
      await page.locator('[id^="f-"]').first().waitFor();
      await page.waitForTimeout(400);
      await pageHealth(page, vp, "控制台表单");
      const send = page.getByRole("button", { name: /发送/ }).first();
      if (!(await send.isVisible())) note(vp, "控制台", "发送按钮不可见");
    } catch (e) { note(vp, "控制台", `流程失败: ${String(e).split("\n")[0].slice(0, 140)}`); }
    for (const e of errors) note(vp, "控制台", e);

    await page.screenshot({ path: path.join(root, `artifacts/sweep-console-${vp.height}.png`) }).catch(() => {});
    await page.close();
  }
  await browser.close();

  if (issues.length) {
    console.log(`FAIL 共 ${issues.length} 项:`);
    for (const i of issues) console.log(" -", i);
    process.exitCode = 1;
  } else {
    console.log("PASS 全页面零外滚 · 无控制台错误 · 全弹窗底栏按钮双视口可达");
  }
})();
