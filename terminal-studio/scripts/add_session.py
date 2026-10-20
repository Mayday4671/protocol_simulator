# -*- coding: utf-8 -*-
"""增量启用 Session 认证（幂等, 可重复执行）

- 公共头在「目标设备ID」后插入 Session(4B, {session}) → 头 16B → 20B
- 注册应答插入 Session 字段(4B, {random}, 公共头同名覆盖, 不占消息体)
"""
import json
import sqlite3
import sys
from pathlib import Path

DB = Path(r"C:/Users/KYD-PC-357/AppData/Roaming/com.simterm.desktop/simterm.db")

SESSION_FIELD = {"name": "Session", "len": 2, "enc": "hex", "value": "{session}"}
RESP_SESSION_FIELD = {"name": "Session", "len": 2, "enc": "hex", "value": "{random}"}


def main():
    if not DB.exists():
        print(f"数据库不存在: {DB}")
        sys.exit(1)
    con = sqlite3.connect(str(DB))
    cur = con.cursor()

    # ---- 公共头插入 Session ----
    sys_id, header_json = cur.execute(
        "SELECT id, header_fields_json FROM protocol_systems WHERE name='UDCP'"
    ).fetchone()
    header = json.loads(header_json)
    existed = next((f for f in header if f["name"] == "Session"), None)
    if existed is None:
        idx = next(
            (i + 1 for i, f in enumerate(header) if f["name"] == "目标设备ID"),
            len(header),
        )
        header.insert(idx, SESSION_FIELD)
    else:
        existed.update(SESSION_FIELD)
    cur.execute(
        "UPDATE protocol_systems SET header_fields_json=? WHERE id=?",
        (json.dumps(header, ensure_ascii=False), sys_id),
    )
    print(f"公共头 Session(2B) 就绪: 头总长 {sum(f['len'] for f in header)}B")

    # ---- 注册应答插入 Session(公共头同名覆盖, 值 {random}) ----
    fields_json = cur.execute(
        "SELECT fields_json FROM protocols WHERE protocol_id='udcp_reg_resp'"
    ).fetchone()[0]
    fields = json.loads(fields_json)
    existed = next((f for f in fields if f["name"] == "Session"), None)
    if existed is None:
        idx = next(
            (i + 1 for i, f in enumerate(fields) if f["name"] == "分配设备ID"),
            len(fields),
        )
        fields.insert(idx, RESP_SESSION_FIELD)
    else:
        existed.update(RESP_SESSION_FIELD)
    cur.execute(
        "UPDATE protocols SET fields_json=? WHERE protocol_id='udcp_reg_resp'",
        (json.dumps(fields, ensure_ascii=False),),
    )
    print("注册应答 Session(2B, {random}) 就绪")

    con.commit()
    con.close()
    print("完成")


if __name__ == "__main__":
    main()
