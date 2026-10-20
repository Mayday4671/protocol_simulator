#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""设备通信协议测试服务端（配套 protocols/COMM_1.0.xml）

帧 = 源设备3B + 目标设备3B + 信令类型1B + 安全信息4B + 业务字段
用法: python udcp_server_gui.py [端口] [--auto]   (默认端口 4001)

流程: 设备配置请求→下发配置数据→收配置应答; 注册→分配安全信息并回注册响应;
      A呼叫请求→给B推来电→收B呼叫应答(校验B的安全信息)→给A回呼叫确认。
"""
import queue
import socket
import struct
import sys
import threading
import tkinter as tk
from tkinter import scrolledtext, ttk

SERVER_ID = b"\x00\x00\x00"

CODE_NAMES = {
    0x01: "配置请求", 0x02: "配置数据", 0x03: "配置应答",
    0x04: "注册", 0x05: "注册响应",
    0x06: "呼叫请求", 0x07: "来电", 0x08: "呼叫应答", 0x09: "呼叫确认",
    0x0A: "心跳", 0x0B: "心跳应答",
}


class ServerGui:
    def __init__(self):
        self.root = tk.Tk()
        self.root.title("设备通信协议 · 测试服务端")
        self.root.geometry("760x480")
        self.sock = None
        self.devices = {}   # 设备编号(3B) -> {"sec": 4B, "addr": (ip, port)}
        self.calls = {}     # 被叫编号 -> (主叫编号, 主叫地址)
        self.seq = 0
        self.logq = queue.Queue()
        self._build_ui()
        self.root.after(100, self._drain_log)

    def _build_ui(self):
        top = ttk.Frame(self.root)
        top.pack(fill="x", padx=8, pady=6)
        ttk.Label(top, text="端口").pack(side="left")
        self.port_var = tk.StringVar(value="4001")
        ttk.Entry(top, textvariable=self.port_var, width=8).pack(side="left", padx=4)
        self.btn_toggle = ttk.Button(top, text="启动", command=self.toggle)
        self.btn_toggle.pack(side="left", padx=4)
        self.status_var = tk.StringVar(value="未启动")
        ttk.Label(top, textvariable=self.status_var).pack(side="left", padx=12)
        ttk.Button(top, text="清屏", command=self.clear_log).pack(side="right")
        self.log_box = scrolledtext.ScrolledText(self.root, state="disabled", font=("Consolas", 10))
        self.log_box.pack(fill="both", expand=True, padx=8, pady=6)

    def log(self, tag, msg):
        line = f"[{tag}] {msg}"
        self.logq.put(line)
        print(line, file=sys.stderr, flush=True)

    def _drain_log(self):
        try:
            while True:
                line = self.logq.get_nowait()
                self.log_box.configure(state="normal")
                self.log_box.insert("end", line + "\n")
                self.log_box.see("end")
                self.log_box.configure(state="disabled")
        except queue.Empty:
            pass
        self.root.after(100, self._drain_log)

    def clear_log(self):
        self.log_box.configure(state="normal")
        self.log_box.delete("1.0", "end")
        self.log_box.configure(state="disabled")

    def toggle(self):
        if self.sock:
            self.stop()
        else:
            self.start()

    def start(self):
        try:
            port = int(self.port_var.get())
            sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
            sock.bind(("0.0.0.0", port))
        except Exception as e:
            self.log("err", f"启动失败: {e}")
            return
        self.sock = sock
        threading.Thread(target=self._recv_loop, daemon=True).start()
        self.btn_toggle.configure(text="停止")
        self.status_var.set(f"监听中 0.0.0.0:{port}")
        self.log("info", f"服务端已启动, 监听 0.0.0.0:{port}")

    def stop(self):
        if self.sock:
            self.sock.close()
            self.sock = None
        self.btn_toggle.configure(text="启动")
        self.status_var.set("未启动")
        self.log("info", "服务端已停止")

    def _recv_loop(self):
        sock = self.sock
        while True:
            try:
                data, addr = sock.recvfrom(65535)
            except OSError:
                return
            try:
                self.on_packet(data, addr)
            except Exception as e:
                self.log("err", f"处理报文异常: {e}")

    # ---------------- 协议逻辑 ----------------

    def build(self, dst, code, sec, extra=b""):
        """服务端出帧: 源=服务端(000000), 目标=指定设备"""
        return SERVER_ID + dst + bytes([code]) + sec + extra

    def send(self, frame, addr):
        self.sock.sendto(frame, addr)

    def on_packet(self, data, addr):
        if len(data) < 11:
            self.log("err", f"未知报文 {len(data)}B 来自 {addr}, 丢弃")
            return
        src, dst = data[0:3], data[3:6]
        code, sec, body = data[6], data[7:11], data[11:]
        who = src.hex().upper()
        dev = self.devices.get(src)
        name = CODE_NAMES.get(code, hex(code))

        # 安全信息鉴权: 配置请求/注册豁免(尚未分配); 配置应答在未注册时也豁免(配置先于注册)
        exempt = code in (0x01, 0x04) or (code == 0x03 and dev is None)
        if not exempt and (dev is None or dev["sec"] != sec):
            want = dev["sec"].hex().upper() if dev else "未注册"
            self.log("err", f"   {who} 安全信息鉴权失败: 收到 {sec.hex().upper()}, 期望 {want} → 拒绝处理")
            return
        self.log("in", f"← {who} {name}  安全信息={sec.hex().upper()}  来自 {addr[0]}:{addr[1]}")

        if code == 0x01:  # 配置请求 → 下发配置数据
            sec_out = dev["sec"] if dev else b"\x00\x00\x00\x00"
            extra = socket.inet_aton("192.168.0.1") + struct.pack(">H", 60)
            self.send(self.build(src, 0x02, sec_out, extra), addr)
            self.log("out", f"→ {who} 配置数据  服务器IP=192.168.0.1 心跳周期=60s (等待配置应答)")

        elif code == 0x03:  # 配置应答
            ok = body[0] == 0xAA if body else False
            self.log("info", f"   {who} 配置应答 {'成功(AA)' if ok else '失败'}")

        elif code == 0x04:  # 注册 → 分配安全信息
            self.seq += 1
            sec_new = struct.pack(">I", 0xA1B20000 + self.seq)
            self.devices[src] = {"sec": sec_new, "addr": addr}
            self.send(self.build(src, 0x05, sec_new, b"\xAA"), addr)
            self.log("out", f"→ {who} 注册响应 成功(AA) 分配安全信息={sec_new.hex().upper()}")

        elif code == 0x06:  # 呼叫请求(A主叫B) → 给B推来电
            callee = body[0:3]
            cdev = self.devices.get(callee)
            if cdev is None:
                self.log("err", f"   被叫 {callee.hex().upper()} 未注册, 呼叫失败")
                return
            self.calls[callee] = (src, addr)
            self.send(self.build(callee, 0x07, cdev["sec"], src), cdev["addr"])
            self.log("out", f"→ {callee.hex().upper()} 来电  主叫={who} (等待被叫应答)")

        elif code == 0x08:  # 呼叫应答(B) → 给A回呼叫确认
            result, caller = body[0], body[1:4]
            info = self.calls.pop(src, None)
            cdev = self.devices.get(caller)
            if info is None or cdev is None:
                self.log("err", f"   {who} 的呼叫应答找不到对应呼叫")
                return
            caller_sn, caller_addr = info
            label = "接收" if result == 0x01 else "拒绝"
            self.send(self.build(caller_sn, 0x09, cdev["sec"], bytes([result]) + src), caller_addr)
            self.log("out", f"→ {caller_sn.hex().upper()} 呼叫确认  {who} {label}, 呼叫建立")

        elif code == 0x0A:  # 心跳 → 回保活确认
            self.send(self.build(src, 0x0B, dev["sec"], bytes([0xAA])), addr)
            self.log("out", f"→ {who} 心跳应答 保活成功(AA)")

        else:
            self.log("err", f"未处理的信令类型 {name}")


if __name__ == "__main__":
    argv = sys.argv[1:]
    app = ServerGui()
    if argv and argv[0].isdigit():
        app.port_var.set(argv[0])
    if "--auto" in argv:
        app.root.after(200, app.start)
    app.root.mainloop()
