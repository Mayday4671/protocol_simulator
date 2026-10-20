const { chromium } = require("playwright");
const assert = require("node:assert/strict");
(async () => {
  const browser = await chromium.launch({ channel: "chrome", headless: true });
  const page = await browser.newPage({
    viewport: { width: 1440, height: 1000 },
  });
  const errors = [];
  page.on("pageerror", (e) => errors.push(String(e)));
  await page.addInitScript(
    ({ aliases }) => {
      const f = (name, value, when) => ({
        name,
        len: 1,
        enc: "hex",
        value,
        role: "none",
        ...(when ? { when: { field: "模式", value: when } } : {}),
      });
      const fields = [
        f("模式", "01"),
        f("A字段1", "A1", "01"),
        f("A字段2", "A2", aliases ? "0 1" : "01"),
        f("B字段1", "B1", "02"),
        f("B字段2", "B2", "02"),
        f("尾标", "EE"),
      ];
      const protocol = {
        id: 1,
        protocol_id: "branches",
        name: "分支验收",
        def_mode: "fields",
        format: "hex",
        msg_type: "Request",
        fields_json: JSON.stringify(fields),
        payload_template: "",
        system_ref: 1,
        group_ref: 1,
        system_name: "测试系统",
        group_name: "配置",
        respond_delay_ms: 0,
        auto_periodic_interval_ms: 1000,
      };
      const sets = Array.from({ length: 13 }, (_, i) => ({
        id: i + 1,
        key: "d" + i,
        name: "配置字典" + i,
        byte_len: 1,
        group_ref: 1,
        system_name: "测试系统",
        group_name: "配置",
        items: [
          {
            id: i + 1,
            set_ref: i + 1,
            value: "01",
            label: "条目" + i,
            sort_order: 0,
          },
        ],
      }));
      sets.push({
        id: 50,
        key: "biz",
        name: "业务字典",
        byte_len: 1,
        group_ref: 2,
        system_name: "测试系统",
        group_name: "业务",
        items: [],
      });
      window.calls = [];
      window.__TAURI_INTERNALS__ = {
        transformCallback: () => 1,
        unregisterCallback: () => {},
        invoke: async (cmd, args) => {
          window.calls.push({ cmd, args });
          if (cmd === "list_protocols") return [protocol];
          if (cmd === "list_dict_sets") return structuredClone(sets);
          if (cmd === "list_systems") return [{ id: 1, name: "测试系统" }];
          if (cmd === "list_groups")
            return [
              {
                id: 1,
                system_ref: 1,
                name: "配置",
                header_enabled: true,
                header_fields_json: JSON.stringify([f("公共类型", "01")]),
              },
              { id: 2, system_ref: 1, name: "业务" },
            ];
          if (cmd === "list_nodes")
            return [
              {
                id: 1,
                name: "测试通道",
                role: "simulator",
                local_ip: "127.0.0.1",
                local_port: 19001,
                remote_ip: "127.0.0.1",
                remote_port: 19002,
                node_id_hex: "01",
                group_bind_json: "[1]",
                enabled: true,
              },
            ];
          if (cmd === "engine_status")
            return { listening_nodes: [], heartbeats: [], periodic_sends: [] };
          if (cmd === "log_stats")
            return { total: 0, in_count: 0, out_count: 0, error_count: 0 };
          if (cmd === "upsert_protocol") {
            Object.assign(protocol, args.payload, {
              fields_json: JSON.stringify(args.payload.fields),
            });
            return protocol;
          }
          if (cmd === "delete_dict_set") {
            sets.splice(
              sets.findIndex((s) => s.id === args.id),
              1,
            );
            return null;
          }
          if (cmd === "upsert_dict_set") {
            const set = {
              ...args.payload,
              id: Math.max(...sets.map((s) => s.id)) + 1,
              key: "created",
              items: [],
              system_name: "测试系统",
              group_name: "配置",
            };
            sets.push(set);
            return set;
          }
          return [];
        },
      };
      window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
        unregisterListener: () => {},
      };
    },
    { aliases: process.argv.includes("--aliases") },
  );

  try {
    page.setDefaultTimeout(7000);
    await page.goto("http://127.0.0.1:1422/protocols");
    await page.getByTitle("编辑", { exact: true }).click();
    const section = (value) =>
      page.locator(`[data-branch-group="模式"][data-branch-value="${value}"]`);
    assert.equal(await section("01").count(), 1);
    assert.equal(await section("02").count(), 1);
    await section("01")
      .getByRole("button", { name: "收起分支组：模式" })
      .click();
    for (const value of ["03", "04"]) {
      await page
        .getByRole("button", { name: "按分支添加", exact: true })
        .click();
      const dialog = page.getByRole("dialog").last();
      await dialog.getByRole("combobox").click();
      await page.getByRole("option", { name: "模式", exact: true }).click();
      await dialog.getByPlaceholder("期望值, 如 01 / 设备配置").fill(value);
      await dialog
        .locator("textarea")
        .fill(`分支${value}字段一 1 AA\n分支${value}字段二 2 BB CC`);
      await dialog.getByRole("button", { name: "添加 整个分支" }).click();
      await section(value).waitFor();
      assert.equal(
        await section(value)
          .getByRole("button", { name: "收起分支组：模式" })
          .getAttribute("aria-expanded"),
        "true",
      );
      assert.equal(
        await section(value).locator("input").first().inputValue(),
        `分支${value}字段一`,
      );
    }
    assert.equal(await page.locator('[data-branch-group="模式"]').count(), 4);
    await page.getByRole("button", { name: "保存", exact: true }).click();
    await page.waitForFunction(() =>
      window.calls.some((c) => c.cmd === "upsert_protocol"),
    );
    const saved = await page.evaluate(
      () =>
        window.calls.filter((c) => c.cmd === "upsert_protocol").at(-1).args
          .payload,
    );
    for (const value of ["01", "02", "03", "04"])
      assert.equal(
        saved.fields.filter((f) => f.when?.value === value).length,
        2,
      );
    await page.getByTitle("编辑", { exact: true }).click();
    for (const value of ["01", "02", "03", "04"])
      await section(value).waitFor();
    assert.equal(
      await page.locator('select:not([aria-hidden="true"])').count(),
      0,
      "all visible dropdowns use shared component",
    );
    await page.screenshot({ path: "artifacts/ux-branches-fixed.png" });
    assert.deepEqual(errors, []);
    console.log(
      "PASS 4 values of same branch field visible together; add after collapse; save payload and reopen retain all 8 fields; shared dropdowns",
    );
  } finally {
    await browser.close();
  }
})().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});
