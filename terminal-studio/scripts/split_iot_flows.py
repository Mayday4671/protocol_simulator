"""Replace only the generated IoT walkthrough with six independent exchanges.

Uses an online backup and one transaction; no protocol/channel/device rows change.
"""
import argparse
import json
import sqlite3
from pathlib import Path
from seed_iot_walkthrough import OPS, PREFIX, SYSTEM, insert_exchanges


def split(db, backup):
    db, backup = Path(db).resolve(), Path(backup).resolve()
    if not db.is_file() or backup.exists() or db == backup:
        raise ValueError('Database must exist and backup must be a new path')
    backup.parent.mkdir(parents=True, exist_ok=True)
    with sqlite3.connect(db, timeout=10) as con:
        con.execute('PRAGMA foreign_keys=ON')
        with sqlite3.connect(backup) as dest:
            con.backup(dest)
        con.execute('BEGIN IMMEDIATE')
        rows = con.execute('''SELECT f.id,f.system_ref,f.group_ref,f.steps_json
            FROM business_flows f JOIN protocol_systems s ON s.id=f.system_ref
            WHERE s.name=? AND f.name=?''',
            (SYSTEM, '物联网设备交互 · 注册到注销')).fetchall()
        if len(rows) != 1:
            raise ValueError('Expected exactly one original generated walkthrough; no changes made')
        old_id, sid, gid, raw = rows[0]
        steps = json.loads(raw)
        expected = [(PREFIX+'_'+op+'_req', PREFIX+'_'+op+'_resp') for op, _, _ in OPS]
        if [(s.get('protocol_id'), s.get('expect_protocol')) for s in steps] != expected:
            raise ValueError('Walkthrough has been customized; refusing to replace it')
        for req, resp in expected:
            if con.execute('SELECT COUNT(*) FROM protocols WHERE protocol_id IN (?,?) AND system_ref=?',
                           (req, resp, sid)).fetchone()[0] != 2:
                raise ValueError('Missing request or response definition')
        ids = insert_exchanges(con, sid, gid, steps)
        con.execute('DELETE FROM business_flows WHERE id=?', (old_id,))
        if con.execute('PRAGMA foreign_key_check').fetchall():
            raise ValueError('Foreign key validation failed')
        return dict(removed_flow_id=old_id, flow_ids=ids, backup=str(backup))


if __name__ == '__main__':
    p = argparse.ArgumentParser()
    p.add_argument('--db', required=True)
    p.add_argument('--backup', required=True)
    a = p.parse_args()
    print(json.dumps(split(a.db, a.backup), ensure_ascii=False))
