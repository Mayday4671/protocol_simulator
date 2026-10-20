-- 值字典: 常用取值集中维护, 协议字段值可引用 {字典名}
CREATE TABLE IF NOT EXISTS dictionaries (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL UNIQUE,
    value       TEXT NOT NULL,
    description TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
);
