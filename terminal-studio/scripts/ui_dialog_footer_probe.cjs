const { chromium } = require("playwright");
const assert = require("node:assert/strict");
/** 协议配置弹窗在矮视口下保存按钮必须完整可见可点（工作区模式高度=面板-32）。 */
(async () => {
  const browser = await chromium.launch({ channel: "chrome", headless: true });
  for (const viewport of [{ width: 1366, height: 690 }, { width: 1440, height: 810 }]) {
    const page = await browser.newPage({ viewport });
    await page.addInitScript(() => {
      const f = (name, value, when) => ({ name, len: 1, enc: "hex", value, role: "none",
        ...(when ? { when: { field: "业务模式", value: when } } : {}) });
      const fields = [f("业务模式", "01")];
      for (const v of ["01", "02", "03"]) for (let i = 1; i <= 6; i++) fields.push(f(`分支${v}字段${i}`, "AA", v));
      fields.push(f("尾标", "EE"));
      const protocol = { id: 1, protocol_id: "wide", name: "宽协议", def_mode: "fields", format: "hex",
        msg_type: "Request", fields_json: JSON.stringify(fields), payload_template: "",
        system_ref: 1, group_ref: 1, system_name: "测试系统", group_name: "配置", respond_delay_ms: 0 };
      window.__TAURI_INTERNALS__ = { transformCallback: () => 1, unregisterCallback: () => {},
        invoke: async (cmd) => {
          if (cmd === "list_protocols") return [protocol];
          if (cmd === "list_dict_sets") return [];
          if (cmd === "list_systems") return [{ id: 1, name: "测试系统" }];
          if (cmd === "list_groups") return [{ id: 1, system_ref: 1, name: "配置", header_enabled: false, header_fields_json: "[]" }];
          if (cmd === "list_nodes") return [];
          if (cmd === "engine_status") return { listening_nodes: [], heartbeats: [], periodic_sends: [] };
          if (cmd === "log_stats") return { total: 0, in_count: 0, out_count: 0, error_count: 0 };
          return [];
        } };
      window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: () => {} };
    });
    const errors = [];
    page.on("pageerror", (e) => errors.push(String(e)));
    await page.goto("http://127.0.0.1:1422/protocols");
    await page.getByTitle("编辑", { exact: true }).click();
    await page.locator('[data-branch-group="业务模式"]').first().waitFor();
    await page.waitForTimeout(450); // 弹窗开合动画结束后量几何
    const geo = await page.evaluate(() => {
      const dialog = document.querySelector('[role="dialog"]');
      const save = [...dialog.querySelectorAll("button")].find((b) => b.textContent.trim() === "保存");
      const r = save.getBoundingClientRect();
      const hit = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
      const fieldsScroll = document.querySelector('[role="dialog"] .min-h-0.overflow-auto');
      return {
        top: r.top, bottom: r.bottom, vh: innerHeight, hit: !!hit && hit.closest("button") === save,
        fieldsScroll: fieldsScroll ? Math.round(fieldsScroll.getBoundingClientRect().height) : 0,
      };
    });
    assert.ok(geo.bottom <= geo.vh && geo.top >= 0, `${viewport.height} 高视口保存按钮被裁切: ${JSON.stringify(geo)}`);
    assert.ok(geo.hit, `${viewport.height} 高视口保存按钮点击被遮挡`);
    assert.ok(geo.fieldsScroll >= 200, `${viewport.height} 高视口字段表区被压扁: ${geo.fieldsScroll}px`);
    assert.deepEqual(errors, []);
    await page.close();
    console.log(`PASS ${viewport.width}x${viewport.height} 保存按钮完整可见可点`);
  }
  await browser.close();
})().catch((e) => { console.error(e); process.exitCode = 1; });
