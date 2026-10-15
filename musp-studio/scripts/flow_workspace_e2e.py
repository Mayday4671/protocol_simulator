"""Real packaged Tauri UI + external Python UDP peer; finite interaction checks."""
import argparse
import json
from pathlib import Path
import socket
import threading
import time

from playwright.sync_api import sync_playwright, expect

parser = argparse.ArgumentParser()
parser.add_argument('--cdp', default='http://127.0.0.1:9223')
parser.add_argument('--output', default='test-results/flow-workspace')
args = parser.parse_args()
output = Path(args.output).resolve()
output.mkdir(parents=True, exist_ok=True)
report = {'passed': False, 'checks': [], 'pageErrors': []}

XML = '''<protocol id="generic-ui-check" nameZh="通用 UDP 联调验收" version="1" byteOrder="big">
<fragments><fragment id="payload">
<field key="value" labelZh="输出值" bytes="1"/>
<field key="flags" labelZh="控制标志" bytes="1" valueKind="bitset"><bits><bit key="enabled" start="7" width="1" labelZh="启用"/><bit key="final" start="0" width="1" labelZh="结束"/></bits></field>
<field key="count" labelZh="数据项数" bytes="1"/>
<array key="items" labelZh="数据项" countRef="count" elementBytes="1" maxItems="8"><field key="value" labelZh="数据值" bytes="1"/></array>
</fragment></fragments><messages>
<message id="SET" code="0x21" nameZh="设置输出" responseCodes="0x22"><header><field key="code" labelZh="信令码" bytes="1" binding="message.code"/></header><body><use ref="payload"/></body></message>
<message id="RESULT" code="0x22" nameZh="输出确认"><header><field key="code" labelZh="信令码" bytes="1" binding="message.code"/></header><body><use ref="payload"/></body></message>
<message id="EVENT" code="0x73" nameZh="对端上报"><header><field key="code" labelZh="信令码" bytes="1" binding="message.code"/></header><body><use ref="payload"/></body></message>
</messages></protocol>'''
VALUES = {'body': {'payload': {'value': 42, 'flags': 129, 'count': 2, 'items': [{'value': 7}, {'value': 9}]}}}

with sync_playwright() as pw:
    browser = pw.chromium.connect_over_cdp(args.cdp)
    page = next(page for context in browser.contexts for page in context.pages if page.url != 'about:blank')
    page.on('pageerror', lambda error: report['pageErrors'].append(str(error)))

    def ipc(action, **fields):
        return page.evaluate("request => window.__TAURI_INTERNALS__.invoke('studio', {request})", {'action': action, **fields})

    def check(name):
        report['checks'].append(name)
        print('PASS', name, flush=True)

    def select(label, text):
        control = page.get_by_label(label, exact=True).first
        control.click()
        option = page.locator('.ant-select-dropdown:visible .ant-select-item-option').filter(has_text=text)
        option.first.click()

    def await_run(previous_ids, status='COMPLETED'):
        deadline = time.monotonic() + 12
        while time.monotonic() < deadline:
            snap = ipc('snapshot')
            runs = [run for run in snap['runs'] if run['id'] not in previous_ids]
            if runs:
                run = runs[0]
                if run['status'] == status:
                    return run
                if run['status'] in ('FAILED', 'CANCELLED', 'INTERRUPTED') and status != run['status']:
                    raise AssertionError(run)
            page.wait_for_timeout(100)
        raise AssertionError(f'No new run with status {status}')

    def install_flow(flow_id, name, nodes, edges):
        flow = {'id': flow_id, 'nameZh': name, 'protocol': {'id': protocol}, 'version': '1', 'defaultVariables': {}, 'nodes': nodes, 'edges': edges}
        ipc('flow_save', flow=flow)
        ipc('flow_publish', flowId=flow_id)

    endpoint = None
    peer = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    peer.bind(('127.0.0.1', 0))
    peer.settimeout(5)
    try:
        snap = ipc('snapshot')
        # Only run against the explicitly isolated workspace supplied to this script.
        assert 'test-results' in snap['dataDir'].lower(), 'Refusing to modify a normal user workspace'
        report['dataDir'] = snap['dataDir']
        report['url'] = page.url
        project = ipc('project_create', name='流程页真实 UDP 验收')['id']
        protocol = ipc('protocol_publish', xml=XML, name='通用 UDP 联调验收')['id']
        endpoint = ipc('endpoint_start', name='通用协议监听', bind='127.0.0.1:0', protocolId=protocol)
        report['endpoint'] = endpoint['bind']
        remote = f'127.0.0.1:{peer.getsockname()[1]}'
        common = [{'id': 'start', 'type': 'START', 'data': {'label': '开始'}}, {'id': 'end', 'type': 'END', 'data': {'label': '结束'}}, {'id': 'error', 'type': 'ERROR', 'data': {'label': '等待超时'}}]
        install_flow('ui-active', '主动发送并等待确认', common + [{'id': 'send', 'type': 'SEND_AND_WAIT', 'data': {'label': '设置输出', 'code': 33, 'expectedCodes': [34], 'timeoutMs': 2500, 'values': VALUES}}], [{'source': 'start', 'target': 'send'}, {'source': 'send', 'target': 'end', 'label': 'matched'}, {'source': 'send', 'target': 'error', 'label': 'timeout'}])
        passive_values = {'body': {'payload': {'value': '${response.body.payload.value}', 'flags': 129, 'count': 2, 'items': [{'value': 7}, {'value': 9}]}}}
        install_flow('ui-passive', '被动接收后回复', common + [{'id': 'wait', 'type': 'WAIT_PACKET', 'data': {'label': '等待对端上报', 'expectedCodes': [115], 'timeoutMs': 2500}}, {'id': 'reply', 'type': 'SEND', 'data': {'label': '回复对端', 'code': 33, 'values': passive_values}}], [{'source': 'start', 'target': 'wait'}, {'source': 'wait', 'target': 'reply', 'label': 'matched'}, {'source': 'wait', 'target': 'error', 'label': 'timeout'}, {'source': 'reply', 'target': 'end', 'label': 'success'}])
        install_flow('ui-timeout', '等待超时检查', common + [{'id': 'wait', 'type': 'WAIT_PACKET', 'data': {'label': '等待对端上报', 'expectedCodes': [115], 'timeoutMs': 300}}], [{'source': 'start', 'target': 'wait'}, {'source': 'wait', 'target': 'end', 'label': 'matched'}, {'source': 'wait', 'target': 'error', 'label': 'timeout'}])
        page.reload()
        page.locator('button.nav-item').filter(has_text='交互流程').click()
        select('流程选择', '主动发送并等待确认')
        page.get_by_label('对端地址', exact=True).fill(remote)
        page.get_by_role('button', name='启动流程').wait_for()
        expect(page.get_by_role('button', name='启动流程')).to_be_enabled()
        check('唯一匹配监听端点自动选择，启动入口始终可见')

        page.locator('.react-flow__node').filter(has_text='设置输出').click()
        page.get_by_label('输出值（body.payload.value）', exact=True).fill('43')
        expect(page.get_by_role('button', name='启动流程')).to_be_disabled()
        page.get_by_role('button', name='校验并发布').click()
        expect(page.get_by_role('button', name='启动流程')).to_be_enabled(timeout=10000)
        check('普通字段直接填写，修改后必须发布才能运行')
        page.get_by_role('button', name='预览报文', exact=True).click()
        preview = page.get_by_label('发送预览', exact=True)
        preview.get_by_text('输出值', exact=True).first.wait_for()
        assert ipc('packet_query')['total'] == 0
        assert not ipc('snapshot')['runs']
        check('发送前Rust编码预览可见，预览不发送也不创建运行实例')

        peer_result = {}
        def respond():
            try:
                packet, address = peer.recvfrom(65507)
                peer_result['received'] = packet.hex(' ').upper()
                assert packet == bytes.fromhex('21 2B 81 02 07 09'), packet
                peer.sendto(bytes([0x22]) + packet[1:], address)
            except Exception as error:
                peer_result['error'] = str(error)
        worker = threading.Thread(target=respond, daemon=True)
        worker.start()
        existing = {run['id'] for run in ipc('snapshot')['runs']}
        page.get_by_role('button', name='启动流程').click()
        run = await_run(existing)
        worker.join(6)
        assert not worker.is_alive() and not peer_result.get('error'), peer_result
        report['activeRun'] = run['id']
        report['peerReceived'] = peer_result['received']
        live = page.get_by_label('本次收发', exact=True)
        live.locator('[data-direction="TX"]').wait_for()
        live.locator('[data-direction="RX"]').wait_for()
        live.locator('[data-direction="RX"]').click()
        live.get_by_text('输出值', exact=True).first.wait_for()
        assert live.locator('.hex-grid button').all_text_contents() == ['22', '2B', '81', '02', '07', '09']
        assert len(ipc('packet_query', runId=run['id'])['items']) == 2
        check('主动发送与外部UDP对端响应均在流程页显示，按本次实例过滤')
        live.get_by_text('控制标志', exact=True).first.click()
        assert live.locator('.hex-grid button.highlighted').all_text_contents() == ['81']
        live.locator('[data-byte-index="1"]').click()
        assert live.locator('.selected-field').get_by_text('输出值', exact=True).count() == 1
        assert live.locator('.selected-field-values').get_by_text('43', exact=True).count() == 1
        check('同页HEX、十进制、Bit与字段解析双向定位，预览面板不串选')
        expect(page.locator('.ant-message-notice')).to_have_count(0, timeout=10000)
        page.set_viewport_size({'width': 1440, 'height': 1000})
        page.locator('main').evaluate('element => { element.scrollTop = 0; }')
        page.locator('.react-flow__controls-fitview').click()
        page.screenshot(path=str(output/'active-1440.png'), full_page=True)
        page.set_viewport_size({'width': 1280, 'height': 800})
        assert page.get_by_role('button', name='启动流程').is_visible()
        assert page.evaluate('document.documentElement.scrollWidth <= window.innerWidth')
        page.locator('.react-flow__controls-fitview').click()
        page.screenshot(path=str(output/'active-1280.png'), full_page=True)

        select('流程选择', '被动接收后回复')
        existing = {run['id'] for run in ipc('snapshot')['runs']}
        page.get_by_role('button', name='启动流程').click()
        passive = await_run(existing, 'WAITING')
        assert ipc('packet_query', runId=passive['id'])['total'] == 0
        host, port = endpoint['bind'].rsplit(':', 1)
        peer.sendto(bytes.fromhex('73 19 81 02 07 09'), (host, int(port)))
        packet, _ = peer.recvfrom(65507)
        assert packet == bytes.fromhex('21 19 81 02 07 09'), packet
        await_run(existing)
        live.locator('[data-direction="RX"]').wait_for()
        live.locator('[data-direction="TX"]').wait_for()
        rows = ipc('packet_query', runId=passive['id'])['items']
        assert len(rows) == 2 and {row['nodeId'] for row in rows} == {'wait', 'reply'}, rows
        assert not ipc('snapshot')['devices']
        check('被动等待真实73报文后按接收字段回复21，不触发MUSP设备或资源规则')
        report['passiveRun'] = passive['id']
        page.screenshot(path=str(output/'passive.png'), full_page=True)

        select('流程选择', '等待超时检查')
        existing = {run['id'] for run in ipc('snapshot')['runs']}
        page.get_by_role('button', name='启动流程').click()
        failed = await_run(existing, 'FAILED')
        assert ipc('packet_query', runId=failed['id'])['total'] == 0
        page.locator('.flow-observe .ant-tag').filter(has_text='失败').first.wait_for(timeout=5000)
        check('等待超时就地显示失败，没有伪造收发记录')
        report['timeoutRun'] = failed['id']
        page.screenshot(path=str(output/'timeout.png'), full_page=True)
        report['packetCount'] = ipc('packet_query')['total']
        assert report['packetCount'] == 4
        assert not report['pageErrors'], report['pageErrors']
        report['passed'] = True
    finally:
        peer.close()
        if endpoint:
            ipc('endpoint_stop', id=endpoint['id'])
        (output/'report.json').write_text(json.dumps(report, ensure_ascii=False, indent=2), 'utf-8')
