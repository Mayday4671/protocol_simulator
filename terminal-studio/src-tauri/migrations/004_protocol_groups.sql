-- 分组升级为一等实体: 每个系统下的分组独立管理, 协议通过 group_ref 关联
CREATE TABLE IF NOT EXISTS protocol_groups (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    system_ref  INTEGER NOT NULL REFERENCES protocol_systems(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    UNIQUE (system_ref, name)
);

ALTER TABLE protocols ADD COLUMN group_ref INTEGER REFERENCES protocol_groups(id) ON DELETE SET NULL;

-- 存量 group_name 数据迁移到分组表
INSERT OR IGNORE INTO protocol_groups (system_ref, name)
SELECT DISTINCT system_ref, trim(group_name) FROM protocols
WHERE system_ref IS NOT NULL AND group_name IS NOT NULL AND trim(group_name) != '';

UPDATE protocols SET group_ref = (
    SELECT g.id FROM protocol_groups g
    WHERE g.system_ref = protocols.system_ref AND g.name = trim(protocols.group_name)
) WHERE group_name IS NOT NULL AND trim(group_name) != '';
