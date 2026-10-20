-- 协议三级分层: 系统(protocol_systems) → 分组(protocols.group_name) → 协议
CREATE TABLE IF NOT EXISTS protocol_systems (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL UNIQUE,
    description TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
);

ALTER TABLE protocols ADD COLUMN system_ref INTEGER REFERENCES protocol_systems(id) ON DELETE SET NULL;
ALTER TABLE protocols ADD COLUMN group_name TEXT;
