"""Acceptance tests for the actual delivered XML, not an unrelated mock protocol."""
import importlib
import hashlib
from contextlib import closing
import sqlite3
import tempfile
import unittest
import xml.etree.ElementTree as ET
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
XML = ROOT / 'protocols' / 'IOT_LAB_2.0.xml'


class DatasetTests(unittest.TestCase):
    def load(self):
        self.assertTrue(XML.exists(), 'New complex IoT dataset has not been generated')
        return ET.parse(XML).getroot()

    def test_scale_and_complete_dictionary_data(self):
        root = self.load()
        self.assertEqual(len(root.findall('system')), 3)
        self.assertEqual(len(root.findall('./system/group')), 9)
        self.assertEqual(len(root.findall('.//protocol')), 54)
        self.assertGreater(sum(len(d.findall('item')) for d in root.findall('.//dict')), 3000)
        for group in root.findall('./system/group'):
            self.assertGreaterEqual(len(group.findall('dict')), 12)
        for d in root.findall('.//dict'):
            values = [i.get('value') for i in d.findall('item')]
            self.assertEqual(len(values), len(set(values)), d.get('name'))
            self.assertTrue(values)
            for item in d.findall('item'):
                self.assertEqual(len(bytes.fromhex(item.get('value'))), int(d.get('byte-len')))
                self.assertTrue(item.get('label'))

    def test_wire_widths_dictionary_scopes_and_prior_conditions(self):
        root = self.load()
        global_dicts = {d.get('key'): d for d in root.findall('dict')}
        for group in root.findall('./system/group'):
            dicts = {**global_dicts, **{d.get('key'): d for d in group.findall('dict')}}
            header = group.findall('./header/field')
            for p in group.findall('protocol'):
                previous = {f.get('name') for f in header}
                for f in [*header, *p.findall('field')]:
                    self.assertEqual(f.get('enc'), 'hex')
                    value = f.get('value', '')
                    if f.get('dict'):
                        d = dicts[f.get('dict')]
                        self.assertEqual(f.get('len'), d.get('byte-len'))
                        if value and '{' not in value:
                            self.assertIn(value, [i.get('value') for i in d.findall('item')])
                    elif value and '{' not in value:
                        self.assertEqual(len(bytes.fromhex(value)), int(f.get('len')), (p.get('id'), f.get('name')))
                    if f.get('when-field'):
                        self.assertIn(f.get('when-field'), previous)
                    previous.add(f.get('name'))
                    if f.findall('unit'):
                        self.assertEqual(int(f.get('len')), sum(int(u.get('len')) for u in f.findall('unit')))
                        for unit in f.findall('unit'):
                            self.assertEqual(unit.get('enc'), 'hex')
                            self.assertFalse(unit.findall('unit') or unit.get('when-field'))
                            self.assertNotIn('{', unit.get('value', ''))
                            self.assertEqual(len(bytes.fromhex(unit.get('value'))), int(unit.get('len')))
                            if unit.get('dict'):
                                self.assertIn(unit.get('dict'), dicts)

    def test_request_response_links_and_branch_list_defaults(self):
        root = self.load()
        protocols = {p.get('id'): p for p in root.findall('.//protocol')}
        for p in protocols.values():
            if p.get('auto-respond'):
                self.assertIn(p.get('auto-respond'), protocols)
                self.assertEqual(protocols[p.get('auto-respond')].get('type'), 'Response')
            if p.get('type') == 'Response':
                self.assertIsNone(p.find("field[@name='接收条目数']"), 'Do not claim a fixed count for variable-size requests')
                self.assertEqual(p.find("field[@name='确认消息码']").get('value'), '{req:消息码}')
            fields = p.findall('field')
            for f in fields:
                if f.get('role') == 'device_count':
                    self.assertGreater(int(f.get('value'), 16), 0)
            if p.get('id').endswith('_write_req'):
                self.assertGreaterEqual(sum(f.get('role') == 'device_list' for f in fields), 4)
                self.assertEqual({f.get('when-value') for f in fields if f.get('when-field') == '业务模式'}, {'01', '02', '03'})

    def test_install_is_safe_and_requires_a_verified_legacy_set(self):
        self.load()
        self.assertTrue((ROOT / 'scripts/iot_dataset.py').exists())
        module = importlib.import_module('iot_dataset')
        with tempfile.TemporaryDirectory(prefix='simterm-iot-tests-') as directory:
            db = Path(directory) / 'test.db'
            module.create_empty_database(db)
            module.install(db, replace_legacy=False, backup_dir=None)
            with closing(sqlite3.connect(db)) as con:
                self.assertEqual(con.execute('SELECT count(*) FROM protocols').fetchone()[0], 54)
                before = con.execute('SELECT count(*) FROM dict_items').fetchone()[0]
            with self.assertRaises(ValueError):
                module.install(db, replace_legacy=True, backup_dir=Path(directory) / 'backups')
            with closing(sqlite3.connect(db)) as con:
                self.assertEqual(con.execute('SELECT count(*) FROM dict_items').fetchone()[0], before)

    def legacy_database(self, module, db):
        module.create_empty_database(db)
        with closing(sqlite3.connect(db)) as con, con:
            for name in module.LEGACY_SYSTEMS:
                con.execute('INSERT INTO protocol_systems(name) VALUES (?)', (name,))
            for system, group in module.LEGACY_GROUPS:
                con.execute('INSERT INTO protocol_groups(system_ref,name) SELECT id,? FROM protocol_systems WHERE name=?', (group, system))
            for pid in module.LEGACY_PROTOCOLS:
                con.execute("INSERT INTO protocols(protocol_id,name,payload_template) VALUES (?,?,'AA')", (pid, pid))
            for key, count in module.LEGACY_DICTS.items():
                did = con.execute('INSERT INTO dict_sets(key,name) VALUES (?,?)', (key, key)).lastrowid
                con.executemany('INSERT INTO dict_items(set_ref,value,label) VALUES (?,?,?)', [(did, f'{i:02X}', str(i)) for i in range(count)])
            con.execute("INSERT INTO nodes(name,local_ip,local_port,remote_ip,remote_port) VALUES ('kept','127.0.0.1',31991,'127.0.0.1',31992)")
            con.execute("INSERT INTO logs(direction,node_ref,local_addr,remote_addr,raw_hex) VALUES ('out',1,'local','remote','AA')")
            reviewed = {table: con.execute(f'SELECT * FROM {table} ORDER BY id').fetchall() for table in module.DATA_TABLES}
            # Freeze THIS reviewed temporary fixture, just as production pins its
            # reviewed DB. Later content edits must not silently refresh the pin.
            pin = patch.object(module, 'REVIEWED_LEGACY_SHA256', hashlib.sha256(module.encode(reviewed).encode('utf-8')).hexdigest(), create=True)
            pin.start()
            self.addCleanup(pin.stop)

    def test_legacy_replacement_backs_up_and_preserves_channels_logs(self):
        self.load()
        module = importlib.import_module('iot_dataset')
        with tempfile.TemporaryDirectory(prefix='simterm-iot-safe-') as directory:
            db = Path(directory) / 'test.db'
            self.legacy_database(module, db)
            report = module.install(db, replace_legacy=True, backup_dir=Path(directory) / 'backups')
            with closing(sqlite3.connect(report['backup'])) as old:
                self.assertEqual(old.execute('SELECT count(*) FROM protocols').fetchone()[0], 13)
                self.assertEqual(old.execute('PRAGMA integrity_check').fetchone()[0], 'ok')
            with closing(sqlite3.connect(db)) as con:
                self.assertEqual(con.execute('SELECT name,local_port,remote_port FROM nodes').fetchone(), ('kept', 31991, 31992))
                self.assertEqual(con.execute('SELECT raw_hex FROM logs').fetchone(), ('AA',))
                self.assertEqual(con.execute('SELECT count(*) FROM protocols').fetchone()[0], 54)
                self.assertEqual(con.execute('PRAGMA foreign_key_check').fetchall(), [])

    def test_new_user_data_aborts_without_deleting_anything(self):
        self.load()
        module = importlib.import_module('iot_dataset')
        additions = (
            "INSERT INTO protocols(protocol_id,name,payload_template) VALUES ('new','user','BB')",
            "INSERT INTO terminal_profiles(name,node_ref,system_ref,protocol_ref,device_key) VALUES ('user',1,1,'dev_reg','0011')",
            "INSERT INTO heartbeat_bindings(node_ref,protocol_ref,interval_ms) VALUES (1,'dev_reg',1000)",
            "INSERT INTO dict_items(set_ref,value,label) SELECT id,'FF','user' FROM dict_sets WHERE key='signal_type'",
            "UPDATE nodes SET group_bind_json='[1]'",
            "UPDATE protocols SET fields_json='[]',name='user edit' WHERE protocol_id='dev_reg'",
            "UPDATE dict_items SET label='user edit' WHERE id=(SELECT min(id) FROM dict_items)",
        )
        for addition in additions:
            with self.subTest(addition=addition), tempfile.TemporaryDirectory(prefix='simterm-iot-abort-') as directory:
                db = Path(directory) / 'test.db'
                self.legacy_database(module, db)
                with closing(sqlite3.connect(db)) as con, con:
                    con.execute(addition)
                    before = con.execute('SELECT protocol_id FROM protocols ORDER BY id').fetchall()
                with self.assertRaises(ValueError):
                    module.install(db, replace_legacy=True, backup_dir=Path(directory) / 'backups')
                with closing(sqlite3.connect(db)) as con:
                    self.assertEqual(con.execute('SELECT protocol_id FROM protocols ORDER BY id').fetchall(), before)

    def test_failed_import_rolls_back_the_entire_replacement(self):
        self.load()
        module = importlib.import_module('iot_dataset')
        with tempfile.TemporaryDirectory(prefix='simterm-iot-rollback-') as directory:
            db = Path(directory) / 'test.db'
            self.legacy_database(module, db)
            with closing(sqlite3.connect(db)) as con, con:
                con.execute("CREATE TRIGGER reject_new BEFORE INSERT ON protocols WHEN NEW.protocol_id LIKE 'iot2_%' BEGIN SELECT RAISE(ABORT,'injected failure'); END")
            with self.assertRaises(sqlite3.IntegrityError):
                module.install(db, replace_legacy=True, backup_dir=Path(directory) / 'backups')
            with closing(sqlite3.connect(db)) as con:
                self.assertEqual({r[0] for r in con.execute('SELECT protocol_id FROM protocols')}, module.LEGACY_PROTOCOLS)
                self.assertEqual(con.execute('SELECT count(*) FROM dict_sets').fetchone()[0], 8)
                self.assertIsNone(con.execute('SELECT group_bind_json FROM nodes').fetchone()[0])


if __name__ == '__main__':
    unittest.main(verbosity=2)
