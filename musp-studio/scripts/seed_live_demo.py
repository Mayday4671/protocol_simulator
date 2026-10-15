"""Populate the installed MUSP application through its real Tauri IPC service.

Launch the installed app with --inspect-local first. This creates a new project,
runs finite loopback UDP exchanges, and leaves their records available to view.
"""
import json
from pathlib import Path
import shutil
import time
import xml.etree.ElementTree as ET

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "test-results" / "live-demo"
OUT.mkdir(parents=True, exist_ok=True)
report = {"passed": False, "runs": []}

with sync_playwright() as pw:
    browser = pw.chromium.connect_over_cdp("http://127.0.0.1:9223")
    page = next(p for c in browser.contexts for p in c.pages
                if p.url.startswith("http://tauri.localhost"))

    def ipc(action, **fields):
        return page.evaluate(
            "request => window.__TAURI_INTERNALS__.invoke('studio', {request})",
            {"action": action, **fields},
        )

    def wait_until(predicate, seconds=15):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            snap = ipc("snapshot")
            if predicate(snap):
                return snap
            page.wait_for_timeout(100)
        raise RuntimeError("Timed out waiting for actual application state")

    def run_flow(flow, local, peer, **variables):
        run = ipc("flow_run", flowId=flow["id"], endpointId=local["id"],
                  remote=peer["bind"], variables=variables)
        run_id = run["runId"]

        def finished(snap):
            found = next((r for r in snap["runs"] if r["id"] == run_id), None)
            if found and found["status"] in ("FAILED", "CANCELLED", "INTERRUPTED"):
                raise RuntimeError(json.dumps(found, ensure_ascii=False))
            return found and found["status"] == "COMPLETED"

        snap = wait_until(finished)
        report["runs"].append(next(r for r in snap["runs"] if r["id"] == run_id))
        print("Completed:", flow["nameZh"], flush=True)
        return snap

    def install(template, name, protocol_id, variables):
        flow = ipc("template_install", id=template, protocolId=protocol_id)
        flow["nameZh"] = name
        flow["defaultVariables"].update(variables)
        labels = {
            "start": "开始", "end": "结束", "error": "失败",
            "configure": "发送配置 0x21，等待 0x22",
            "check-results": "检查五条配置结果",
            "register": "注册请求 0x01 → 许可 0x02",
            "check-license": "检查许可与认证模式", "save-session": "保存会话号",
            "confirm": "注册确认 0x03 → 完成 0x04",
            "check-complete": "检查注册结果", "repeat": "循环三次",
            "heartbeat": "心跳 0x31 → 应答 0x32", "check": "检查应答",
            "interval": "等待 1 秒",
        }
        for node in flow["nodes"]:
            node["data"]["label"] = labels.get(node["id"], node["id"])
        flow = ipc("flow_save", flow=flow)
        return ipc("flow_publish", flowId=flow["id"])

    try:
        initial = ipc("snapshot")
        if any(e["running"] for e in initial["endpoints"]):
            raise RuntimeError("Existing active endpoints: leave current work intact")
        project = ipc("project_create", name="MUSP v0.8 信令交互演示")
        report["project"] = project
        page.reload()
        page.wait_for_selector(".connection .connected")
        snap = ipc("snapshot")
        system = snap["systems"][0]["id"]
        source = ROOT / "protocols" / "musp-v08.xml"
        xml = source.read_text(encoding="utf-8")
        validation = ipc("protocol_validate", xml=xml)
        assert validation["valid"], validation
        # Use the normal import, draft, and immutable publication operations.
        draft = ipc("protocol_import", path=str(source), name="MUSP v0.8 信令联调")
        protocol = ipc("protocol_publish", id=draft["id"], xml=xml,
                       name="MUSP v0.8 信令联调")
        report["protocol"] = protocol
        report["messageCount"] = len(ET.fromstring(xml).findall("./messages/message"))
        # Finite demonstration; normal offline transitions still apply afterward.
        ipc("settings_save", heartbeatTimeoutMs=300000)
        local = ipc("endpoint_start", name="本机联调端", bind="127.0.0.1:0",
                    protocolId=protocol["id"])
        peer = ipc("endpoint_start", name="虚拟对端（本机 UDP）", bind="127.0.0.1:0",
                   protocolId=protocol["id"], virtualPeer=True)
        report["endpoints"] = [local, peer]

        configuration = install("config-devices-5", "01 · 五台设备配置 21→22",
                                protocol["id"], {"systemId": system})
        run_flow(configuration, local, peer)
        snap = wait_until(lambda s: len(s["devices"]) == 5)
        assert all(d["status"] == "OFFLINE" for d in snap["devices"])
        registration = install("register-no-auth", "02 · 注册握手 01→02→03→04",
                               protocol["id"], {"systemId": system})
        for device in sorted(snap["devices"], key=lambda d: d["address"]):
            run_flow(registration, local, peer, systemId=system,
                     deviceAddress=device["address"],
                     deviceNumericId=int(device["address"].replace(" ", ""), 16))
        snap = wait_until(lambda s: all(d["status"] == "ONLINE" for d in s["devices"]))
        device = min(snap["devices"], key=lambda d: d["address"])
        session = next(s for s in snap["sessions"]
                       if s["deviceId"] == device["id"] and s["active"] and s["registered"])
        heartbeat = install("heartbeat", "03 · 三轮心跳 31→32", protocol["id"],
                            {"systemId": system, "deviceAddress": device["address"],
                             "sessionId": session["wireSessionId"]})
        run_flow(heartbeat, local, peer)
        report["flows"] = [configuration, registration, heartbeat]
        wait_until(lambda s: s.get("processedPackets", 0) >= 56)
        packets = ipc("packet_query", limit=200)
        assert packets["total"] == 56, packets["total"]
        assert all(p["status"] not in ("解析失败", "处理失败") for p in packets["items"])
        report["packetCount"] = packets["total"]
        report["devices"] = ipc("snapshot")["devices"]
        for fmt in ("json", "csv"):
            exported = ipc("export", kind="packets", format=fmt)
            shutil.copy2(exported["path"], OUT / f"MUSP-v0.8-packets.{fmt}")
        exported = ipc("export", kind="project")
        shutil.copy2(exported["path"], OUT / "MUSP-v0.8-project.zip")
        shutil.copy2(source, OUT / "MUSP-v0.8.xml")
        page.reload()
        page.wait_for_selector(".connection .connected")
        page.locator("button.nav-item").filter(has_text="报文监控").click()
        page.locator("button").filter(has_text="收起报文构造").click()
        page.wait_for_selector("button.packet-row")
        # Bring the earliest configuration exchange into the virtualized list.
        scroller = page.locator(".virtual-scroll")
        scroller.evaluate("el => { el.scrollTop = el.scrollHeight; }")
        rows = page.locator("button.packet-row").filter(has=page.locator("code", has_text="22"))
        rows.first.wait_for()
        rows.first.click()
        page.locator('.packet-inspector tr[data-row-key="body.results[0].result"]').click()
        page.screenshot(path=str(OUT / "configuration-result.png"))
        report["passed"] = True
        print(json.dumps({"project": project["name"], "messages": report["messageCount"],
                          "packets": packets["total"], "endpoints": [local["bind"], peer["bind"]]},
                         ensure_ascii=False), flush=True)
    except Exception as error:
        report["error"] = str(error)
        page.screenshot(path=str(OUT / "failure.png"))
        raise
    finally:
        (OUT / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2),
                                        encoding="utf-8")
