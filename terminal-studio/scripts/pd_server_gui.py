#!/usr/bin/env python3
"""智能配电接入协议 PD-1.0 测试服务端(对端平台)。

用法: python scripts/pd_server_gui.py 4002 [--auto]

公共头 16B: 版本1B + 信令1B + 源地址4B + 目标地址4B + 流水号2B + 认证令牌2B + 报文长度2B
平台行为:
  01 开机注册   -> 02 注册许可: 按源地址分配递增 2B 令牌(0x1001 起), 令牌写回头内令牌位
  05 心跳保活   -> 06 心跳应答: 令牌不符回 02 令牌无效
  03 注销       -> 04 注销应答: 吊销该终端令牌
  11 参数设置   -> 12 设置应答(成功)
  13 参数查询   -> 14 查询响应(3 条参数 + 校验和)
  21/23/25 上报 -> 22/24/26 应答(成功, 流水回显)
"""
import socket
import sys
import threading

PLATFORM = "00000000"
HDR = 16


def log(msg):
    print(msg, file=sys.stderr, flush=True)


class PDServer:
    def __init__(self, port):
        self.port = port
        self.tokens = {}   # 源地址(hex) -> 令牌(hex)
        self.next_token = 0x1001
        self.lock = threading.Lock()

    # ---- 帧构造: 头内 源地址=平台, 目标地址=对端终端 ----
    def frame(self, sig, dst, seq, token, body=b""):
        total = HDR + len(body)
        head = bytes([0x21, sig]) + bytes.fromhex(PLATFORM) + bytes.fromhex(dst)
        head += bytes.fromhex(seq) + bytes.fromhex(token) + total.to_bytes(2, "big")
        return head + body

    # ---- 信令处理 ----
    def handle(self, data, addr):
        if len(data) < HDR:
            log(f"[忽略] 长度不足: {len(data)}B 来自 {addr}")
            return None
        sig = data[1]
        src = data[2:6].hex().upper()
        dst = data[6:10].hex().upper()
        seq = data[10:12].hex().upper()
        token = data[12:14].hex().upper()
        body = data[HDR:]
        name = {0x01: "开机注册", 0x03: "注销", 0x05: "心跳保活", 0x11: "参数设置",
                0x13: "参数查询", 0x21: "状态上报", 0x23: "告警上报", 0x25: "设备清单上报"}.get(sig, f"未知{sig:02X}")
        log(f"[收到] {name} 源={src} 目标={dst} 流水={seq} 令牌={token} 体长={len(body)}B")

        with self.lock:
            if sig == 0x01:  # 开机注册 -> 注册许可
                assigned = self.tokens.get(src)
                if assigned is None:
                    assigned = f"{self.next_token:04X}"
                    self.next_token += 1
                    self.tokens[src] = assigned
                    log(f"[令牌] 为终端 {src} 分配 {assigned}")
                else:
                    log(f"[令牌] 终端 {src} 重复注册, 沿用 {assigned}")
                return self.frame(0x02, src, seq, assigned, bytes([0x00]) + bytes.fromhex("00000E10"))

            if sig == 0x03:  # 注销
                self.tokens.pop(src, None)
                log(f"[令牌] 终端 {src} 令牌已吊销")
                return self.frame(0x04, src, seq, "0000", bytes([0x00]))

            expect = self.tokens.get(src)
            if expect is None:
                log(f"[拒绝] 终端 {src} 未注册")
                return self.frame(self.resp_of(sig), src, seq, "0000", bytes([0x03]))  # 未注册
            if token != expect:
                log(f"[拒绝] 终端 {src} 令牌不符: 期望 {expect} 实收 {token}")
                return self.frame(self.resp_of(sig), src, seq, "0000", bytes([0x02]))  # 令牌无效

            if sig == 0x05:
                return self.frame(0x06, src, seq, token, bytes([0x00]))
            if sig == 0x11:
                return self.frame(0x12, src, seq, token, bytes([0x00]))
            if sig == 0x13:  # 参数查询响应: 3 × (编号1B + 值2B) + 校验和2B
                entries = bytes.fromhex("01003C") + bytes.fromhex("020190") + bytes.fromhex("03001E")
                return self.frame(0x14, src, seq, token, bytes([0x03]) + entries + bytes.fromhex("55AA"))
            if sig == 0x21:
                return self.frame(0x22, src, seq, token, bytes([0x00]))
            if sig == 0x23:
                return self.frame(0x24, src, seq, token, bytes([0x00]))
            if sig == 0x25:
                log(f"[清单] {body[0]} 台: " + " ".join(body[1:1 + 5 * body[0]].hex().upper()[i:i + 8] for i in range(0, 5 * body[0] * 2, 10)))
                return self.frame(0x26, src, seq, token, bytes([0x00]))
        log(f"[忽略] 未支持信令 {sig:02X}")
        return None

    @staticmethod
    def resp_of(sig):
        return {0x05: 0x06, 0x11: 0x12, 0x13: 0x14, 0x21: 0x22, 0x23: 0x24, 0x25: 0x26}.get(sig, 0x00)

    def serve(self):
        sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        sock.bind(("0.0.0.0", self.port))
        log(f"[启动] PD-1.0 平台服务端监听 0.0.0.0:{self.port}")
        while True:
            data, addr = sock.recvfrom(65535)
            try:
                reply = self.handle(data, addr)
                if reply:
                    sock.sendto(reply, addr)
                    log(f"[回复] {len(reply)}B -> {addr}")
            except Exception as e:  # 单帧异常不致退出
                log(f"[异常] {e}")


if __name__ == "__main__":
    if len(sys.argv) < 2:
        print(__doc__)
        sys.exit(1)
    PDServer(int(sys.argv[1])).serve()
