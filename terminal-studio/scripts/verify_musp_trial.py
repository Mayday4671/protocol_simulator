"""Independent static fixture checks; not a substitute for base-engine tests."""
from pathlib import Path
import binascii
import json
import xml.etree.ElementTree as E

root=Path(__file__).resolve().parents[1]
doc=E.parse(root/'protocols/MUSP_v0.8_无人机联调_基座试录.xml')
samples=json.loads((root/'protocols/MUSP_v0.8_试录静态样例.json').read_text(encoding='utf-8'))
gold=json.loads((root.parent/'docs/06_示例与测试数据/golden-vectors.json').read_text(encoding='utf-8-sig'))
protocols=doc.findall('.//protocol')
assert len(protocols)==len(samples)==34
assert len({p.get('id') for p in protocols})==34
for group in doc.findall('.//group'):
    for p in group.findall('protocol'):
        header={f.get('name'):dict(f.attrib) for f in group.find('header')}
        body=[]
        for f in p.findall('field'):
            if f.get('header-ref'):
                header[f.get('header-ref')]['value']=f.get('value')
            else:
                body.append(dict(f.attrib))
        encoded=bytearray(); spans={}; active=[]; values={}
        for f in list(header.values())+body:
            if f.get('when-field') and values.get(f['when-field']) != f['when-value']:
                continue
            value=f['value']; length=int(f['len']); start=len(encoded)
            raw=(b'' if value=='{vlen}' else bytes(length) if value.startswith('{') else bytes.fromhex(value))
            if f.get('role')=='device_list':
                assert len(raw)%length==0
            elif value != '{vlen}':
                assert len(raw)==length, (p.get('id'),f['name'])
            encoded.extend(raw); spans[f['name']]=(start,len(encoded)); active.append(f); values[f['name']]=value
        for f in active:
            value=f['value']; start,end=spans[f['name']]
            if value=='{tlen}':
                encoded[start:end]=len(encoded).to_bytes(end-start,'big')
            elif value.startswith('{byte_length:'):
                first,last=value[13:-1].split(':')
                encoded[start:end]=(spans[last][0]-spans[first][0]).to_bytes(end-start,'big')
        for f in active:
            if f['value']=='{crc16_ccitt_false}':
                start,end=spans[f['name']]
                assert end==len(encoded)
                encoded[start:end]=binascii.crc_hqx(encoded[:start],0xffff).to_bytes(2,'big')
        assert encoded==bytes.fromhex(samples[p.get('id')]),p.get('id')
for p in protocols:
    assert p.find('payload') is None
    assert p.get('auto-respond') is None
    assert '静态' in p.get('description') or p.get('id') in ('musp_config_set','musp_config_result')
    for f in p.findall('.//field')+p.findall('.//unit'):
        assert int(f.get('len'))>0
    raw=bytes.fromhex(samples[p.get('id')])
    assert raw[:3]==bytes.fromhex('AA5501')
    assert len(raw)==int.from_bytes(raw[4:6],'big')
    assert len(raw)==raw[3]+int.from_bytes(raw[21:23],'big')+2
    assert binascii.crc_hqx(raw[:-2],0xffff)==int.from_bytes(raw[-2:],'big')
for pid,key in [('musp_config_set','request'),('musp_config_result','response')]:
    assert samples[pid]==gold[key]['hex']
print('PASS: 34 default fixtures; lengths and CRCs; 99B/31B golden vectors.')
print('Count changes and actual engine rendering are checked by musp_dynamic_config Rust tests.')
