# -*- coding: utf-8 -*-
"""增量录入「设备信息上报/应答」协议对 + device_sn 设备档案码表(幂等, 可重复执行)

- 新分组: 设备配置
- msg_type 码表追加 0C/0D 两个条目; 新码表 op_type(操作类型: 01新增/02删除)
- 新码表: device_sn(设备档案, 8B), 初始 SN000001/SN000002
- 新协议: udcp_devinfo_req(操作类型+数量+变长设备列表) / udcp_devinfo_resp(数量回填+结果), 应答链 100ms
"""
import json
import sqlite3
import sys
from pathlib import Path

DB = Path(r"C:/Users/KYD-PC-357/AppData/Roaming/com.simterm.desktop/simterm.db")

REQ_FIELDS = [
    {"name": "消息类型", "len": 1, "enc": "hex", "value": "0C", "dict": "msg_type"},
    {"name": "目标设备ID", "len": 2, "enc": "hex", "value": "00 01"},
    {"name": "操作类型", "len": 1, "enc": "hex", "value": "01", "dict": "op_type"},
    {"name": "设备数量", "len": 2, "enc": "hex", "value": "00 01"},
    {"name": "设备列表", "len": 12, "enc": "hex", "value": "{vlen}", "unit_fields": [
        {"name": "序列号", "len": 8, "enc": "hex", "value": "", "dict": "device_sn"},
        {"name": "设备类型", "len": 2, "enc": "hex", "value": "", "dict": "device_type"},
        {"name": "心跳周期", "len": 2, "enc": "hex", "value": ""},
    ]},
]
RESP_FIELDS = [
    {"name": "消息类型", "len": 1, "enc": "hex", "value": "0D", "dict": "msg_type"},
    {"name": "目标设备ID", "len": 2, "enc": "hex", "value": "{req:源设备ID}"},
    {"name": "序号", "len": 4, "enc": "hex", "value": "{req:序号}"},
    {"name": "操作类型", "len": 1, "enc": "hex", "value": "{req:操作类型}", "dict": "op_type"},
    {"name": "配置数量", "len": 2, "enc": "hex", "value": "{req:设备数量}"},
    {"name": "应答结果", "len": 1, "enc": "hex", "value": "00", "dict": "cfg_result"},
    {"name": "预留", "len": 1, "enc": "hex", "value": "00"},
]

PROTOCOLS = [
    ("udcp_devinfo_req", "设备配置请求", "Request", "设备配置", REQ_FIELDS,
     "^55AA010C", "udcp_devinfo_resp", 100,
     "把一批设备配置到服务端 (TYPE=0x0C, 总长 19+12N 字节); 设备列表变长, 每台记录=序列号8B+类型2B+心跳2B, 由「批量配置设备」生成"),
    ("udcp_devinfo_resp", "设备配置应答", "Response", "设备配置", RESP_FIELDS,
     "^55AA010D", None, 0,
     "服务端对设备配置的应答 (TYPE=0x0D, 总长 21B); 应答结果=成功时, 本批设备自动同步设备档案（新增入库/删除移除）"),
]


def main():
    if not DB.exists():
        print(f"数据库不存在: {DB}")
        sys.exit(1)
    con = sqlite3.connect(str(DB))
    cur = con.cursor()
    cur.execute("PRAGMA foreign_keys = ON")

    row = cur.execute("SELECT id FROM protocol_systems WHERE name='UDCP'").fetchone()
    if not row:
        print("未找到 UDCP 系统, 请先运行 seed_udcp.py")
        sys.exit(1)
    sys_id = row[0]

    # ---- 分组: 设备配置 ----
    row = cur.execute(
        "SELECT id FROM protocol_groups WHERE name='设备配置' AND system_ref=?", (sys_id,)
    ).fetchone()
    if row:
        gid = row[0]
    else:
        cur.execute(
            "INSERT INTO protocol_groups (system_ref, name) VALUES (?, '设备配置')", (sys_id,)
        )
        gid = cur.lastrowid
        print(f"新增分组: 设备配置 (id={gid})")

    # ---- msg_type 追加 0C/0D ----
    msg_set = cur.execute(
        'SELECT id FROM dict_sets WHERE "key"=\'msg_type\' AND system_ref=?', (sys_id,)
    ).fetchone()[0]
    for value, label in (("0C", "设备信息上报"), ("0D", "设备信息应答")):
        exists = cur.execute(
            "SELECT 1 FROM dict_items WHERE set_ref=? AND value=?", (msg_set, value)
        ).fetchone()
        if not exists:
            nxt = cur.execute(
                "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM dict_items WHERE set_ref=?",
                (msg_set,),
            ).fetchone()[0]
            cur.execute(
                "INSERT INTO dict_items (set_ref, value, label, sort_order) VALUES (?, ?, ?, ?)",
                (msg_set, value, label, nxt),
            )
            print(f"msg_type 追加条目: {value} {label}")

    # ---- op_type 操作类型码表 ----
    row = cur.execute(
        "SELECT id FROM dict_sets WHERE \"key\"='op_type' AND system_ref=?", (sys_id,)
    ).fetchone()
    if row:
        op_set = row[0]
    else:
        cur.execute(
            'INSERT INTO dict_sets ("key", name, system_ref, byte_len, description) '
            "VALUES ('op_type', '操作类型', ?, 1, '设备配置操作: 01新增 02删除')",
            (sys_id,),
        )
        op_set = cur.lastrowid
        print("新增码表: op_type 操作类型 (id=%s, 1B)" % op_set)
    for value, label in (("01", "新增设备"), ("02", "删除设备")):
        exists = cur.execute(
            "SELECT 1 FROM dict_items WHERE set_ref=? AND value=?", (op_set, value)
        ).fetchone()
        if not exists:
            cur.execute(
                "INSERT INTO dict_items (set_ref, value, label, sort_order) VALUES (?, ?, ?, "
                "(SELECT COALESCE(MAX(sort_order), -1) + 1 FROM dict_items WHERE set_ref=?))",
                (op_set, value, label, op_set),
            )
            print(f"op_type 条目: {value} {label}")

    # ---- device_sn 设备档案码表 ----
    row = cur.execute(
        'SELECT id FROM dict_sets WHERE "key"=\'device_sn\' AND system_ref=?', (sys_id,)
    ).fetchone()
    if row:
        sn_set = row[0]
    else:
        cur.execute(
            'INSERT INTO dict_sets ("key", name, system_ref, byte_len, description) '
            "VALUES ('device_sn', '设备档案', ?, 8, '设备信息上报成功后自动入库的序列号档案')",
            (sys_id,),
        )
        sn_set = cur.lastrowid
        print(f"新增码表: device_sn 设备档案 (id={sn_set}, 8B)")
    for value, label in (
        ("534E303030303031", "SN000001"),
        ("534E303030303032", "SN000002"),
    ):
        exists = cur.execute(
            "SELECT 1 FROM dict_items WHERE set_ref=? AND value=?", (sn_set, value)
        ).fetchone()
        if not exists:
            cur.execute(
                "INSERT INTO dict_items (set_ref, value, label, sort_order) VALUES (?, ?, ?, "
                "(SELECT COALESCE(MAX(sort_order), -1) + 1 FROM dict_items WHERE set_ref=?))",
                (sn_set, value, label, sn_set),
            )
            print(f"设备档案初始条目: {label}")

    # ---- 协议 ----
    for pid, name, msg_type, _, fields, match, auto, delay, desc in PROTOCOLS:
        cur.execute(
            "INSERT INTO protocols (protocol_id, name, format, msg_type, def_mode, fields_json, "
            "payload_template, match_rule, auto_respond, respond_delay_ms, description, system_ref, group_ref) "
            "VALUES (?, ?, 'hex', ?, 'fields', ?, '', ?, ?, ?, ?, ?, ?) "
            "ON CONFLICT(protocol_id) DO UPDATE SET name=excluded.name, msg_type=excluded.msg_type, "
            "fields_json=excluded.fields_json, match_rule=excluded.match_rule, "
            "auto_respond=excluded.auto_respond, respond_delay_ms=excluded.respond_delay_ms, "
            "description=excluded.description, system_ref=excluded.system_ref, group_ref=excluded.group_ref",
            (pid, name, msg_type, json.dumps(fields, ensure_ascii=False),
             match, auto, delay, desc, sys_id, gid),
        )
        print(f"协议就绪: [{pid}] {name}")

    con.commit()
    n = cur.execute("SELECT COUNT(*) FROM protocols").fetchone()[0]
    print(f"完成, 协议总数 {n}")
    con.close()


if __name__ == "__main__":
    main()
