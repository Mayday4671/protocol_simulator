# -*- coding: utf-8 -*-
"""XML 协议模板 → 数据库导入器(与 UI「导入 .xml」同语义, 幂等可重复执行)

层级: system > group > (header | dict | protocol); 根级 dict = 全局共享字典。
公共头/分组专属字典都挂在 <group> 内; 兼容旧模板的 system 级 header/dict
（header 视为该系统所有分组的默认头, dict 归入该系统各分组）。

用法: python scripts/import_xml.py <模板.xml>
"""
import json
import sqlite3
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

DB = Path(r"C:/Users/KYD-PC-357/AppData/Roaming/com.simterm.desktop/simterm.db")

MSG_TYPE_MAP = {"Request": "Request", "Response": "Response", "Heartbeat": "Heartbeat", "Event": "Event"}


def field_json(el) -> dict:
    f = {
        "name": el.get("name", "").strip(),
        "len": int(el.get("len", "1")),
        "enc": el.get("enc", "hex"),
        "value": el.get("value", "").strip(),
    }
    if el.get("dict"):
        f["dict"] = el.get("dict").strip()
    if el.get("profile-key"):
        f["profile_key"] = el.get("profile-key").strip()
    if el.get("header-ref"):
        f["hdr_ref"] = el.get("header-ref").strip()
    if el.get("role") and el.get("role") != "none":
        f["role"] = el.get("role").strip()
    if el.get("when-field") and el.get("when-value"):
        f["when"] = {
            "field": el.get("when-field").strip(),
            "value": el.get("when-value").strip(),
        }
    units = [field_json(u) for u in el.findall("unit")]
    if units:
        f["unit_fields"] = units
    return f


def cond(c) -> dict:
    return {"field": c.get("field", "").strip(), "value": c.get("value", "").strip()}



def main():
    if len(sys.argv) < 2:
        print("用法: python scripts/import_xml.py <模板.xml>")
        sys.exit(1)
    tpl = Path(sys.argv[1])
    if not tpl.exists():
        print(f"模板不存在: {tpl}")
        sys.exit(1)
    root = ET.parse(tpl).getroot()
    if root.tag != "simterm-import":
        print("根元素须为 <simterm-import>")
        sys.exit(1)

    con = sqlite3.connect(str(DB))
    cur = con.cursor()
    cur.execute("PRAGMA foreign_keys = ON")
    stats = {"systems": 0, "groups": 0, "dicts": 0, "protocols": 0}

    def resolve_system(name: str, desc=""):
        row = cur.execute("SELECT id FROM protocol_systems WHERE name=?", (name,)).fetchone()
        if row:
            return row[0]
        cur.execute(
            "INSERT INTO protocol_systems (name, description) VALUES (?, ?)",
            (name, desc or name),
        )
        return cur.lastrowid

    def resolve_group(sys_id: int, name: str):
        row = cur.execute(
            "SELECT id FROM protocol_groups WHERE name=? AND system_ref=?", (name, sys_id)
        ).fetchone()
        if row:
            return row[0]
        cur.execute("INSERT INTO protocol_groups (system_ref, name) VALUES (?, ?)", (sys_id, name))
        stats["groups"] += 1
        return cur.lastrowid

    def set_group_header(gid, fields):
        cur.execute(
            "UPDATE protocol_groups SET header_enabled=1, header_fields_json=? WHERE id=?",
            (json.dumps(fields, ensure_ascii=False), gid),
        )

    def upsert_dict(group_id, key, name, byte_len, items):
        row = cur.execute(
            'SELECT id FROM dict_sets WHERE "key"=? AND group_ref IS ?',
            (key, group_id),
        ).fetchone()
        if row:
            set_id = row[0]
        else:
            cur.execute(
                'INSERT INTO dict_sets ("key", name, system_ref, group_ref, byte_len, description) '
                "VALUES (?, ?, NULL, ?, ?, ?)",
                (key, name, group_id, byte_len, f"由 {tpl.name} 导入"),
            )
            set_id = cur.lastrowid
        cur.execute("DELETE FROM dict_items WHERE set_ref=?", (set_id,))
        for sort, (value, label) in enumerate(items):
            cur.execute(
                "INSERT INTO dict_items (set_ref, value, label, sort_order) VALUES (?, ?, ?, ?)",
                (set_id, value.upper().replace(" ", ""), label, sort),
            )
        stats["dicts"] += 1

    def upsert_protocol(sys_id, group_id, p):
        pid = p.get("id").strip()
        fields = [field_json(f) for f in p.findall("field")]
        payload = p.find("payload")
        def_mode = "fields" if payload is None else "template"
        fields_json = json.dumps(fields, ensure_ascii=False) if def_mode == "fields" else None
        template = (payload.text or "").strip() if payload is not None else ""
        conds = [cond(c) for m in p.findall("match") for c in m.findall("cond")]
        conds_json = json.dumps(conds, ensure_ascii=False) if conds else None
        cur.execute(
            """INSERT INTO protocols (protocol_id, name, format, msg_type, def_mode, fields_json,
               payload_template, match_rule, match_fields_json, auto_respond, respond_delay_ms,
               auto_periodic, auto_periodic_interval_ms, description,
               system_ref, group_ref)
               VALUES (?, ?, 'hex', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
               ON CONFLICT(protocol_id) DO UPDATE SET
                 name=excluded.name, msg_type=excluded.msg_type, def_mode=excluded.def_mode,
                 fields_json=excluded.fields_json, payload_template=excluded.payload_template,
                 match_rule=excluded.match_rule, match_fields_json=excluded.match_fields_json,
                 auto_respond=excluded.auto_respond, respond_delay_ms=excluded.respond_delay_ms,
                 auto_periodic=excluded.auto_periodic,
                 auto_periodic_interval_ms=excluded.auto_periodic_interval_ms,
                 description=excluded.description, system_ref=excluded.system_ref, group_ref=excluded.group_ref""",
            (
                pid, p.get("name", pid), MSG_TYPE_MAP.get(p.get("type", "Request"), "Request"),
                def_mode, fields_json, template,
                p.get("match") or None, conds_json, p.get("auto-respond") or None,
                int(p.get("delay", "0") or 0),
                p.get("auto-periodic") or None,
                int(p.get("auto-periodic-interval", "0") or 0),
                p.get("description"),
                sys_id, group_id,
            ),
        )
        stats["protocols"] += 1
        print(f"  协议就绪: [{pid}] {p.get('name', pid)}")

    def upsert_flow(sys_id, flow_el):
        name = flow_el.get("name", "").strip()
        if not name:
            print("  跳过: <flow> 缺少 name")
            return
        steps = []
        step_els = flow_el.findall("step")
        for idx, st in enumerate(step_els):
            steps.append({
                "id": st.get("id", ""), "name": st.get("name", ""),
                "x": float(st.get("x", 360)), "y": float(st.get("y", 60 + idx * 175)),
                "protocol_id": st.get("protocol", ""), "terminal_mode": st.get("terminal-mode", "single"),
                "terminal_id": int(st.get("terminal-id")) if st.get("terminal-id") else None,
                "params": {pm.get("name"): pm.get("value") for pm in st.findall("param")},
                "expect_protocol": st.get("expect") or None,
                "timeout_ms": int(st.get("timeout-ms", "3000")), "delay_ms": int(st.get("delay-ms", "0")),
                "next": st.get("next"), "on_success": st.get("on-success"),
                "on_failure": st.get("on-failure"), "on_timeout": st.get("on-timeout"),
            })
        triggers = [t.strip() for t in (flow_el.get("trigger-protocols") or "").split(",") if t.strip()]
        steps_json = json.dumps(steps, ensure_ascii=False)
        trig_json = json.dumps(triggers) if triggers else None
        schedule = int(flow_el.get("schedule-interval-ms", "0"))
        row = cur.execute("SELECT id FROM business_flows WHERE name=? AND system_ref=?", (name, sys_id)).fetchone()
        if row:
            cur.execute("UPDATE business_flows SET description=?,steps_json=?,trigger_protocols_json=?,schedule_interval_ms=? WHERE id=?",
                        (flow_el.get("description"), steps_json, trig_json, schedule, row[0]))
        else:
            cur.execute("INSERT INTO business_flows(name,system_ref,description,steps_json,trigger_protocols_json,schedule_interval_ms) VALUES(?,?,?,?,?,?)",
                        (name, sys_id, flow_el.get("description"), steps_json, trig_json, schedule))
        stats["flows"] = stats.get("flows", 0) + 1
        print(f"  业务流程就绪: [{name}] {len(steps)} 步")

    def handle_system(sys_el):
        name = sys_el.get("name", "").strip()
        desc = sys_el.get("description", "")
        sys_id = resolve_system(name, desc)
        stats["systems"] += 1

        # 兼容旧模板: system 级 header/dict 作为各分组的默认(分组自有配置优先)
        legacy_header = sys_el.find("header")
        legacy_fields = (
            [field_json(f) for f in legacy_header.findall("field")] if legacy_header is not None else None
        )
        legacy_dicts = sys_el.findall("dict")

        for g in sys_el.findall("group"):
            gname = g.get("name", "").strip()
            gid = resolve_group(sys_id, gname) if gname else None
            if gid is None:
                for p in g.findall("protocol"):
                    upsert_protocol(sys_id, None, p)
                continue
            # 分组公共头(严格隔离: 不回落系统级)
            header = g.find("header")
            if header is not None:
                set_group_header(gid, [field_json(f) for f in header.findall("field")])
            elif legacy_fields:
                set_group_header(gid, legacy_fields)
            # 分组专属字典
            for d in g.findall("dict"):
                items = [(i.get("value"), i.get("label")) for i in d.findall("item")]
                upsert_dict(gid, d.get("key"), d.get("name", d.get("key")), int(d.get("byte-len", "1")), items)
            # 兼容旧模板: system 级 dict 拷入该分组
            for d in legacy_dicts:
                items = [(i.get("value"), i.get("label")) for i in d.findall("item")]
                row = cur.execute(
                    'SELECT id FROM dict_sets WHERE "key"=? AND group_ref=?', (d.get("key"), gid)
                ).fetchone()
                if not row:
                    upsert_dict(gid, d.get("key"), d.get("name", d.get("key")), int(d.get("byte-len", "1")), items)
            print(
                f"分组 [{name}/{gname}] 公共头 "
                f"{len(header.findall('field')) if header is not None else (len(legacy_fields) if legacy_fields else 0)} 字段"
            )
            for p in g.findall("protocol"):
                upsert_protocol(sys_id, gid, p)
        for p in sys_el.findall("protocol"):
            upsert_protocol(sys_id, None, p)
        for f in sys_el.findall("flow"):
            upsert_flow(sys_id, f)

    # 全局共享字典 / 系统(无系统归属协议)
    for d in root.findall("dict"):
        items = [(i.get("value"), i.get("label")) for i in d.findall("item")]
        upsert_dict(None, d.get("key"), d.get("name", d.get("key")), int(d.get("byte-len", "1")), items)
    for sys_el in root.findall("system"):
        handle_system(sys_el)
    for p in root.findall("protocol"):
        upsert_protocol(None, None, p)

    con.commit()
    n = cur.execute("SELECT COUNT(*) FROM protocols").fetchone()[0]
    print(
        f"完成: 系统+{stats['systems']} 分组+{stats['groups']} 字典+{stats['dicts']} "
        f"协议+{stats['protocols']} 流程+{stats.get('flows', 0)}, 协议总数 {n}"
    )
    con.close()


if __name__ == "__main__":
    main()
