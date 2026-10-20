# -*- coding: utf-8 -*-
"""UDCP 1.0 协议集录入: 清空旧协议数据, 按 protocols/UDCP_1.0.md 模板逐字段写入数据库。
建模: 16B 公共头全在系统级(全 hex); 协议只含消息体 + 与公共头同名的覆盖字段。
用法: 先停止应用, 再运行  python scripts/seed_udcp.py
"""
import json
import sqlite3
import sys
from pathlib import Path

DB = Path.home() / "AppData" / "Roaming" / "com.simterm.desktop" / "simterm.db"

# ---------------- 码表 ----------------
DICTS = [
    ("msg_type", "消息类型", 1, [
        ("01", "注册请求"), ("02", "注册应答"), ("03", "配置请求"), ("04", "配置应答"),
        ("05", "状态上报"), ("06", "状态应答"), ("07", "心跳"), ("08", "心跳应答"),
        ("09", "数据传输"), ("0A", "数据确认"),
    ]),
    ("reg_result", "注册结果", 1, [
        ("00", "成功"), ("01", "未知设备"), ("02", "参数错误"), ("03", "设备已注册"),
        ("04", "服务端忙"), ("05", "版本不支持"), ("06", "认证失败"),
    ]),
    ("cfg_result", "配置结果", 1, [("00", "配置成功"), ("01", "配置失败")]),
    ("device_status", "设备状态", 1, [
        ("00", "未知"), ("01", "正常"), ("02", "警告"), ("03", "故障"), ("04", "维护"),
    ]),
    ("device_type", "设备类型", 2, [
        ("0001", "普通终端"), ("0002", "通信设备"), ("0003", "网关"),
        ("0004", "控制设备"), ("0005", "服务器"), ("FFFF", "其他"),
    ]),
    ("work_mode", "工作模式", 1, [
        ("00", "待机"), ("01", "正常工作"), ("02", "调试"), ("03", "维护"),
    ]),
    ("data_type", "数据类型", 1, [
        ("01", "二进制"), ("02", "文本"), ("03", "JSON"), ("04", "控制数据"),
        ("05", "文件数据"), ("06", "音频"), ("07", "视频"), ("08", "自定义"),
    ]),
]

# 16B 公共头(系统级, 全 hex); 消息类型默认 01, 各协议同名覆盖
HEADER_FIELDS = [
    {"name": "帧头", "len": 2, "enc": "hex", "value": "55 AA"},
    {"name": "版本号", "len": 1, "enc": "hex", "value": "01"},
    {"name": "消息类型", "len": 1, "enc": "hex", "value": "01", "dict": "msg_type"},
    {"name": "标志位", "len": 1, "enc": "hex", "value": "00"},
    {"name": "保留", "len": 1, "enc": "hex", "value": "00"},
    {"name": "报文总长", "len": 2, "enc": "hex", "value": "{tlen}"},
    {"name": "序号", "len": 4, "enc": "hex", "value": "{seq}"},
    {"name": "源设备ID", "len": 2, "enc": "hex", "value": "{node_id}"},
    {"name": "目标设备ID", "len": 2, "enc": "hex", "value": "00 01"},
]


def f(name, ln, value, dict_key=None):
    field = {"name": name, "len": ln, "enc": "hex", "value": value}
    if dict_key:
        field["dict"] = dict_key
    return field


# 公共头覆盖字段简写
TYPE = lambda v: f("消息类型", 1, v, "msg_type")
DST = lambda v: f("目标设备ID", 2, v)

# ---------------- 协议 (protocol_id, 名称, 类型, 分组, 字段, 匹配, 自动应答, 延迟, 描述) ----------------
PROTOCOLS = [
    ("udcp_reg_req", "注册请求", "Request", "注册管理",
     [TYPE("01"),
      f("设备类型", 2, "00 01", "device_type"),
      f("设备版本", 2, "00 01"),
      f("软件版本", 2, "00 01"),
      f("序列号", 8, "53 4E 30 30 30 30 30 31")],
     "^55AA0101", "udcp_reg_resp", 100, "设备上电后向服务端注册 (TYPE=0x01, 总长 30B); 序列号默认 SN000001"),

    ("udcp_reg_resp", "注册应答", "Response", "注册管理",
     [TYPE("02"), DST("{req:源设备ID}"),
      f("注册结果", 1, "00", "reg_result"),
      f("分配设备ID", 2, "10 01"),
      f("心跳周期", 2, "00 0A"),
      f("预留", 1, "00")],
     "^55AA0102", None, 0, "服务端对注册请求的应答 (TYPE=0x02, 总长 22B); 设备侧将其设为入网等待协议"),

    ("udcp_cfg_req", "配置请求", "Request", "注册管理",
     [TYPE("03"), DST("10 01"),
      f("服务器IP", 4, "7F 00 00 01"),
      f("服务器端口", 2, "13 88"),
      f("心跳周期", 2, "00 0A"),
      f("上报周期", 2, "00 05"),
      f("工作模式", 1, "01", "work_mode"),
      f("预留", 1, "00")],
     "^55AA0103", "udcp_cfg_resp", 100, "服务端向设备下发配置 (TYPE=0x03, 总长 28B)"),

    ("udcp_cfg_resp", "配置应答", "Response", "注册管理",
     [TYPE("04"), DST("{req:源设备ID}"),
      f("配置结果", 1, "00", "cfg_result"),
      f("预留", 1, "00")],
     "^55AA0104", None, 0, "设备对配置请求的应答 (TYPE=0x04, 总长 18B)"),

    ("udcp_status_report", "状态上报", "Event", "状态维护",
     [TYPE("05"),
      f("设备状态", 1, "01", "device_status"),
      f("CPU占用", 1, "17"),
      f("内存占用", 1, "2D"),
      f("温度", 2, "01 0C"),
      f("收包数", 4, "00 00 03 E8"),
      f("发包数", 4, "00 00 07 D0"),
      f("预留", 3, "00 00 00")],
     "^55AA0105", None, 0, "设备周期上报自身状态 (TYPE=0x05, 总长 32B, 不需应答); 温度单位 0.1℃"),

    ("udcp_status_resp", "状态应答", "Response", "状态维护",
     [TYPE("06"), DST("{req:源设备ID}")],
     "^55AA0106", None, 0, "服务端对状态上报的应答 (TYPE=0x06, 纯公共头 16B)"),

    ("udcp_heartbeat", "心跳", "Heartbeat", "状态维护",
     [TYPE("07"),
      f("运行时长", 4, "00 00 02 58"),
      f("设备状态", 1, "01", "device_status"),
      f("预留", 3, "00 00 00")],
     "^55AA0107", "udcp_heartbeat_resp", 50, "设备周期保活 (TYPE=0x07, 总长 24B); 心跳绑定选本协议, 入网成功后自动启动"),

    ("udcp_heartbeat_resp", "心跳应答", "Response", "状态维护",
     [TYPE("08"), DST("{req:源设备ID}")],
     "^55AA0108", None, 0, "服务端对心跳的应答 (TYPE=0x08, 纯公共头 16B)"),

    ("udcp_data_on", "数据传输(需确认)", "Event", "数据传输",
     [TYPE("09"), f("标志位", 1, "01"), DST("10 02"),
      f("数据长度", 2, "00 05"),
      f("数据类型", 1, "02", "data_type"),
      f("预留", 1, "00"),
      f("数据内容", 5, "48 45 4C 4C 4F")],
     "^55AA010901", "udcp_data_ack", 50, "设备间数据, 标志位=1 需对端确认 (TYPE=0x09, 总长 25B); 默认内容 HELLO"),

    ("udcp_data_off", "数据传输(免确认)", "Event", "数据传输",
     [TYPE("09"), DST("10 02"),
      f("数据长度", 2, "00 05"),
      f("数据类型", 1, "02", "data_type"),
      f("预留", 1, "00"),
      f("数据内容", 5, "48 45 4C 4C 4F")],
     "^55AA010900", None, 0, "设备间数据, 标志位=0 不需确认 (TYPE=0x09, 总长 25B)"),

    ("udcp_data_ack", "数据确认", "Response", "数据传输",
     [TYPE("0A"), f("序号", 4, "{req:序号}"), DST("{req:源设备ID}")],
     "^55AA010A", None, 0, "对需确认数据的 ACK (TYPE=0x0A, 纯公共头 16B); 序号与目标设备ID 回填自被确认的请求"),
]


def main():
    if not DB.exists():
        print(f"数据库不存在: {DB}")
        sys.exit(1)
    con = sqlite3.connect(str(DB))
    cur = con.cursor()
    cur.execute("PRAGMA foreign_keys = ON")

    # ---- 清理旧协议数据 (节点与日志保留) ----
    for table in ("heartbeat_bindings", "dict_items", "dict_sets",
                  "protocols", "protocol_groups", "protocol_systems"):
        cur.execute(f"DELETE FROM {table}")
    print("已清空: 协议 / 系统 / 分组 / 码表 / 心跳绑定")

    # ---- 系统与 16B 公共头 ----
    cur.execute(
        "INSERT INTO protocol_systems (name, description, header_enabled, header_fields_json) "
        "VALUES ('UDCP', 'UDCP 1.0 物联网设备通信协议', 1, ?)",
        (json.dumps(HEADER_FIELDS, ensure_ascii=False),),
    )
    sys_id = cur.lastrowid

    # ---- 分组 ----
    groups = {}
    for gname in ("注册管理", "状态维护", "数据传输"):
        cur.execute("INSERT INTO protocol_groups (system_ref, name) VALUES (?, ?)", (sys_id, gname))
        groups[gname] = cur.lastrowid

    # ---- 码表 ----
    for key, name, byte_len, items in DICTS:
        cur.execute(
            'INSERT INTO dict_sets ("key", name, system_ref, byte_len, description) '
            "VALUES (?, ?, ?, ?, '由 UDCP 模板导入')",
            (key, name, sys_id, byte_len),
        )
        set_id = cur.lastrowid
        for sort, (value, label) in enumerate(items):
            cur.execute(
                "INSERT INTO dict_items (set_ref, value, label, sort_order) VALUES (?, ?, ?, ?)",
                (set_id, value, label, sort),
            )

    # ---- 协议 ----
    for pid, name, msg_type, group, fields, match, auto, delay, desc in PROTOCOLS:
        cur.execute(
            "INSERT INTO protocols (protocol_id, name, format, msg_type, def_mode, fields_json, "
            "payload_template, match_rule, auto_respond, respond_delay_ms, description, system_ref, group_ref) "
            "VALUES (?, ?, 'hex', ?, 'fields', ?, '', ?, ?, ?, ?, ?, ?)",
            (pid, name, msg_type, json.dumps(fields, ensure_ascii=False),
             match, auto, delay, desc, sys_id, groups[group]),
        )

    con.commit()

    # ---- 校验 ----
    n_protos = cur.execute("SELECT COUNT(*) FROM protocols").fetchone()[0]
    n_dicts = cur.execute("SELECT COUNT(*) FROM dict_sets").fetchone()[0]
    n_items = cur.execute("SELECT COUNT(*) FROM dict_items").fetchone()[0]
    header = cur.execute(
        "SELECT header_enabled, header_fields_json FROM protocol_systems WHERE id=?", (sys_id,)
    ).fetchone()
    hfields = json.loads(header[1])
    print(f"系统 UDCP(id={sys_id}) 公共头启用={header[0]} 字段数={len(hfields)} 总长={sum(x['len'] for x in hfields)}B")
    print(f"协议 {n_protos} 个, 码表 {n_dicts} 张, 条目 {n_items} 条")
    for pid, name, mt, gname, auto in cur.execute(
        "SELECT p.protocol_id, p.name, p.msg_type, g.name, p.auto_respond FROM protocols p "
        "JOIN protocol_groups g ON g.id = p.group_ref ORDER BY p.id"
    ).fetchall():
        chain = f" → {auto}" if auto else ""
        print(f"  [{pid}] {name} ({mt}/{gname}){chain}")
    con.close()
    print("录入完成")


if __name__ == "__main__":
    main()
