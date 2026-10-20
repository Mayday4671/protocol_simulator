"""Generate, stage, or safely replace the verified legacy laboratory dataset.

No implicit production path, no network traffic, no schema changes on install.
Run `python scripts/iot_dataset.py --help` for explicit commands.
"""
import argparse
from contextlib import closing
from datetime import datetime
import hashlib
import json
from pathlib import Path
import sqlite3
import xml.etree.ElementTree as ET

from import_xml import field_json, cond
from iot_seed_data import build_tree

ROOT = Path(__file__).resolve().parents[1]
XML = ROOT / 'protocols' / 'IOT_LAB_2.0.xml'
LEGACY_PROTOCOLS = {
    'dev_cfg_req', 'dev_cfg_data', 'dev_cfg_ack', 'dev_reg', 'dev_reg_resp',
    'dev_call', 'dev_incoming', 'dev_call_ack', 'dev_call_cfm',
    'dev_heartbeat', 'dev_heartbeat_resp', 'demo_branch', 'demo_list',
}
LEGACY_DICTS = {'signal_type': 11, 'resp_result': 2, 'call_result': 2, 'dev_sn': 3,
                'dev_sn_4b': 4, 'verify_err': 60, 'demo_code': 3, 'd_mtqp79bm': 0}
LEGACY_SYSTEMS = {'设备通信协议', '视频会议协议', '设备通信协议1'}
LEGACY_GROUPS = {(system, group) for system in ('设备通信协议', '设备通信协议1')
                 for group in ('接入管理', '呼叫控制', '分支条目演示')} | {('视频会议协议', '网关协议')}
DATA_TABLES = ('protocols', 'dict_items', 'dict_sets', 'protocol_groups', 'protocol_systems')
# Content fingerprint of the specifically inspected legacy installation, 2026-09-08.
# Never auto-refresh this pin during install: same IDs/counts can hide user edits.
REVIEWED_LEGACY_SHA256 = '11476db62df86c4c06527cd5658606c6fc006b8c0912a238c80ccae997670956'


def encode(value):
    return json.dumps(value, ensure_ascii=False, separators=(',', ':'))


def create_empty_database(path):
    path = Path(path).resolve()
    if path.exists():
        raise ValueError(f'Refusing to overwrite existing database: {path}')
    path.parent.mkdir(parents=True, exist_ok=True)
    # Exclusive creation also protects against another process creating the target.
    with path.open('xb'):
        pass
    with closing(sqlite3.connect(path)) as con:
        for migration in sorted((ROOT / 'src-tauri/migrations').glob('*.sql')):
            con.executescript(migration.read_text(encoding='utf-8-sig'))


def verify_target(con, replace_legacy):
    if con.execute('SELECT count(*) FROM terminal_profiles').fetchone()[0]:
        raise ValueError('Terminal profiles now exist; stop and review before replacing protocol families')
    if con.execute('SELECT count(*) FROM heartbeat_bindings').fetchone()[0]:
        raise ValueError('Heartbeat bindings now exist; stop and review before replacing protocols')
    if any(row[0] not in (None, '', '[]') for row in con.execute('SELECT group_bind_json FROM nodes')):
        raise ValueError('Custom channel bindings now exist; stop and review')
    if not replace_legacy:
        if any(con.execute(f'SELECT count(*) FROM {table}').fetchone()[0] for table in DATA_TABLES):
            raise ValueError('Stage target must contain no protocols, systems, groups or dictionaries')
        return
    if {r[0] for r in con.execute('SELECT protocol_id FROM protocols')} != LEGACY_PROTOCOLS:
        raise ValueError('Protocol IDs differ from the explicitly reviewed 13 legacy protocols')
    dictionaries = con.execute('SELECT d.key,d.group_ref,d.system_ref,count(i.id) FROM dict_sets d LEFT JOIN dict_items i ON i.set_ref=d.id GROUP BY d.id').fetchall()
    if len(dictionaries) != 8 or {r[0]: r[3] for r in dictionaries} != LEGACY_DICTS or any(r[1] or r[2] for r in dictionaries):
        raise ValueError('Dictionaries differ from the explicitly reviewed 8 legacy dictionaries')
    if {r[0] for r in con.execute('SELECT name FROM protocol_systems')} != LEGACY_SYSTEMS:
        raise ValueError('Protocol systems have changed; stop and review')
    if set(con.execute('SELECT s.name,g.name FROM protocol_groups g JOIN protocol_systems s ON s.id=g.system_ref')) != LEGACY_GROUPS:
        raise ValueError('Protocol groups have changed; stop and review')
    content = {table: con.execute(f'SELECT * FROM {table} ORDER BY id').fetchall() for table in DATA_TABLES}
    if hashlib.sha256(encode(content).encode('utf-8')).hexdigest() != REVIEWED_LEGACY_SHA256:
        raise ValueError('Existing protocol/dictionary content differs from the reviewed fingerprint; stop and review')


def snapshot(con):
    # Logs may legitimately be appended while the app is open. Configuration and
    # terminal state may not change between the online backup and the write lock.
    return {table: con.execute(f'SELECT * FROM {table} ORDER BY id').fetchall()
            for table in (*DATA_TABLES, 'nodes', 'terminal_profiles', 'heartbeat_bindings')}


def add_dictionary(con, d, group_id):
    did = con.execute('INSERT INTO dict_sets(key,name,group_ref,byte_len) VALUES (?,?,?,?)',
                      (d.get('key'), d.get('name'), group_id, int(d.get('byte-len')))).lastrowid
    con.executemany('INSERT INTO dict_items(set_ref,value,label,sort_order) VALUES (?,?,?,?)',
                    [(did, i.get('value'), i.get('label'), index) for index, i in enumerate(d.findall('item'))])


def import_tree(con, root):
    group_ids = []
    for d in root.findall('dict'):
        add_dictionary(con, d, None)
    for system in root.findall('system'):
        sid = con.execute('INSERT INTO protocol_systems(name,description) VALUES (?,?)',
                          (system.get('name'), system.get('description'))).lastrowid
        for group in system.findall('group'):
            fields = [field_json(f) for f in group.findall('./header/field')]
            gid = con.execute('INSERT INTO protocol_groups(system_ref,name,header_enabled,header_fields_json) VALUES (?,?,1,?)',
                              (sid, group.get('name'), encode(fields))).lastrowid
            group_ids.append(gid)
            for d in group.findall('dict'):
                add_dictionary(con, d, gid)
            for p in group.findall('protocol'):
                con.execute("""INSERT INTO protocols(protocol_id,name,format,msg_type,def_mode,fields_json,
                            payload_template,match_fields_json,auto_respond,respond_delay_ms,
                            auto_periodic_interval_ms,system_ref,group_ref)
                            VALUES (?,?,'hex',?,'fields',?,'',?,?,0,0,?,?)""",
                            (p.get('id'), p.get('name'), p.get('type'), encode([field_json(f) for f in p.findall('field')]),
                             encode([cond(c) for c in p.findall('./match/cond')]), p.get('auto-respond'), sid, gid))
    return group_ids


def validate_xml():
    root = ET.parse(XML).getroot()
    # Prevent editing the XML after verification and accidentally installing an
    # unverified document. Custom imports belong in the application's importer.
    actual = ET.canonicalize(ET.tostring(root, encoding='unicode'), strip_text=True)
    expected = ET.canonicalize(ET.tostring(build_tree().getroot(), encoding='unicode'), strip_text=True)
    if actual != expected:
        raise ValueError('XML differs from the deterministic verified data definition; regenerate and retest')
    return root


def install(path, *, replace_legacy, backup_dir):
    path = Path(path).resolve(strict=True)
    root = validate_xml()
    backup = None
    with closing(sqlite3.connect(path.as_uri() + '?mode=rw', uri=True, timeout=10)) as con:
        con.execute('PRAGMA foreign_keys=ON')
        verify_target(con, replace_legacy)
        if replace_legacy:
            if backup_dir is None:
                raise ValueError('Replacing legacy data requires an explicit backup directory')
            directory = Path(backup_dir).resolve()
            directory.mkdir(parents=True, exist_ok=True)
            backup = directory / f'simterm-before-iot2-{datetime.now():%Y%m%d-%H%M%S-%f}.db'
            with backup.open('xb'):
                pass
            with closing(sqlite3.connect(backup)) as saved:
                con.backup(saved)
                if saved.execute('PRAGMA integrity_check').fetchall() != [('ok',)]:
                    raise ValueError('Backup integrity check failed; no data replaced')
                backed_up = snapshot(saved)
        try:
            con.execute('BEGIN IMMEDIATE')
            verify_target(con, replace_legacy)
            if backup and snapshot(con) != backed_up:
                raise ValueError('Configuration changed during backup; no data replaced, review and retry')
            protected_nodes = con.execute('SELECT id,name,local_ip,local_port,remote_ip,remote_port FROM nodes ORDER BY id').fetchall()
            if replace_legacy:
                for table in DATA_TABLES:
                    ids = [r[0] for r in con.execute(f'SELECT id FROM {table}')]
                    if ids:
                        con.execute(f'DELETE FROM {table} WHERE id IN ({",".join("?" for _ in ids)})', ids)
            group_ids = import_tree(con, root)
            con.execute('UPDATE nodes SET group_bind_json=?', (encode(group_ids),))
            expected = {'protocol_systems': 3, 'protocol_groups': 9, 'protocols': 54,
                        'dict_sets': len(root.findall('.//dict')), 'dict_items': len(root.findall('.//item'))}
            actual = {table: con.execute(f'SELECT count(*) FROM {table}').fetchone()[0] for table in expected}
            if actual != expected or con.execute('PRAGMA foreign_key_check').fetchall():
                raise ValueError('Imported counts or references failed validation')
            if protected_nodes != con.execute('SELECT id,name,local_ip,local_port,remote_ip,remote_port FROM nodes ORDER BY id').fetchall():
                raise ValueError('Channel network configuration changed unexpectedly')
            if con.execute('PRAGMA integrity_check').fetchall() != [('ok',)]:
                raise ValueError('Database integrity check failed; rolling back')
            con.commit()
        except BaseException:
            con.rollback()
            raise
    return {'database': str(path), 'backup': str(backup) if backup else None, **actual}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    commands.add_parser('generate')
    stage = commands.add_parser('stage', help='Create a NEW isolated validation database')
    stage.add_argument('--db', type=Path, required=True)
    replace = commands.add_parser('install', help='Replace only the explicitly verified legacy dataset')
    replace.add_argument('--db', type=Path, required=True)
    replace.add_argument('--replace-legacy', action='store_true', required=True)
    replace.add_argument('--backup-dir', type=Path, required=True)
    args = parser.parse_args()
    if args.command == 'generate':
        tree = build_tree()
        ET.indent(tree, space='  ')
        tree.write(XML, encoding='utf-8', xml_declaration=True)
        print(encode({'xml': str(XML), 'protocols': len(tree.findall('.//protocol')), 'dictionaries': len(tree.findall('.//dict')), 'items': len(tree.findall('.//item'))}))
    else:
        if args.command == 'stage':
            create_empty_database(args.db)
        print(encode(install(args.db, replace_legacy=args.command == 'install', backup_dir=getattr(args, 'backup_dir', None))))


if __name__ == '__main__':
    main()
