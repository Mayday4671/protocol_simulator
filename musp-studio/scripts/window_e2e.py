"""Native titlebar operations against a running MUSP test WebView2."""
import json
from pathlib import Path
from playwright.sync_api import sync_playwright

with sync_playwright() as pw:
    browser=pw.chromium.connect_over_cdp("http://127.0.0.1:9223")
    page=browser.contexts[0].pages[0]
    def state(command):
        return page.evaluate("command=>window.__TAURI_INTERNALS__.invoke('plugin:window|'+command,{label:'main'})",command)
    initial=state("is_maximized")
    page.locator(".title-spacer").dblclick()
    page.wait_for_timeout(350)
    after=state("is_maximized")
    assert initial!=after, "Double clicking titlebar did not change native window state"
    page.locator(".window-controls button").nth(1).click()
    page.wait_for_timeout(200)
    assert state("is_maximized")==initial
    page.locator(".titlebar .ant-select").first.click()
    page.locator(".ant-select-dropdown:visible").first.wait_for()
    page.keyboard.press("Escape")
    assert state("is_maximized")==initial
    result={"passed":True,"checks":["native-double-click-maximize","restore","project-selector-is-not-drag-region"]}
    Path("test-results/window-e2e.json").write_text(json.dumps(result,indent=2),encoding="utf-8")
    print(result)
