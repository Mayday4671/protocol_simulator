"""Exercise the actual Tauri WebView2 via its opt-in --inspect-local CDP port.

Requires playwright in a local venv. No browser mocks, test web server, or fake IPC.
The app must already be running. A separate project and loopback sockets are used.
"""
import argparse
import json
from pathlib import Path
import socket
import time
from playwright.sync_api import sync_playwright

parser = argparse.ArgumentParser()
parser.add_argument("--cdp", default="http://127.0.0.1:9223")
parser.add_argument("--output", default="test-results/desktop-e2e")
args = parser.parse_args()
output = Path(args.output).resolve()
output.mkdir(parents=True, exist_ok=True)
report = {"passed": False, "checks": [], "pageErrors": [], "screenshots": []}

with sync_playwright() as pw:
    browser = pw.chromium.connect_over_cdp(args.cdp)
    page = next(p for context in browser.contexts for p in context.pages if p.url != "about:blank")
    page.on("pageerror", lambda error: report["pageErrors"].append(str(error)))
    report["url"] = page.url

    def ipc(action, **fields):
        return page.evaluate("request => window.__TAURI_INTERNALS__.invoke('studio', {request})", {"action": action, **fields})

    def check(name):
        report["checks"].append(name)
        print("PASS", name, flush=True)

    def wait_run(run_id):
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            snapshot = ipc("snapshot")
            run = next((r for r in snapshot["runs"] if r["id"] == run_id), None)
            if run and run["status"] == "COMPLETED":
                return snapshot
            if run and run["status"] in ("FAILED", "CANCELLED", "INTERRUPTED"):
                raise AssertionError(run)
            page.wait_for_timeout(75)
        raise AssertionError("Flow timeout: " + run_id)

    def navigate(name):
        page.locator("button.nav-item").filter(has_text=name).click()
        page.wait_for_timeout(600)
        assert not page.get_by_text("本地服务尚未连接", exact=True).count()

    endpoints = []
    original_project = ipc("snapshot")["projectId"]
    try:
        project = ipc("project_create", name="桌面安装验收 " + time.strftime("%Y%m%d-%H%M%S"))
        report["projectId"] = project["id"]
        page.reload()
        page.wait_for_selector(".connection .connected")
        snap = ipc("snapshot")
        protocol = next(p["id"] for p in snap["protocols"] if p["published"])
        system = snap["systems"][0]["id"]
        ipc("settings_save", heartbeatTimeoutMs=0)
        local = ipc("endpoint_start", name="桌面验收设备端", bind="127.0.0.1:0", protocolId=protocol)
        peer = ipc("endpoint_start", name="桌面验收控制端", bind="127.0.0.1:0", protocolId=protocol, virtualPeer=True)
        endpoints = [local["id"], peer["id"]]
        check("真实Tauri IPC、项目隔离和两个Rust UDP socket")

        flow = ipc("template_install", id="config-devices-5", protocolId=protocol)
        ipc("flow_publish", flowId=flow["id"])
        variables = dict(flow["defaultVariables"], systemId=system)
        run = ipc("flow_run", flowId=flow["id"], endpointId=local["id"], remote=peer["bind"], variables=variables)
        snap = wait_run(run["runId"])
        assert len(snap["devices"]) == 5, snap["devices"]
        assert all(d["status"] == "OFFLINE" for d in snap["devices"])
        check("0x21真实发送与0x22逐项AA确认生成5台离线设备")

        registration = ipc("template_install", id="register-no-auth", protocolId=protocol)
        ipc("flow_publish", flowId=registration["id"])
        for device in snap["devices"]:
            variables = dict(registration["defaultVariables"], systemId=system, deviceAddress=device["address"], deviceNumericId=int(device["address"].replace(" ", ""), 16))
            run = ipc("flow_run", flowId=registration["id"], endpointId=local["id"], remote=peer["bind"], variables=variables)
            snap = wait_run(run["runId"])
        deadline=time.monotonic()+5
        while not all(d["status"] == "ONLINE" for d in snap["devices"]):
            assert time.monotonic()<deadline,snap["devices"]
            page.wait_for_timeout(50)
            snap=ipc("snapshot")
        check("5台设备实际注册握手后ONLINE，状态来自后端")

        # A malformed real datagram must remain inspectable, with no invented code.
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
            host, port = local["bind"].rsplit(":", 1)
            sock.sendto(b"\xDE\xAD\xBE\xEF", (host, int(port)))
        page.wait_for_timeout(250)
        logs = ipc("packet_query", limit=200)
        assert any(p["msgCode"] is None and p["status"] == "解析失败" for p in logs["items"])
        rx = ipc("packet_query", direction="RX", limit=200)
        tx = ipc("packet_query", direction="TX", limit=200)
        assert rx["total"] + tx["total"] == logs["total"]
        assert all(p["direction"] == "RX" for p in rx["items"])
        report["packetCount"] = logs["total"]
        check("损坏UDP保留原始数据、RX/TX数据库过滤正确")

        navigate("报文监控")
        page.locator("button").filter(has_text="收起报文构造").click()
        page.wait_for_selector("button.packet-row")
        page.locator("button.packet-row").first.click()
        assert "DE AD BE EF".replace(" ", "") == "".join(page.locator(".hex-grid button").all_text_contents())
        assert page.locator(".packet-inspector").get_by_text("解析失败", exact=False).count() or page.get_by_text("解析失败", exact=True).count()
        page.locator("button.packet-row").filter(has=page.locator("code", has_text="04")).first.click()
        page.locator("#hex-byte-0").click()
        assert page.locator(".selected-field").is_visible()
        assert page.locator(".hex-grid button.highlighted").count() > 0
        page.locator(".packet-inspector .ant-table-tbody tr[data-row-key]").nth(1).click()
        assert page.locator(".selected-row").count() > 0
        check("损坏报文UI不崩溃，HEX与解析字段双向选择")

        navigate("协议设计")
        page.locator(".protocol-toolbar button").filter(has_text="校验").first.click()
        page.get_by_text("校验通过，可发布不可变版本", exact=True).wait_for()
        check("应用内Monaco XML通过Rust语法与DSL校验")

        def window_state(command):
            return page.evaluate("command => window.__TAURI_INTERNALS__.invoke('plugin:window|' + command, {label:'main'})", command)
        assert window_state("is_decorated") is False
        initial_maximized=window_state("is_maximized")
        page.locator(".window-controls button").nth(1).click()
        page.wait_for_timeout(250)
        assert window_state("is_maximized") != initial_maximized
        page.locator(".window-controls button").nth(1).click()
        page.wait_for_timeout(250)
        assert window_state("is_maximized") == initial_maximized
        page.get_by_role("button",name="最小化窗口",exact=True).click()
        page.wait_for_timeout(250)
        assert window_state("is_minimized") is True
        window_state("unminimize")
        page.wait_for_timeout(250)
        check("统一标题栏、真实最小化与最大化/还原")

        navigate("交互流程")
        page.locator(".page-heading .ant-select").click()
        page.locator(".ant-select-item-option-content").filter(has_text=flow["nameZh"]).click()
        page.locator(".react-flow__node").first.wait_for()
        assert page.locator(".react-flow__node").count() == len(flow["nodes"])
        page.locator(".ant-select").filter(has_text="运行端点").click()
        page.locator(".ant-select-item-option-content").filter(has_text=local["name"]).click()
        page.get_by_placeholder("远端 IP:端口",exact=True).fill(peer["bind"])
        page.locator(".flow-main input.ant-input").first.fill("桌面图形配置回归")
        page.locator(".flow-toolbar button").filter(has_text="保存草稿").click()
        page.get_by_text("草稿 · 需发布后运行",exact=True).wait_for()
        assert page.locator("button").filter(has_text="启动流程").is_disabled()
        page.locator(".flow-toolbar button").filter(has_text="校验并发布").click()
        page.locator(".flow-toolbar .ant-tag").filter(has_text="已发布").wait_for()
        before_runs={r["id"] for r in ipc("snapshot")["runs"]}
        page.locator("button").filter(has_text="启动流程").click()
        deadline=time.monotonic()+10
        while True:
            snapshot=ipc("snapshot")
            created=[r for r in snapshot["runs"] if r["id"] not in before_runs]
            if created:
                completed=wait_run(created[0]["id"])
                assert all(d["status"]=="ONLINE" for d in completed["devices"])
                break
            assert time.monotonic()<deadline,"UI run did not create an actual flow"
            page.wait_for_timeout(75)
        logs=ipc("packet_query",limit=200)
        report["packetCount"]=logs["total"]
        check("可视化流程保存/发布/真实运行，草稿禁止启动，配置不重置在线设备")

        for index, name in enumerate(["工作台", "协议设计", "设备管理", "交互流程", "报文监控", "通话资源", "模板库"], 1):
            navigate(name)
            assert page.locator(".page-heading h1").count() == 1
            if name == "设备管理":
                assert page.locator(".ant-table-tbody tr[data-row-key]").count() == 5
                page.locator(".ant-table-tbody tr[data-row-key]").first.click()
            if name == "交互流程":
                page.locator(".page-heading .ant-select").click()
                page.locator(".ant-select-item-option-content").filter(has_text="桌面图形配置回归").click()
                page.locator(".react-flow__node").first.wait_for()
            path = output / f"page-{index}.png"
            page.screenshot(path=str(path))
            report["screenshots"].append(str(path))
        check("七个功能页面连接真实桌面服务并正常显示")

        was_dark = page.locator(".studio.dark").count() == 1
        page.get_by_role("button", name="切换主题", exact=True).click()
        assert (page.locator(".studio.dark").count() == 1) != was_dark
        page.reload()
        page.wait_for_selector(".connection .connected")
        assert (page.locator(".studio.dark").count() == 1) != was_dark
        page.screenshot(path=str(output / "light-theme.png"))
        page.get_by_role("button", name="切换主题", exact=True).click()
        check("主题切换即时生效并在重载后保留")

        report["exports"] = []
        for fmt in ["json", "jsonl", "csv"]:
            exported = ipc("export", kind="packets", format=fmt)
            path = Path(exported["path"])
            assert path.is_file() and path.stat().st_size > 0
            assert exported["rows"] == logs["total"]
            if fmt == "json":
                assert len(json.loads(path.read_text(encoding="utf-8"))) == logs["total"]
            report["exports"].append(exported)
        exported = ipc("export", kind="project")
        imported = ipc("project_import", path=exported["path"])
        assert imported["id"] != project["id"]
        check("完整报文JSON/JSONL/CSV与项目ZIP导出导入")
        assert not report["pageErrors"], report["pageErrors"]
        check("WebView2无未处理JavaScript异常")
        report["passed"] = True
    except Exception as error:
        report["error"] = str(error)
        page.screenshot(path=str(output / "failure.png"))
        raise
    finally:
        for endpoint in endpoints:
            try:
                ipc("endpoint_stop", id=endpoint)
            except Exception as error:
                report.setdefault("cleanupErrors", []).append(str(error))
        try:
            ipc("project_open", id=original_project)
        except Exception as error:
            report.setdefault("cleanupErrors", []).append(str(error))
        (output / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
