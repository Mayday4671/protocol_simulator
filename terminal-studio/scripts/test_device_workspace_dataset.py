import json
import sqlite3
import tempfile
import unittest
from contextlib import closing
from pathlib import Path
from iot_dataset import create_empty_database
from device_workspace_dataset import reset_dataset

class DeviceDatasetTests(unittest.TestCase):
    def test_replaces_all_fake_data_with_complete_configured_devices(self):
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / 'test.db'
            create_empty_database(path)
            with closing(sqlite3.connect(path)) as con, con:
                con.execute("INSERT INTO nodes(name,local_port,remote_ip,remote_port) VALUES ('旧节点',1234,'127.0.0.1',1235)")
                con.execute("INSERT INTO logs(direction,local_addr,remote_addr,raw_hex) VALUES ('out','a','b','FF')")
            report = reset_dataset(path)
            self.assertEqual(report['profiles'], 18)
            with closing(sqlite3.connect(path)) as con:
                self.assertEqual(con.execute('SELECT count(*) FROM logs').fetchone()[0],0)
                self.assertEqual(con.execute('SELECT count(*) FROM nodes').fetchone()[0],2)
                rows = con.execute('SELECT device_key,fields_json,session_hex FROM terminal_profiles').fetchall()
                self.assertEqual(len({r[0] for r in rows}),18)
                for _, raw, session in rows:
                    props = json.loads(raw)
                    self.assertIn('zone',props)
                    self.assertIn('device_type',props)
                    self.assertTrue(any(len(v)>50 for v in props.values()))
                    self.assertIsNone(session)
                self.assertEqual(con.execute('PRAGMA foreign_key_check').fetchall(),[])
            self.assertEqual(reset_dataset(path)['profiles'],18)

if __name__ == '__main__':
    unittest.main()
