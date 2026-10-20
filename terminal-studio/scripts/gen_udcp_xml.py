# -*- coding: utf-8 -*-
"""从数据库生成 protocols/UDCP_1.0.xml 模板, 并给字段补上语义标记(role)写回数据库。

- 模板与库内数据一致（导出后再导入幂等）
- role 按 字段语义 映射标注: session/device_sn/device_list/device_count/op_type/result/src_id/dst_id
"""
import json
import sqlite3
import sys
from pathlib import Path

DB = Path(r"C:/Users/KYD-PC-357/AppData/Roaming/com.simterm.desktop/simterm.db")
OUT = Path(__file__).resolve().parent.parent / "protocols" / "UDCP_1.0.xml"

# (protocol_id 或 "header", 字段名) → role
ROLES = {
    ("header", "源设备ID"): "src_id",
    ("header", "目标设备ID"): "dst_id",
    ("header", "Session"): "session",
    ("udcp_reg_req", "序列号"): "device_sn",
    ("udcp_reg_resp", "注册结果"): "result",
    ("udcp_reg_resp", "分配设备ID"): "dst_id",
    ("udcp_reg_resp", "Session"): "session",
    ("udcp_cfg_req", "目标设备ID"): "dst_id",
    ("udcp_cfg_resp", "配置结果"): "result",
    ("udcp_cfg_resp", "目标设备ID"): "dst_id",
    ("udcp_status_report", "目标设备ID"): "dst_id",
    ("udcp_status_resp", "目标设备ID"): "dst_id",
    ("udcp_heartbeat", "目标设备ID"): "dst_id",
    ("udcp_heartbeat_resp", "目标设备ID"): "dst_id",
    ("udcp_data_on", "目标设备ID"): "dst_id",
    ("udcp_data_off", "目标设备ID"): "dst_id",
    ("udcp_data_ack", "目标设备ID"): "dst_id",
    ("udcp_devinfo_req", "操作类型"): "op_type",
    ("udcp_devinfo_req", "设备数量"): "device_count",
    ("udcp_devinfo_req", "设备列表"): "device_list",
    ("udcp_devinfo_req", "·序列号"): "device_sn",
    ("udcp_devinfo_resp", "操作类型"): "op_type",
    ("udcp_devinfo_resp", "配置数量"): "device_count",
    ("udcp_devinfo_resp", "应答结果"): "result",
    ("udcp_devinfo_resp", "目标设备ID"): "dst_id",
}


def esc(s: str) -> str:
    return (
        s.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
        .replace('"', "&quot;")
    )


def apply_roles(pid: str, fields: list) -> list:
    for f in fields:
        r = ROLES.get((pid, f["name"]))
        if r:
            f["role"] = r
        else:
            f.pop("role", None)
        for u in f.get("unit_fields", []):
            ur = ROLES.get((pid, "·" + u["name"]))
            if ur:
                u["role"] = ur
            else:
                u.pop("role", None)
    return fields


def field_xml(f: dict, indent: str) -> str:
    line = (
        f'{indent}<field name="{esc(f["name"])}" len="{f["len"]}" '
        f'enc="{f["enc"]}" value="{esc(f.get("value", ""))}"'
    )
    if f.get("dict"):
        line += f' dict="{f["dict"]}"'
    if f.get("role"):
        line += f' role="{f["role"]}"'
    units = f.get("unit_fields") or []
    if not units:
        return line + "/>\n"
    line += ">\n"
    for u in units:
        uline = (
            f'{indent}  <unit name="{esc(u["name"])}" len="{u["len"]}" '
            f'enc="{u["enc"]}" value="{esc(u.get("value", ""))}"'
        )
        if u.get("dict"):
            uline += f' dict="{u["dict"]}"'
        if u.get("role"):
            uline += f' role="{u["role"]}"'
        line += uline + "/>\n"
    return line + f"{indent}</field>\n"


def main():
    con = sqlite3.connect(str(DB))
    cur = con.cursor()

    # ---- 系统 + 公共头 ----
    sys_id, sys_name, header_json = cur.execute(
        "SELECT id, name, header_fields_json FROM protocol_systems WHERE name='UDCP'"
    ).fetchone()
    header = apply_roles("header", json.loads(header_json))
    cur.execute(
        "UPDATE protocol_systems SET header_fields_json=? WHERE id=?",
        (json.dumps(header, ensure_ascii=False), sys_id),
    )

    out = ['<?xml version="1.0" encoding="UTF-8"?>', "<simterm-import>", f'  <system name="{esc(sys_name)}" description="UDCP 1.0 物联网设备通信协议（18B 公共头含 2B Session 认证 + 13 个业务报文）">']
    out.append('    <header enabled="true">')
    for f in header:
        out.append(field_xml(f, "      "))
    out.append("    </header>")

    # ---- 码表 ----
    for key, name, blen in cur.execute(
        "SELECT \"key\", name, byte_len FROM dict_sets WHERE system_ref=? ORDER BY id",
        (sys_id,),
    ).fetchall():
        out.append(f'    <dict key="{esc(key)}" name="{esc(name)}" byte-len="{blen}">')
        for value, label in cur.execute(
            "SELECT value, label FROM dict_items WHERE set_ref=(SELECT id FROM dict_sets WHERE \"key\"=? AND system_ref=?) ORDER BY sort_order",
            (key, sys_id),
        ).fetchall():
            out.append(f'      <item value="{esc(value)}" label="{esc(label)}"/>')
        out.append("    </dict>")

    # ---- 分组协议（补 role 后写回 DB 并生成 XML） ----
    rows = cur.execute(
        "SELECT p.protocol_id, p.name, p.msg_type, p.group_ref, p.match_rule, "
        "p.auto_respond, p.respond_delay_ms, p.description, p.fields_json, g.name "
        "FROM protocols p LEFT JOIN protocol_groups g ON g.id=p.group_ref "
        "WHERE p.system_ref=? ORDER BY p.id",
        (sys_id,),
    ).fetchall()
    groups: dict[str, list] = {}
    for pid, name, mt, _g, match, auto, delay, desc, fj, gname in rows:
        fields = apply_roles(pid, json.loads(fj))
        cur.execute(
            "UPDATE protocols SET fields_json=? WHERE protocol_id=?",
            (json.dumps(fields, ensure_ascii=False), pid),
        )
        groups.setdefault(gname or "", []).append((pid, name, mt, match, auto, delay, desc, fields))

    for gname, protos in groups.items():
        if gname:
            out.append(f'    <group name="{esc(gname)}">')
            ind = "      "
        else:
            ind = "    "
        for pid, name, mt, match, auto, delay, desc, fields in protos:
            attrs = f'{ind}<protocol id="{esc(pid)}" name="{esc(name)}" type="{mt}"'
            if match:
                attrs += f' match="{esc(match)}"'
            if auto:
                attrs += f' auto-respond="{esc(auto)}"'
            if delay:
                attrs += f' delay="{delay}"'
            if desc:
                attrs += f' description="{esc(desc)}"'
            out.append(attrs + ">")
            for f in fields:
                out.append(field_xml(f, ind + "  "))
            out.append(f"{ind}</protocol>")
        if gname:
            out.append("    </group>")

    out.append("  </system>")
    out.append("</simterm-import>")
    import re
    text = re.sub(r"\n\s*\n", "\n", "\n".join(out) + "\n")
    OUT.write_text(text, encoding="utf-8")

    con.commit()
    n_role = sum(1 for (pid, name) in ROLES)
    print(f"模板已生成: {OUT}")
    print(f"标注语义 {n_role} 处, 已写回数据库(公共头+协议字段)")


if __name__ == "__main__":
    main()
