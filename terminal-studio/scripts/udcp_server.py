# -*- coding: utf-8 -*-
"""UDCP 1.0 测试服务端: 模拟协议中的"服务端"角色, 配合终端模拟器测试信令交互

用法:  python scripts/udcp_server.py [监听端口, 默认 9001]
模拟器侧配置: 设备节点 remote 指向 127.0.0.1:<本端口>, node_id 配 2 字节(如 10 01)

自动行为:
  收到 注册请求(01) → 回 注册应答(02, 成功/分配ID=请求方ID/心跳周期10s)
  收到 心跳(07)     → 回 心跳应答(08, 纯公共头)
  收到 状态上报(05) → 回 状态应答(06, 纯公共头)
  收到 数据(09,标志位=1) → 回 数据确认(0A, 序号回填请求序号)
键盘命令: c=下发配置请求  d=发送需确认数据(等ACK)  q=退出
"""
import socket
import struct
import sys
import threading
import time

MAGIC = 0x55AA
VER = 1
TYPE_NAME = {
    0x01: "注册请求", 0x02: "注册应答", 0x03: "配置请求", 0x04: "配置应答",
    0x05: "状态上报", 0x06: "状态应答", 0x07: "心跳", 0x08: "心跳应答",
    0x09: "数据传输", 0x0A: "数据确认",
}

port = int(sys.argv[1]) if len(sys.argv) > 1 else 9001
sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock.bind(("0.0.0.0", port))
server_seq = 0          # 服务端自身发送序号
devices = {}            # src_id -> 最近地址
last_addr = [None]      # 最近通信的对端地址


def log(msg):
    print(f"[{time.strftime('%H:%M:%S')}] {msg}", flush=True)


def build(t, flags, seq, src, dst, body=b""):
    total = 16 + len(body)
    return struct.pack(">HBBBBHIHH", MAGIC, VER, t, flags, 0, total, seq, src, dst) + body


def reply(t, req, addr, body=b""):
    """按请求头构造应答: 序号沿用请求序号, 目标=请求源"""
    _, _, _, flags, _, _, seq, src, dst = req
    sock.sendto(build(t, 0, seq, 0x0001, src, body), addr)


def next_seq():
    global server_seq
    server_seq += 1
    return server_seq


def send_to_last(t, flags, body, label):
    addr = last_addr[0]
    if not addr:
        log("!! 还没有设备通信过, 先让模拟器发一条报文")
        return
    _, _, _, _, _, _, _, src, _ = last_req[0]
    sock.sendto(build(t, flags, next_seq(), 0x0001, src, body), addr)
    log(f"→ 发送 {label} 到 {src:04X}@{addr[0]}:{addr[1]} 序号={server_seq}")


def on_packet(data, addr):
    if len(data) < 16:
        log(f"!! 丢弃过短报文 {len(data)}B 来自 {addr}")
        return
    magic, ver, t, flags, _rsv, length, seq, src, dst = struct.unpack(">HBBBBHIHH", data[:16])
    if magic != MAGIC:
        log(f"!! 非法帧头 {data[:2].hex()} 来自 {addr}, 丢弃")
        return
    body = data[16:length] if length <= len(data) else data[16:]
    name = TYPE_NAME.get(t, f"未知{t:02X}")
    flag_s = " 需确认" if flags & 1 else ""
    log(f"← {name}{flag_s} 来自 {src:04X}@{addr[0]}:{addr[1]} "
        f"序号={seq} 目标={dst:04X} 长度={length} hex={data.hex(' ', 1).upper()}")
    devices[src] = addr
    last_addr[0] = addr
    last_req[0] = (magic, ver, t, flags, _rsv, length, seq, src, dst)

    if t == 0x01 and len(body) >= 14:  # 注册请求 → 注册应答
        dtype, dver, sver, sn = struct.unpack(">HHH8s", body[:14])
        log(f"   设备类型={dtype:04X} 硬件版本={dver} 软件版本={sver} 序列号={sn.decode(errors='replace').rstrip(chr(0))}")
        # 结果=00成功, 分配设备ID=沿用请求方ID, 心跳周期=10s
        reply(0x02, last_req[0], addr, struct.pack(">BHHB", 0x00, src, 10, 0))
        log(f"→ 注册应答(成功, 分配ID={src:04X}, 心跳10s)")
    elif t == 0x07 and len(body) >= 8:  # 心跳 → 心跳应答
        uptime, status = struct.unpack(">IB3x", body[:8])
        log(f"   运行时长={uptime}s 设备状态={status:02X}")
        reply(0x08, last_req[0], addr)
        log("→ 心跳应答")
    elif t == 0x05 and len(body) >= 16:  # 状态上报 → 状态应答
        st, cpu, mem, temp, rx, tx = struct.unpack(">BBBHII", body[:13])
        log(f"   状态={st:02X} CPU={cpu}% 内存={mem}% 温度={temp/10}℃ 收包={rx} 发包={tx}")
        reply(0x06, last_req[0], addr)
        log("→ 状态应答")
    elif t == 0x09:  # 数据传输
        if len(body) >= 4:
            dlen, dtype, _r = struct.unpack(">HBB", body[:4])
            payload = body[4:4 + dlen]
            log(f"   数据长度={dlen} 类型={dtype:02X} 内容={payload.decode(errors='replace')}")
        if flags & 1:  # 需确认 → 数据确认(序号回填)
            reply(0x0A, last_req[0], addr)
            log(f"→ 数据确认(序号={seq})")
    elif t == 0x04 and len(body) >= 2:  # 配置应答(对 c 命令的回应)
        result, = struct.unpack(">B", body[:1])
        log(f"   配置结果={'成功' if result == 0 else '失败'}")
    elif t == 0x0A:  # 数据确认(对 d 命令的回应)
        log(f"   收到 ACK, 确认的序号={seq}")


last_req = [(0, 0, 0, 0, 0, 0, 0, 0, 0)]


def console():
    hint = "命令: c=下发配置请求  d=发送需确认数据  q=退出"
    print(hint, flush=True)
    while True:
        try:
            cmd = input().strip().lower()
        except EOFError:  # 无标准输入(后台运行)时静默关闭命令线程
            return
        if cmd == "q":
            log("退出")
            sys.exit(0)
        elif cmd == "c":
            body = struct.pack(">4sHHHBB", socket.inet_aton("127.0.0.1"), port, 10, 5, 0x01, 0)
            send_to_last(0x03, 0, body, "配置请求(IP/端口/心跳10s/上报5s/正常模式)")
        elif cmd == "d":
            payload = b"HELLO-SRV"
            body = struct.pack(">HBB", len(payload), 0x02, 0) + payload
            send_to_last(0x09, 1, body, "数据传输(需确认, HELLO-SRV)")
        elif cmd:
            print(hint, flush=True)


threading.Thread(target=console, daemon=True).start()
log(f"UDCP 测试服务端已启动, 监听 0.0.0.0:{port} (Ctrl+C 退出)")
try:
    while True:
        data, addr = sock.recvfrom(65535)
        try:
            on_packet(data, addr)
        except Exception as e:  # 解析错误不退出服务
            log(f"!! 处理报文异常: {e}")
except KeyboardInterrupt:
    log("退出")
