"""Local UDP platform for the custom IoT walkthrough. No traffic is initiated.
python scripts/iot_walkthrough_peer.py --port 39162
"""
import argparse
import json
import secrets
import socket
import struct
import time

HEADER=struct.Struct('!2sBBHHHHH')
LENGTHS={0x01:7,0x02:5,0x10:4,0x11:1,0x20:1,0x7F:1}

class Platform:
    def __init__(self): self.devices={}
    def reply(self,data,peer):
        if len(data)<HEADER.size:return None
        magic,version,code,src,dst,seq,token,total=HEADER.unpack_from(data)
        if magic!=b'\xc0\xde' or version!=1 or code not in LENGTHS or dst!=0 or total!=len(data):return None
        body=data[HEADER.size:];key=(peer,src)
        state=self.devices.get(key)
        result=0
        if len(body)!=LENGTHS[code]:result=2
        elif code==1:
            if body[0]!=1:result=2
            else:
                state=dict(token=secrets.randbelow(65535)+1,period=10,threshold=300,relay=0)
                self.devices[key]=state;token=state['token']
        elif not state:result=3
        elif token!=state['token']:result=4
        elif code==0x10:
            period,threshold=struct.unpack('!HH',body)
            if not 1<=period<=3600 or not 1<=threshold<=2000:result=2
            else:state.update(period=period,threshold=threshold)
        elif code==0x11 and body!=b'\x01':result=2
        elif code==0x20:
            if body[0]>1:result=2
            else:state['relay']=body[0]
        period=state['period'] if state else 0
        relay=state['relay'] if state else 0
        extra={1:struct.pack('!H',30),2:struct.pack('!I',int(time.time())),
               0x10:struct.pack('!H',period),0x11:struct.pack('!HHBH',250,500,relay,period),
               0x20:bytes([relay]),0x7F:b''}[code]
        payload=bytes([result])+extra
        answer=HEADER.pack(magic,version,code|0x80,dst,src,seq,token,HEADER.size+len(payload))+payload
        if code==0x7F and result==0:self.devices.pop(key,None)
        return answer

def serve(port,delay_ms):
    platform=Platform()
    with socket.socket(socket.AF_INET,socket.SOCK_DGRAM) as sock:
        sock.bind(('127.0.0.1',port));print(json.dumps(dict(event='ready',address=f'127.0.0.1:{port}')),flush=True)
        while True:
            data,peer=sock.recvfrom(65535);answer=platform.reply(data,peer)
            if answer is None:
                print(json.dumps(dict(event='ignored',peer=str(peer),hex=data.hex())),flush=True);continue
            time.sleep(delay_ms/1000);sock.sendto(answer,peer)
            print(json.dumps(dict(event='exchange',peer=str(peer),request=data.hex().upper(),response=answer.hex().upper())),flush=True)

if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--port',type=int,default=39162);p.add_argument('--delay-ms',type=int,default=250)
    a=p.parse_args();serve(a.port,a.delay_ms)
