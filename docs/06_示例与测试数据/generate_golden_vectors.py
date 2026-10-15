"""仅生成 MUSP v0.8 的确定性配置请求/响应样本，不依赖第三方库。
运行: python generate_golden_vectors.py
CRC: CRC16/CCITT-FALSE poly1021 initFFFF refin/out=false xor0
"""
from pathlib import Path
import json

def crc16_ccitt_false(data: bytes) -> int:
    crc = 0xFFFF
    for b in data:
        crc ^= b << 8
        for _ in range(8):
            crc = ((crc << 1) ^ 0x1021) & 0xFFFF if crc & 0x8000 else (crc << 1) & 0xFFFF
    return crc

def header(msg_code, src, dst, body, *, txn=0x65, session=0x1234, flags=0, ext=b''):
    assert len(src)==len(dst)==3 and 0 <= msg_code <= 0xff
    head_len=23+len(ext)
    total=head_len+len(body)+2
    h=(bytes.fromhex('AA 55')+bytes([1,head_len])+total.to_bytes(2,'big')+bytes([msg_code])
       +src+dst+bytes([flags,0])+txn.to_bytes(4,'big')+session.to_bytes(2,'big')+len(body).to_bytes(2,'big')+ext)
    assert len(h)==head_len
    payload=h+body
    out=payload+crc16_ccitt_false(payload).to_bytes(2,'big')
    assert len(out)==total
    return out

def main():
    out=Path(__file__).resolve().parent
    A=bytes.fromhex('00 00 3B'); C=bytes.fromhex('00 00 01')
    # 5条单设备通道配置，每条 = 1+1+4+2+1+1 = 10B。
    items=[]
    for i in range(5):
        items.append(bytes([i+1,1])+((1000000+100*i).to_bytes(4,'big'))+(10+i).to_bytes(2,'big')+bytes([1,0]))
    assert all(len(x)==10 for x in items)
    config_common=bytes.fromhex('81 00 01 00 01 00 00 00')
    prefix=bytes.fromhex('02 01 05 0A 00 00 3B 00 01 00')
    ext=bytes.fromhex('A1 04 DE AD BE EF')
    request=header(0x21,C,A,config_common+prefix+b''.join(items),txn=0x65,flags=0x01,ext=ext)
    response=header(0x22,A,C,bytes.fromhex('05 AA AA AA AA AA'),txn=0x65,flags=0x02)
    assert len(request)==99 and len(response)==31
    for name,data in [('config-set-99B.hex',request),('config-result-31B.hex',response)]:
        (out/name).write_text(' '.join(f'{b:02X}' for b in data)+'\n',encoding='utf-8')
    summary={'request':{'hex':request.hex(' ').upper(),'lengthBytes':len(request),'crc':request[-2:].hex(' ').upper(),'bodyLengthBytes':68,'headerLengthBytes':29},
             'response':{'hex':response.hex(' ').upper(),'lengthBytes':len(response),'crc':response[-2:].hex(' ').upper(),'bodyLengthBytes':6,'headerLengthBytes':23}}
    (out/'golden-vectors.json').write_text(json.dumps(summary,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print('OK: CONFIG_SET=',len(request),'B, CRC=',summary['request']['crc'], 'CONFIG_RESULT=',len(response),'B, CRC=',summary['response']['crc'])

if __name__=='__main__':main()
