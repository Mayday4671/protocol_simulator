"""Open an existing live demo project and its actual recorded packet fields."""
import argparse
import json
from pathlib import Path
import shutil
import time
from playwright.sync_api import sync_playwright

parser = argparse.ArgumentParser()
parser.add_argument("--capture-all", action="store_true")
parser.add_argument("--resume-endpoints", action="store_true")
parser.add_argument("--register-again", action="store_true")
args = parser.parse_args()
root = Path(__file__).resolve().parents[1]
out = root / "test-results" / "live-demo"
report = json.loads((out / "report.json").read_text(encoding="utf-8"))

with sync_playwright() as pw:
    browser = pw.chromium.connect_over_cdp("http://127.0.0.1:9223")
    page = next(p for c in browser.contexts for p in c.pages
                if p.url.startswith("http://tauri.localhost"))
    page_errors = []
    page.on("pageerror", lambda error: page_errors.append(str(error)))

    def ipc(action, **fields):
        return page.evaluate(
            "request => window.__TAURI_INTERNALS__.invoke('studio', {request})",
            {"action": action, **fields},
        )

    snap = ipc("snapshot")
    if snap["projectId"] != report["project"]["id"]:
        ipc("project_open", id=report["project"]["id"])
    if args.resume_endpoints:
        running = {e["id"] for e in ipc("snapshot")["endpoints"] if e["running"]}
        for ep in report["endpoints"]:
            if ep["id"] not in running:
                ipc("endpoint_start", id=ep["id"], name=ep["name"], bind=ep["bind"],
                    protocolId=report["protocol"]["id"], virtualPeer=ep["virtualPeer"])
    if args.register_again:
        local, peer = report["endpoints"]

        def run(flow, variables):
            started = ipc("flow_run", flowId=flow["id"], endpointId=local["id"],
                          remote=peer["bind"], variables=variables)
            deadline = time.monotonic() + 15
            while time.monotonic() < deadline:
                current = next(r for r in ipc("snapshot")["runs"] if r["id"] == started["runId"])
                if current["status"] == "COMPLETED":
                    report["runs"].append(current)
                    return
                if current["status"] in ("FAILED", "CANCELLED", "INTERRUPTED"):
                    raise RuntimeError(str(current))
                page.wait_for_timeout(100)
            raise RuntimeError("Registration/heartbeat did not finish")

        registration = report["flows"][1]
        for device in sorted(ipc("snapshot")["devices"], key=lambda d: d["address"]):
            variables = dict(registration["defaultVariables"], deviceAddress=device["address"],
                             deviceNumericId=int(device["address"].replace(" ", ""), 16))
            run(registration, variables)
        snap = ipc("snapshot")
        device = min(snap["devices"], key=lambda d: d["address"])
        session = next(s for s in snap["sessions"]
                       if s["deviceId"] == device["id"] and s["active"] and s["registered"])
        heartbeat = report["flows"][2]
        heartbeat["defaultVariables"]["sessionId"] = session["wireSessionId"]
        ipc("flow_save", flow=heartbeat)
        report["flows"][2] = ipc("flow_publish", flowId=heartbeat["id"])
        run(heartbeat, heartbeat["defaultVariables"])
        report["devices"] = ipc("snapshot")["devices"]
        assert all(d["status"] == "ONLINE" for d in report["devices"])
    page.reload()
    page.wait_for_selector(".connection .connected")

    def navigate(name):
        page.locator("button.nav-item").filter(has_text=name).click()
        page.wait_for_timeout(300)

    if args.capture_all:
        for i, name in enumerate(("工作台", "协议设计", "设备管理", "交互流程",
                                  "报文监控", "通话资源", "模板库", "系统设置"), 1):
            navigate(name)
            assert page.locator(".page-heading p").count() == 0
            if name == "协议设计":
                page.locator(".protocol-toolbar .ant-select").click()
                page.locator(".ant-select-item-option-content").filter(
                    has_text=report["protocol"]["name"] + " · 1").click()
                seed_close = page.locator('.protocol-tabs [data-node-key="seed"] .ant-tabs-tab-remove')
                if seed_close.count():
                    seed_close.click()
            if name == "交互流程":
                page.locator(".page-heading .ant-select").click()
                page.locator(".ant-select-item-option-content").filter(
                    has_text=report["flows"][1]["nameZh"]).click()
                page.locator(".react-flow__node").first.wait_for()
                page.locator(".ant-select").filter(has_text="运行端点").click()
                page.locator(".ant-select-item-option-content").filter(
                    has_text=report["endpoints"][0]["name"]).click()
                page.get_by_placeholder("远端 IP:端口", exact=True).fill(report["endpoints"][1]["bind"])
            page.screenshot(path=str(out / f"page-{i}.png"))
    navigate("报文监控")
    collapse = page.locator("button").filter(has_text="收起报文构造")
    if collapse.count():
        collapse.click()
    page.wait_for_selector("button.packet-row")
    page.locator(".virtual-scroll").evaluate("el => { el.scrollTop = el.scrollHeight; }")
    page.locator("button.packet-row").filter(has=page.locator("code", has_text="21")).first.click()
    flag = page.locator('.packet-inspector tr[data-row-key="body.configCommon8.configFlags"]')
    flag.click()
    assert page.locator(".selected-field").get_by_text("配置控制标志", exact=True).is_visible()
    page.screenshot(path=str(out / "configuration-request.png"))
    page.locator("button.packet-row").filter(has=page.locator("code", has_text="22")).first.click()
    result = page.locator('.packet-inspector tr[data-row-key="body.results[0].result"]')
    result.click()
    assert "AA" in result.inner_text() and "170" in result.inner_text()
    assert "配置成功" in result.inner_text()
    assert page.locator(".hex-grid button.highlighted").count() == 1
    page.screenshot(path=str(out / "configuration-result.png"))
    records = ipc("packet_query", limit=200)
    report["packetCount"] = records["total"]
    assert not page_errors, page_errors
    report["pageErrors"] = page_errors
    report["passed"] = True
    report.pop("error", None)
    report["viewChecks"] = ["配置控制标志与Bit选中", "AA / 170 / 配置成功", "字段和HEX双向定位"]
    if args.capture_all:
        report["copyChecks"] = "八页均无标题副文案"
    for fmt in ("json", "csv"):
        exported = ipc("export", kind="packets", format=fmt)
        shutil.copy2(exported["path"], out / f"MUSP-v0.8-packets.{fmt}")
    exported = ipc("export", kind="project")
    shutil.copy2(exported["path"], out / "MUSP-v0.8-project.zip")
    (out / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps({"passed": True, "packets": records["total"], "project": report["project"]["name"]}, ensure_ascii=False))
