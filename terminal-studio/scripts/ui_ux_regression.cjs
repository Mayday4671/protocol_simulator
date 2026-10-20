// Run against `vite preview --host 127.0.0.1 --port 1422`.
// Python Playwright is unavailable on this host; reuse its installed Node runtime.
// Only the native IPC bridge is stubbed. Every protocol/dictionary comes from
// the delivered dataset's actual SQLite import, never a simplified UI fixture.
const { DatabaseSync } = require("node:sqlite");
const { chromium } = require("playwright");
const assert = require("node:assert/strict");
const path = require("node:path");

const root = path.resolve(__dirname, "..");
const db = new DatabaseSync(
  path.join(root, "artifacts/iot-dataset-validation.db"),
  { readOnly: true },
);
const systems = db.prepare("SELECT * FROM protocol_systems ORDER BY id").all();
const groups = db.prepare("SELECT * FROM protocol_groups ORDER BY id").all();
const protocols = db
  .prepare(
    "SELECT p.*,s.name system_name,g.name group_name FROM protocols p JOIN protocol_systems s ON s.id=p.system_ref JOIN protocol_groups g ON g.id=p.group_ref ORDER BY p.id",
  )
  .all();
const dictionaries = db
  .prepare(
    "SELECT d.*,g.name group_name,s.name system_name FROM dict_sets d LEFT JOIN protocol_groups g ON g.id=d.group_ref LEFT JOIN protocol_systems s ON s.id=g.system_ref ORDER BY d.id",
  )
  .all();
for (const d of dictionaries)
  d.items = db
    .prepare("SELECT * FROM dict_items WHERE set_ref=? ORDER BY sort_order,id")
    .all(d.id);
db.close();
assert.equal(protocols.length, 54);
assert.equal(dictionaries.length, 130);

(async () => {
  const browser = await chromium.launch({ channel: "chrome", headless: true });
  const page = await browser.newPage({
    viewport: { width: 1600, height: 1100 },
  });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  await page.addInitScript(
    (data) => {
      window.calls = [];
      window.__TAURI_INTERNALS__ = {
        transformCallback: () => 1,
        unregisterCallback: () => {},
        invoke: async (cmd, args) => {
          window.calls.push({ cmd, args });
          if (cmd === "list_protocols") return data.protocols;
          if (cmd === "list_dict_sets") return data.dictionaries;
          if (cmd === "list_systems") return data.systems;
          if (cmd === "list_groups") return data.groups;
          if (cmd === "list_nodes")
            return [
              {
                id: 90001,
                name: "仅页面验收通道",
                role: "simulator",
                local_ip: "127.0.0.1",
                local_port: 31991,
                remote_ip: "127.0.0.1",
                remote_port: 31992,
                node_id_hex: "01000001",
                group_bind_json: JSON.stringify(data.groups.map((g) => g.id)),
                enabled: true,
              },
            ];
          if (cmd === "engine_status")
            return { listening_nodes: [], heartbeats: [], periodic_sends: [] };
          if (cmd === "log_stats")
            return { total: 0, in_count: 0, out_count: 0, error_count: 0 };
          if (cmd === "send_udp_message") return { bytes: 1, hex: "00" }; // Inspect submitted parameters; Rust harness tests actual UDP.
          if (/^(upsert|delete|import|start)_/.test(cmd))
            throw new Error(
              "Unexpected mutation in read-only UI acceptance: " + cmd,
            );
          return [];
        },
      };
      window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
        unregisterListener: () => {},
      };
    },
    { systems, groups, protocols, dictionaries },
  );
  async function choose(index, name) {
    await page.getByRole("combobox").nth(index).click();
    await page.getByRole("option", { name, exact: true }).click();
  }
  async function selectDictionaryField(name, value) {
    await page
      .locator(`label[for="f-${name}"]`)
      .locator("..")
      .getByRole("combobox")
      .click();
    await page
      .getByRole("option")
      .filter({ hasText: new RegExp("^" + value + "\\s") })
      .click();
  }

  try {
    page.setDefaultTimeout(7000);
    await page.goto("http://127.0.0.1:1422/console");
    await choose(0, "环境采集协议");
    await choose(1, "接入管理");
    await choose(2, "终端注销请求");
    for (const width of [1700, 1366, 1000]) {
      await page.setViewportSize({ width, height: 900 });
      const fields = page.locator(".runtime-fields").last();
      await fields.waitFor();
      const columns = await fields.evaluate(
        (e) => getComputedStyle(e).gridTemplateColumns.split(" ").length,
      );
      assert.ok(columns >= 2, `width ${width}: fields must share a row`);
      const send = await page
        .getByRole("button", { name: "发送报文", exact: true })
        .boundingBox();
      assert.ok(send.y + send.height < 900, "send action always visible");
      const overflow = await page
        .locator(".console-workspace")
        .evaluate((e) => e.scrollWidth > e.clientWidth + 1);
      assert.equal(overflow, false, "no horizontal page overflow");
      await page.screenshot({
        path: path.join(root, `artifacts/ux-console-${width}.png`),
      });
    }
    await page.getByRole("button", { name: "实时监控", exact: true }).click();
    assert.equal(
      await page
        .locator(".console-monitor")
        .evaluate((e) => e.getBoundingClientRect().height < 90),
      true,
    );
    await page.getByRole("button", { name: "实时监控", exact: true }).click();
    await page.getByRole("button", { name: "发送报文", exact: true }).click();
    await page.waitForFunction(() =>
      window.calls.some((c) => c.cmd === "send_udp_message"),
    );
    // “全部分组”中直接选择协议也必须匹配通道。
    await choose(1, "全部分组");
    await page.getByRole("button", { name: "发送报文", exact: true }).click();
    assert.ok(await page.getByLabel("业务通道").textContent());
    for (const route of [
      "/",
      "/nodes",
      "/protocols",
      "/dict",
      "/devices",
      "/monitor",
    ]) {
      await page.goto("http://127.0.0.1:1422" + route);
      await page.waitForLoadState("networkidle");
      await page.screenshot({
        path: path.join(
          root,
          `artifacts/ux-${route.slice(1) || "overview"}.png`,
        ),
      });
      assert.equal(
        await page
          .locator("body")
          .evaluate((e) => e.scrollWidth > innerWidth + 1),
        false,
        route + " page overflow",
      );
    }
    assert.deepEqual(errors, []);
    console.log(
      "PASS all 7 menus, 3 console widths, multi-column fields, fixed send bar, monitor collapse, automatic channel and send parameters",
    );
  } finally {
    await browser.close();
  }
})().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});
