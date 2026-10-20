import sqlite3
import unittest
from pathlib import Path

MIGRATIONS = Path(__file__).resolve().parents[1] / 'src-tauri' / 'migrations'

class SystemScopeTests(unittest.TestCase):
    def setUp(self):
        self.db = sqlite3.connect(':memory:')
        self.db.execute('PRAGMA foreign_keys=ON')
        for path in sorted(MIGRATIONS.glob('*.sql')):
            if not path.name.startswith('017_'):
                self.db.executescript(path.read_text(encoding='utf-8-sig'))
        self.db.execute("INSERT INTO protocols(protocol_id,name,payload_template) VALUES('legacy','Legacy','')")
        self.db.execute("INSERT INTO business_flows(name,system_ref) VALUES('legacy flow',999)")
        migration = MIGRATIONS / '017_required_system_scope.sql'
        if migration.exists():
            self.db.executescript(migration.read_text(encoding='utf-8'))

    def test_migrates_without_losing_data(self):
        self.assertEqual(self.db.execute('SELECT COUNT(*) FROM protocols').fetchone()[0], 1)
        for table in ('protocols', 'business_flows'):
            self.assertEqual(self.db.execute(f'SELECT s.name FROM {table} p JOIN protocol_systems s ON s.id=p.system_ref').fetchone(), ('迁移系统',))

    def test_rejects_orphan_and_system_delete(self):
        for statement in (
            "INSERT INTO protocols(protocol_id,name,payload_template) VALUES('new','New','')",
            "INSERT INTO business_flows(name,system_ref) VALUES('new',999)",
            "UPDATE protocols SET system_ref=NULL",
            "DELETE FROM protocol_systems WHERE name='迁移系统'",
        ):
            with self.assertRaises(sqlite3.IntegrityError, msg=statement):
                self.db.execute(statement)

    def test_lost_dictionary_scope_is_preserved(self):
        db = sqlite3.connect(':memory:')
        db.execute('PRAGMA foreign_keys=ON')
        for path in sorted(MIGRATIONS.glob('*.sql')):
            if not path.name.startswith('017_'):
                db.executescript(path.read_text(encoding='utf-8-sig'))
        for group in (888, 999):
            ref = db.execute("INSERT INTO dict_sets(key,name,byte_len,group_ref) VALUES('same-key','Lost',1,?)", (group,)).lastrowid
            db.execute("INSERT INTO dict_items(set_ref,value,label) VALUES(?,'01','Preserved')", (ref,))
        db.executescript((MIGRATIONS / '017_required_system_scope.sql').read_text(encoding='utf-8'))
        self.assertEqual(db.execute("SELECT COUNT(*) FROM dict_sets d JOIN protocol_groups g ON g.id=d.group_ref JOIN protocol_systems s ON s.id=g.system_ref WHERE d.key='same-key' AND s.name='迁移系统'").fetchone()[0], 2)
        self.assertEqual(db.execute("SELECT COUNT(*) FROM dict_items WHERE label='Preserved'").fetchone()[0], 2)

    def test_device_orphans_and_legacy_dictionary_systems(self):
        db = sqlite3.connect(':memory:')
        for path in sorted(MIGRATIONS.glob('*.sql')):
            if not path.name.startswith('017_'):
                db.executescript(path.read_text(encoding='utf-8-sig'))
        node = db.execute("INSERT INTO nodes(name,local_port,remote_ip,remote_port) VALUES('Node',9000,'127.0.0.1',9001)").lastrowid
        for system in (888, 999):
            db.execute("INSERT INTO terminal_profiles(name,node_ref,system_ref,protocol_ref,device_key,session_hex) VALUES('Device',?,?,'legacy','same-key','CAFE')", (node,system))
        db.execute("INSERT INTO dict_sets(key,name,byte_len,system_ref) VALUES('legacy-system','Legacy',1,777)")
        db.executescript((MIGRATIONS / '017_required_system_scope.sql').read_text(encoding='utf-8'))
        self.assertEqual(db.execute("SELECT COUNT(*) FROM terminal_profiles t JOIN protocol_systems s ON t.system_ref=s.id WHERE t.device_key='same-key' AND t.session_hex='CAFE'").fetchone()[0], 2)
        self.assertEqual(db.execute("SELECT COUNT(*) FROM dict_sets d JOIN protocol_systems s ON d.system_ref=s.id WHERE d.key='legacy-system'").fetchone()[0], 1)
        for statement in (
            "UPDATE terminal_profiles SET system_ref=12345",
            "INSERT INTO terminal_profiles(name,node_ref,system_ref,protocol_ref,device_key) VALUES('bad',1,12345,'p','new')",
            "UPDATE dict_sets SET system_ref=12345 WHERE key='legacy-system'",
        ):
            with self.assertRaises(sqlite3.IntegrityError, msg=statement):
                db.execute(statement)

    def test_group_consistency_and_shared_dictionaries(self):
        self.db.execute("INSERT INTO protocol_systems(name) VALUES('A'),('B')")
        a, b = [r[0] for r in self.db.execute("SELECT id FROM protocol_systems WHERE name IN ('A','B') ORDER BY name")]
        g = self.db.execute("INSERT INTO protocol_groups(system_ref,name) VALUES(?,'G')", (a,)).lastrowid
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("UPDATE protocols SET system_ref=?,group_ref=?", (b,g))
        self.db.execute("UPDATE protocols SET system_ref=?,group_ref=?", (a,g))
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute('UPDATE protocol_groups SET system_ref=? WHERE id=?', (b,g))
        self.db.execute("INSERT INTO dict_sets(key,name,byte_len) VALUES('shared-test','Shared',1)")
        with self.assertRaises(sqlite3.IntegrityError):
            self.db.execute("INSERT INTO dict_sets(key,name,byte_len,group_ref) VALUES('bad','Bad',1,999)")

if __name__ == '__main__':
    unittest.main()
