/**
 * 产品标准视觉审查: 真实工件库数据渲染全部页面/关键弹窗, 双视口截图。
 * 产物: artifacts/review-<surface>-<vp>.png
 */
const { chromium } = require("playwright");
const path = require("path");
const root = path.resolve(__dirname, "..");

async function loadFixtures() {
  const { DatabaseSync } = require("node:sqlite");
  const db = new DatabaseSync(path.join(root, "artifacts/device-workspace-validation.db"), { readOnly: true });
  const data = {
    systems: db.prepare("SELECT * FROM protocol_systems").all(),
    groups: db.prepare("SELECT * FROM protocol_groups").all(),
    nodes: db.prepare("SELECT * FROM nodes").all(),
    profiles: db.prepare("SELECT * FROM terminal_profiles ORDER BY id").all(),
    protocols: db.prepare("SELECT p.*, s.name system_name, g.name group_name FROM protocols p JOIN protocol_systems s ON s.id=p.system_ref JOIN protocol_groups g ON g.id=p.group_ref").all(),
    dictionaries: db.prepare("SELECT * FROM dict_sets").all(),
    logs: db.prepare("SELECT * FROM logs ORDER BY id DESC LIMIT 60").all(),
  };
  for (const d of data.dictionaries) d.items = db.prepare("SELECT * FROM dict_items WHERE set_ref=? ORDER BY sort_order,id").all(d.id);
  db.close();
  return data;
}

(async () => {
  const data = await loadFixtures();
  const browser = await chromium.launch({ channel: "chrome", headless: true });
  const shots = [];
  for (const vp of [{ width: 1366, height: 690 }, { width: 1440, height: 810 }]) {
    const page = await browser.newPage({ viewport: vp });
    page.setDefaultTimeout(12000);
    await page.addInitScript(() => {
      window.__TAURI_INTERNALS__ = { transformCallback: () => 1, unregisterCallback: () => {},
        invoke: async (cmd) => {
          if (cmd === "plugin:event|listen") return 1;
          if (cmd === "plugin:event|unlisten") return;
          if (cmd === "list_protocols") return structuredClone(window.__seed.protocols);
          if (cmd === "list_dict_sets") return structuredClone(window.__seed.dictionaries);
          if (cmd === "list_systems") return structuredClone(window.__seed.systems);
          if (cmd === "list_groups") return structuredClone(window.__seed.groups);
          if (cmd === "list_terminals") return structuredClone(window.__seed.profiles);
          if (cmd === "list_nodes") return structuredClone(window.__seed.nodes);
          if (cmd === "query_logs") return structuredClone(window.__seed.logs);
          if (cmd === "engine_status") return { listening_nodes: [], running_heartbeats: [], periodic_sends: [] };
          if (cmd === "log_stats") return { today_in: 30, today_out: 30, total: 114 };
          return [];
        } };
      window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
    });
    await page.addInitScript(seed => { window.__seed = seed; }, data);
    const snap = async (name) => {
      await page.waitForTimeout(700);
      const file = path.join(root, `artifacts/review-${name}-${vp.height}.png`);
      await page.screenshot({ path: file });
      shots.push(file);
    };

    const step = async (name, fn) => { try { await fn(); } catch (e) { console.log("STEP-FAIL", name, String(e).split("\n")[0].slice(0, 140)); } };
    await step("dashboard", async () => { await page.goto("http://127.0.0.1:1422/"); await snap("dashboard"); });
    await step("nodes", async () => { await page.goto("http://127.0.0.1:1422/nodes"); await snap("nodes"); });
    await step("protocols", async () => { await page.goto("http://127.0.0.1:1422/protocols"); await snap("protocols"); });
    await step("protocol-editor", async () => {
      await page.goto("http://127.0.0.1:1422/protocols");
      await page.getByTitle("编辑", { exact: true }).nth(3).click();
      await page.locator('[role="dialog"][data-state="open"]').last().waitFor();
      await page.waitForTimeout(600);
      await snap("protocol-editor");
      await page.locator('[role="dialog"][data-state="open"]').last().locator("button", { hasText: "取消" }).last().click();
      await page.waitForTimeout(300);
    });
    await step("dict", async () => { await page.goto("http://127.0.0.1:1422/dict"); await page.waitForTimeout(600); await snap("dict"); });
    await step("devices", async () => { await page.goto("http://127.0.0.1:1422/devices"); await snap("devices"); });
    await step("device-editor", async () => {
      await page.getByRole("button", { name: /^编辑设备：/ }).first().click();
      await page.getByRole("region", { name: "设备配置表单", exact: true }).waitFor();
      await snap("device-editor");
    });
    await step("console", async () => {
      await page.goto("http://127.0.0.1:1422/console");
      await page.waitForTimeout(600);
      await page.getByRole("combobox").nth(1).click();
      await page.getByRole("option", { name: "参数配置", exact: true }).first().click();
      await page.getByRole("combobox").nth(2).click();
      await page.getByRole("option", { name: "批量配置请求", exact: true }).first().click();
      await page.locator('[id^="f-"]').first().waitFor({ timeout: 6000 });
      await snap("console");
    });
    await step("monitor", async () => { await page.goto("http://127.0.0.1:1422/monitor"); await snap("monitor"); });
    await page.close();
  }
  await browser.close();
  console.log(shots.join("\n"));
})();
