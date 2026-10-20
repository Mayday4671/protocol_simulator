-- 009 分组级作用域 + 字段断言匹配
-- 1) 公共头从系统级下沉到分组级(严格隔离, 不回落系统)
-- 2) 码表作用域改为 全局共享 / 分组专属 两档(系统档废除)
-- 3) 协议新增字段断言匹配条件(match_fields_json)
-- 4) 数据迁移: 系统级公共头/系统级码表 自动拷贝到各分组, 不需要手工重录

-- 1) 分组公共头
ALTER TABLE protocol_groups ADD COLUMN header_enabled INTEGER NOT NULL DEFAULT 0;
ALTER TABLE protocol_groups ADD COLUMN header_fields_json TEXT;

-- 2) 码表分组作用域
ALTER TABLE dict_sets ADD COLUMN group_ref INTEGER;

-- 3) 协议字段断言匹配
ALTER TABLE protocols ADD COLUMN match_fields_json TEXT;

-- 4a) 含未分组协议的系统建「默认分组」归拢(协议表单此后分组必选)
INSERT OR IGNORE INTO protocol_groups (system_ref, name)
SELECT DISTINCT p.system_ref, '默认分组' FROM protocols p
WHERE p.system_ref IS NOT NULL AND p.group_ref IS NULL;
UPDATE protocols SET group_ref = (
  SELECT g.id FROM protocol_groups g
  WHERE g.system_ref = protocols.system_ref AND g.name = '默认分组'
) WHERE system_ref IS NOT NULL AND group_ref IS NULL;

-- 4b) 启用公共头的系统 → 头拷贝到该系统所有分组
UPDATE protocol_groups SET
  header_enabled = 1,
  header_fields_json = s.header_fields_json
FROM protocol_systems s
WHERE protocol_groups.system_ref = s.id
  AND s.header_enabled = 1
  AND s.header_fields_json IS NOT NULL;

-- 4c) 码表唯一性先改为 (key, group_ref), 否则下方两系统同 Key 码表展开到分组时
--     (新行 system_ref 均为 NULL) 会撞上旧的"全局唯一"索引
DROP INDEX IF EXISTS idx_dict_sets_key_sys;
DROP INDEX IF EXISTS idx_dict_sets_key_global;
CREATE UNIQUE INDEX idx_dict_sets_key_group ON dict_sets(key, group_ref) WHERE group_ref IS NOT NULL;

-- 4d) 系统级码表拷贝到该系统所有分组(同 Key 各分组一份)
INSERT INTO dict_sets (key, name, system_ref, group_ref, byte_len, description, created_at)
SELECT d.key, d.name, NULL, g.id, d.byte_len, d.description, d.created_at
FROM dict_sets d
JOIN protocol_groups g ON g.system_ref = d.system_ref
WHERE d.system_ref IS NOT NULL;

INSERT INTO dict_items (set_ref, value, label, sort_order)
SELECT n.id, i.value, i.label, i.sort_order
FROM dict_items i
JOIN dict_sets d ON d.id = i.set_ref AND d.system_ref IS NOT NULL
JOIN dict_sets n ON n.key = d.key AND n.group_ref IS NOT NULL
  AND n.group_ref IN (SELECT g.id FROM protocol_groups g WHERE g.system_ref = d.system_ref);

DELETE FROM dict_items WHERE set_ref IN (SELECT id FROM dict_sets WHERE system_ref IS NOT NULL);
DELETE FROM dict_sets WHERE system_ref IS NOT NULL;

-- 4e) 全局共享档(无分组)按 Key 唯一
CREATE UNIQUE INDEX idx_dict_sets_key_global ON dict_sets(key) WHERE group_ref IS NULL;
