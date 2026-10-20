"""MUSP v0.8 local test peer. Config acknowledgments + authMode=0 registration/heartbeat.
Not a resource controller or a hardware configuration implementation.
"""
import argparse
import socket
import time
from collections import OrderedDict
from dataclasses import dataclass
from pathlib import Path

@dataclass
class Frame:
    code: int
    src: int
    dst: int
    txn: int
    sid: int
    body: bytes
    flags: int = 0
    extension: bytes = b''

def crc16(data):
    c = 0xffff
    for b in data:
        c ^= b << 8
        for _ in range(8):
            c = ((c << 1) ^ (0x1021 if c & 0x8000 else 0)) & 0xffff
    return c

def encode(f):
    h = 23 + len(f.extension)
    total = h + len(f.body) + 2
    b = (b'\xaa\x55\x01' + bytes([h]) + total.to_bytes(2,'big') + bytes([f.code])
         + f.src.to_bytes(3,'big') + f.dst.to_bytes(3,'big') + bytes([f.flags,0])
         + f.txn.to_bytes(4,'big') + f.sid.to_bytes(2,'big') + len(f.body).to_bytes(2,'big')
         + f.extension + f.body)
    return b + crc16(b).to_bytes(2,'big')

def decode(b):
    if len(b) < 25 or b[:3] != b'\xaa\x55\x01':
        raise ValueError('同步头、版本或最小长度不符')
    h=b[3]; total=int.from_bytes(b[4:6],'big'); n=int.from_bytes(b[21:23],'big')
    if h < 23 or h+n+2 != len(b) or total != len(b):
        raise ValueError('头部、业务体或报文长度不符')
    if crc16(b[:-2]) != int.from_bytes(b[-2:],'big'):
        raise ValueError('CRC错误，拒绝回包')
    if b[14] != 0 or b[13] & ~3:
        raise ValueError('保留字节或标志位不受支持')
    ext=b[23:h]
    if bool(b[13]&1) != bool(ext):
        raise ValueError('认证标志与扩展头不一致')
    if ext and (len(ext)<2 or ext[0]!=0xa1 or ext[1]+2!=len(ext)):
        raise ValueError('认证TLV长度不符或类型未支持')
    return Frame(b[6],int.from_bytes(b[7:10],'big'),int.from_bytes(b[10:13],'big'),
                 int.from_bytes(b[15:19],'big'),int.from_bytes(b[19:21],'big'),b[h:-2],b[13],ext)

class Peer:
    def __init__(self, common8=True, session_start=0x1234, allowed_devices=(0x3b,)):
        self.common8=common8
        self.next_sid=session_start
        self.allowed_devices=set(allowed_devices)
        self.sessions={}
        self.cache=OrderedDict()

    def reply(self, packet):
        f=decode(packet)
        if f.flags & 2:
            raise ValueError('应答信令不再应答')
        now=time.monotonic()
        if f.code in (3,0x31):
            current=self.sessions.get((f.src,f.dst))
            if not current or current['sid']!=f.sid or now>=current['expires']:
                raise ValueError('注册会话无效或已过期')
            if f.code==0x31 and not current['registered']:
                raise ValueError('注册尚未完成，拒绝心跳')
        if packet in self.cache and now-self.cache[packet][0]<3600:
            return self.cache[packet][1]
        sid=f.sid
        key=(f.src,f.dst)
        if f.code == 0x21:
            offset=8 if self.common8 else 0
            body=f.body[offset:]
            if len(body)<10:
                raise ValueError('配置专用前缀不足10字节')
            mode,op,count,size=body[:4]
            if mode not in (1,2) or op not in (1,2,3) or count>20 or size!=10 or len(body)!=10+count*size:
                raise ValueError('配置模式、操作或条目数量/长度不符')
            if mode==1 and body[4:7]!=b'\x00\x00\x00':
                raise ValueError('模式一目标设备地址必须为0')
            code=0x22; payload=bytes([count])+b'\xaa'*count
        elif f.code == 1:
            if len(f.body)!=10 or f.sid!=0 or f.flags!=0:
                raise ValueError('注册请求长度、会话或标志不符')
            if f.src not in self.allowed_devices:
                raise ValueError('设备未预配置；使用 --device 指定测试设备地址')
            sid=self.next_sid
            self.next_sid=(sid % 65535)+1
            self.sessions[key]={'sid':sid,'txn':f.txn,'numeric':f.body[1:5],
                                'caps':f.body[6:8],'registered':False,'expires':now+3600}
            code=2; payload=b'\x00'+(3600).to_bytes(4,'big')+b'\x00\x00'
        elif f.code in (3,0x31):
            state=self.sessions.get(key)
            if not state or now>=state['expires']:
                raise ValueError('尚未注册或许可已过期')
            if f.sid!=state['sid']:
                raise ValueError('会话号与注册许可不一致')
            if f.extension:
                raise ValueError('本测试场景为authMode=0，不接受认证扩展')
            if f.code==3:
                if len(f.body)!=5 or f.body[:4]!=state['numeric'] or f.body[4]!=0:
                    raise ValueError('注册确认内容不符')
                if f.txn==state['txn']:
                    raise ValueError('注册确认必须使用新事务号')
                state['registered']=True
                code=4; payload=b'\x00'+state['caps']
            else:
                if not state['registered']:
                    raise ValueError('注册尚未完成，拒绝心跳')
                if len(f.body)!=6:
                    raise ValueError('心跳业务体必须6字节')
                code=0x32; payload=b'\x00'+(int(time.time())&0xffffffff).to_bytes(4,'big')
        else:
            raise ValueError(f'未支持信令0x{f.code:02X}，不伪造成功')
        result=encode(Frame(code,f.dst,f.src,f.txn,sid,payload,2))
        self.cache[packet]=(now,result)
        while len(self.cache)>1024:
            self.cache.popitem(last=False)
        return result

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--bind',default='127.0.0.1')
    p.add_argument('--port',type=int,default=39002)
    p.add_argument('--config-common8',choices=['on','off'],default='on')
    p.add_argument('--session-start',type=lambda x:int(x,0),default=0x1234)
    p.add_argument('--device',action='append',type=lambda x:int(x,0))
    p.add_argument('--log',type=Path)
    a=p.parse_args()
    if not 1<=a.session_start<=65535: p.error('session-start必须为1..65535')
    peer=Peer(a.config_common8=='on',a.session_start,a.device or [0x3b])
    def log(s):
        text=time.strftime('%H:%M:%S')+' '+s
        print(text,flush=True)
        if a.log:
            a.log.parent.mkdir(parents=True,exist_ok=True)
            with a.log.open('a',encoding='utf-8') as f: f.write(text+'\n')
    with socket.socket(socket.AF_INET,socket.SOCK_DGRAM) as sock:
        sock.bind((a.bind,a.port))
        log(f'LISTEN {a.bind}:{a.port}; 配置应答 + 无认证注册 + 心跳；Ctrl+C停止')
        while True:
            packet,remote=sock.recvfrom(65535)
            log(f'RX {remote[0]}:{remote[1]} {packet.hex(" ").upper()}')
            try:
                response=peer.reply(packet)
                sock.sendto(response,remote)
                f=decode(response)
                log(f'TX 0x{f.code:02X} txn={f.txn} sid=0x{f.sid:04X} {response.hex(" ").upper()}')
            except ValueError as e:
                log(f'REJECT {e}')
if __name__=='__main__':
    try: main()
    except KeyboardInterrupt: pass
