"""Verify the custom close button actually exits and releases its UDP socket."""
import json
import socket
import time
from pathlib import Path
from playwright.sync_api import sync_playwright,Error

with sync_playwright() as pw:
    browser=pw.chromium.connect_over_cdp("http://127.0.0.1:9223")
    page=browser.contexts[0].pages[0]
    def ipc(action,**fields):
        return page.evaluate("request=>window.__TAURI_INTERNALS__.invoke('studio',{request})",{"action":action,**fields})
    ipc("project_create",name="关闭窗口与socket释放验收")
    snapshot=ipc("snapshot")
    protocol=next(p["id"] for p in snapshot["protocols"] if p["published"])
    endpoint=ipc("endpoint_start",name="关窗端口释放测试",bind="127.0.0.1:0",protocolId=protocol)
    try:
        page.get_by_role("button",name="关闭窗口",exact=True).click()
    except Error as error:
        if "closed" not in str(error).lower():
            raise
    host,port=endpoint["bind"].rsplit(":",1)
    deadline=time.monotonic()+5
    while True:
        with socket.socket(socket.AF_INET,socket.SOCK_DGRAM) as check:
            check.setsockopt(socket.SOL_SOCKET,socket.SO_EXCLUSIVEADDRUSE,1)
            try:
                check.bind((host,int(port)))
                break
            except OSError:
                assert time.monotonic()<deadline,"Window close did not release UDP socket"
                time.sleep(.05)
    result={"passed":True,"closedSocketAddress":endpoint["bind"],"checks":["custom-close-button","socket-port-rebind-after-close"]}
    Path("test-results/close-window-e2e.json").write_text(json.dumps(result,indent=2),encoding="utf-8")
    print(result)
