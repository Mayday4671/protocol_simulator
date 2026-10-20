"""Local UDP peer for the base trial. Sends documented static vectors, no protocol emulation."""
import argparse, json, socket, time
from pathlib import Path
root = Path(__file__).resolve().parents[1]
p = argparse.ArgumentParser()
p.add_argument('--mode', choices=['respond', 'receive-samples', 'verify-sends'], default='respond')
p.add_argument('--port', type=int, default=39002)
p.add_argument('--target-port', type=int, default=39001)
a = p.parse_args()
samples = json.loads((root/'protocols/MUSP_v0.8_试录静态样例.json').read_text(encoding='utf-8'))
with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
    sock.bind(('127.0.0.1', a.port))
    sock.settimeout(120)
    if a.mode == 'respond':
        print('READY: 127.0.0.1:' + str(a.port), flush=True)
        packet, remote = sock.recvfrom(65535)
        expected = bytes.fromhex(samples['musp_config_set'])
        if packet != expected:
            raise SystemExit('FAIL: outgoing configuration differs from golden vector: ' + packet.hex(' '))
        response = bytes.fromhex(samples['musp_config_result'])
        sock.sendto(response, remote)
        print(json.dumps({'request_bytes':len(packet), 'request_matches_golden':True, 'response_bytes':len(response), 'remote':remote}), flush=True)
    elif a.mode == 'verify-sends':
        expected = {bytes.fromhex(v)[6]: (k, bytes.fromhex(v)) for k,v in samples.items()}
        seen = set()
        print('READY: verify 34 client sends', flush=True)
        while len(seen) < len(expected):
            packet, remote = sock.recvfrom(65535)
            name, wanted = expected[packet[6]]
            if packet != wanted:
                raise SystemExit('FAIL: ' + name + ' differs: ' + packet.hex(' '))
            seen.add(packet[6])
            print('PASS: ' + name, flush=True)
        print('PASS: all 34 client sends match static fixtures', flush=True)
    else:
        for name, value in samples.items():
            packet = bytes.fromhex(value)
            sock.sendto(packet, ('127.0.0.1', a.target_port))
            print(name + ': ' + str(len(packet)) + ' bytes', flush=True)
            time.sleep(0.3)
