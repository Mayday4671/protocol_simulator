"""Clear the explicitly selected fake-data database and rebuild a usable IoT workspace."""
import argparse
from contextlib import closing
import json
from pathlib import Path
import sqlite3
from iot_dataset import create_empty_database, encode, import_tree
from iot_seed_data import build_tree, FAMILIES

def device_properties(fields, ordinal):
    values, props = {}, {}
    for f in fields:
        name, key = f['name'], f.get('profile_key')
        if 'when' in f and values.get(f['when']['field']) != f['when']['value']:
            continue
        value = f.get('value', '')
        if name == '业务模式': value = f'{ordinal % 3 + 1:02X}'
        if key == 'zone': value = f'{ordinal + 1:04X}'
        if key == 'device_type': value = f'{ordinal + 1:02X}'
        if key == 'vendor': value = f'{0x1001 + ordinal:04X}'
        if f.get('unit_fields'):
            n = next(int(values[c['name']],16) for c in reversed(fields[:fields.index(f)])
                     if c.get('role') == 'device_count' and c['name'] in values)
            rows = []
            for i in range(n):
                rows.append(''.join(f'{0x01000000 + ordinal*32+i+1:08X}' if u['name']=='设备编号' else u['value'] for u in f['unit_fields']))
            value = ''.join(rows)
        values[name] = value
        if key: props[key] = value
    return props

def reset_dataset(path):
    path = Path(path).resolve(strict=True)
    root = build_tree().getroot()
    with closing(sqlite3.connect(path.as_uri()+'?mode=rw',uri=True,timeout=20)) as con:
        con.execute('PRAGMA foreign_keys=ON')
        con.execute('BEGIN IMMEDIATE')
        try:
            for table in ('logs','heartbeat_bindings','terminal_profiles','dict_items','dict_sets','protocols','protocol_groups','protocol_systems','nodes'):
                con.execute(f'DELETE FROM {table}')
            gids = import_tree(con, root)
            node = con.execute("INSERT INTO nodes(name,role,local_ip,local_port,remote_ip,remote_port,node_id_hex,group_bind_json) VALUES ('设备模拟通道','simulator','127.0.0.1',9001,'127.0.0.1',9002,'01000001',?)",(encode(gids),)).lastrowid
            con.execute("INSERT INTO nodes(name,role,local_ip,local_port,remote_ip,remote_port,node_id_hex,group_bind_json) VALUES ('本地应答通道','target','127.0.0.1',9002,'127.0.0.1',9001,'00000000',?)",(encode(gids),))
            for family_index, family in enumerate(FAMILIES,1):
                prefix, name = family[:2]
                pid = f'iot2_{prefix}_write_req'
                sid, fields = con.execute('SELECT system_ref,fields_json FROM protocols WHERE protocol_id=?',(pid,)).fetchone()
                for i in range(6):
                    props = device_properties(json.loads(fields),i)
                    identity = f'{family_index:02X}0000{i+1:02X}'
                    con.execute('INSERT INTO terminal_profiles(name,node_ref,system_ref,protocol_ref,device_key,fields_json) VALUES (?,?,?,?,?,?)',
                                (f'{name.removesuffix("协议")}设备 {i+1:02}',node,sid,pid,identity,encode(props)))
            if con.execute('PRAGMA foreign_key_check').fetchall(): raise ValueError('Invalid references')
            if con.execute('PRAGMA integrity_check').fetchone()[0] != 'ok': raise ValueError('Database integrity error')
            report = {'database': str(path), 'profiles':con.execute('SELECT count(*) FROM terminal_profiles').fetchone()[0],
                      'protocols':con.execute('SELECT count(*) FROM protocols').fetchone()[0],
                      'dictionaries':con.execute('SELECT count(*) FROM dict_sets').fetchone()[0],
                      'items':con.execute('SELECT count(*) FROM dict_items').fetchone()[0]}
            con.commit()
            return report
        except BaseException:
            con.rollback()
            raise

if __name__ == '__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--db',type=Path,required=True)
    parser.add_argument('--clear',action='store_true',required=True,help='Explicitly clear all previous fake data')
    parser.add_argument('--create',action='store_true',help='Create a new test database first')
    args=parser.parse_args()
    if args.create: create_empty_database(args.db)
    print(encode(reset_dataset(args.db)))
