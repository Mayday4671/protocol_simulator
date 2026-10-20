-- Restore missing device/legacy-dictionary system IDs as editable entities.
-- Keeping IDs preserves device uniqueness and node capture buckets keyed by system ID.
INSERT INTO protocol_systems(id, name, description)
SELECT missing.id, '迁移系统（原系统 ' || missing.id || '，' || lower(hex(randomblob(4))) || '）',
       '历史设备或字典归属迁移，保留原系统编号及设备会话'
FROM (
 SELECT system_ref AS id FROM terminal_profiles
 UNION SELECT system_ref AS id FROM dict_sets WHERE system_ref IS NOT NULL
) missing
WHERE missing.id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM protocol_systems WHERE id=missing.id);

-- Preserve historical records under an ordinary, editable system entity.
INSERT OR IGNORE INTO protocol_systems(name, description)
SELECT '迁移系统', '历史数据迁移归属，可编辑或将数据移动到其他系统'
WHERE EXISTS (SELECT 1 FROM protocols WHERE system_ref IS NULL OR system_ref NOT IN (SELECT id FROM protocol_systems))
 OR EXISTS (SELECT 1 FROM business_flows WHERE system_ref IS NULL OR system_ref NOT IN (SELECT id FROM protocol_systems))
 OR EXISTS (SELECT 1 FROM protocol_groups WHERE system_ref NOT IN (SELECT id FROM protocol_systems))
 OR EXISTS (SELECT 1 FROM dict_sets WHERE group_ref IS NOT NULL AND group_ref NOT IN (SELECT id FROM protocol_groups));
UPDATE protocol_groups SET name=name || '（迁移 ' || id || '）', system_ref=(SELECT id FROM protocol_systems WHERE name='迁移系统')
WHERE system_ref NOT IN (SELECT id FROM protocol_systems);
-- Keep each lost dictionary scope separate to preserve duplicate keys and all items.
INSERT INTO protocol_groups(system_ref,name)
SELECT (SELECT id FROM protocol_systems WHERE name='迁移系统'), '迁移字典 ' || id
FROM dict_sets WHERE group_ref IS NOT NULL AND group_ref NOT IN (SELECT id FROM protocol_groups);
UPDATE dict_sets SET group_ref=(SELECT id FROM protocol_groups WHERE name='迁移字典 ' || dict_sets.id AND system_ref=(SELECT id FROM protocol_systems WHERE name='迁移系统'))
WHERE group_ref IS NOT NULL AND group_ref NOT IN (SELECT id FROM protocol_groups);
UPDATE protocols SET group_ref=NULL WHERE group_ref IS NOT NULL AND group_ref NOT IN (SELECT id FROM protocol_groups);
UPDATE protocols SET system_ref=(SELECT system_ref FROM protocol_groups WHERE id=protocols.group_ref) WHERE group_ref IS NOT NULL;
UPDATE protocols SET system_ref=(SELECT id FROM protocol_systems WHERE name='迁移系统') WHERE system_ref IS NULL OR system_ref NOT IN (SELECT id FROM protocol_systems);
CREATE TRIGGER protocols_scope_insert BEFORE INSERT ON protocols
BEGIN
 SELECT RAISE(ABORT, '请选择有效的归属系统') WHERE NEW.system_ref IS NULL OR NOT EXISTS (SELECT 1 FROM protocol_systems WHERE id=NEW.system_ref);
 SELECT RAISE(ABORT, '分组必须属于所选系统') WHERE NEW.group_ref IS NOT NULL AND NOT EXISTS (SELECT 1 FROM protocol_groups WHERE id=NEW.group_ref AND system_ref=NEW.system_ref);
END;
CREATE TRIGGER protocols_scope_update BEFORE UPDATE ON protocols
BEGIN
 SELECT RAISE(ABORT, '请选择有效的归属系统') WHERE NEW.system_ref IS NULL OR NOT EXISTS (SELECT 1 FROM protocol_systems WHERE id=NEW.system_ref);
 SELECT RAISE(ABORT, '分组必须属于所选系统') WHERE NEW.group_ref IS NOT NULL AND NOT EXISTS (SELECT 1 FROM protocol_groups WHERE id=NEW.group_ref AND system_ref=NEW.system_ref);
END;
UPDATE business_flows SET group_ref=NULL WHERE group_ref IS NOT NULL AND group_ref NOT IN (SELECT id FROM protocol_groups);
UPDATE business_flows SET system_ref=(SELECT system_ref FROM protocol_groups WHERE id=business_flows.group_ref) WHERE group_ref IS NOT NULL;
UPDATE business_flows SET system_ref=(SELECT id FROM protocol_systems WHERE name='迁移系统') WHERE system_ref IS NULL OR system_ref NOT IN (SELECT id FROM protocol_systems);
CREATE TRIGGER business_flows_scope_insert BEFORE INSERT ON business_flows
BEGIN
 SELECT RAISE(ABORT, '请选择有效的归属系统') WHERE NEW.system_ref IS NULL OR NOT EXISTS (SELECT 1 FROM protocol_systems WHERE id=NEW.system_ref);
 SELECT RAISE(ABORT, '分组必须属于所选系统') WHERE NEW.group_ref IS NOT NULL AND NOT EXISTS (SELECT 1 FROM protocol_groups WHERE id=NEW.group_ref AND system_ref=NEW.system_ref);
END;
CREATE TRIGGER business_flows_scope_update BEFORE UPDATE ON business_flows
BEGIN
 SELECT RAISE(ABORT, '请选择有效的归属系统') WHERE NEW.system_ref IS NULL OR NOT EXISTS (SELECT 1 FROM protocol_systems WHERE id=NEW.system_ref);
 SELECT RAISE(ABORT, '分组必须属于所选系统') WHERE NEW.group_ref IS NOT NULL AND NOT EXISTS (SELECT 1 FROM protocol_groups WHERE id=NEW.group_ref AND system_ref=NEW.system_ref);
END;
UPDATE dict_sets SET system_ref=(SELECT system_ref FROM protocol_groups WHERE id=dict_sets.group_ref)
WHERE system_ref IS NOT NULL AND group_ref IS NOT NULL;
CREATE TRIGGER dict_scope_insert BEFORE INSERT ON dict_sets
BEGIN
 SELECT RAISE(ABORT, '请选择有效的字典系统') WHERE NEW.system_ref IS NOT NULL AND NOT EXISTS (SELECT 1 FROM protocol_systems WHERE id=NEW.system_ref);
 SELECT RAISE(ABORT, '字典分组必须属于所选系统') WHERE NEW.system_ref IS NOT NULL AND NEW.group_ref IS NOT NULL AND NOT EXISTS (SELECT 1 FROM protocol_groups WHERE id=NEW.group_ref AND system_ref=NEW.system_ref);
 SELECT RAISE(ABORT, '请选择有效的字典分组') WHERE NEW.group_ref IS NOT NULL AND NOT EXISTS (SELECT 1 FROM protocol_groups WHERE id=NEW.group_ref);
END;
CREATE TRIGGER dict_scope_update BEFORE UPDATE ON dict_sets
BEGIN
 SELECT RAISE(ABORT, '请选择有效的字典系统') WHERE NEW.system_ref IS NOT NULL AND NOT EXISTS (SELECT 1 FROM protocol_systems WHERE id=NEW.system_ref);
 SELECT RAISE(ABORT, '字典分组必须属于所选系统') WHERE NEW.system_ref IS NOT NULL AND NEW.group_ref IS NOT NULL AND NOT EXISTS (SELECT 1 FROM protocol_groups WHERE id=NEW.group_ref AND system_ref=NEW.system_ref);
 SELECT RAISE(ABORT, '请选择有效的字典分组') WHERE NEW.group_ref IS NOT NULL AND NOT EXISTS (SELECT 1 FROM protocol_groups WHERE id=NEW.group_ref);
END;
CREATE TRIGGER system_delete_restrict BEFORE DELETE ON protocol_systems
BEGIN
 SELECT RAISE(ABORT, '系统仍包含数据，请先移动或删除所属数据') WHERE
 EXISTS (SELECT 1 FROM protocols WHERE system_ref=OLD.id) OR
 EXISTS (SELECT 1 FROM protocol_groups WHERE system_ref=OLD.id) OR
 EXISTS (SELECT 1 FROM business_flows WHERE system_ref=OLD.id) OR
 EXISTS (SELECT 1 FROM terminal_profiles WHERE system_ref=OLD.id) OR
 EXISTS (SELECT 1 FROM dict_sets WHERE system_ref=OLD.id);
END;
CREATE TRIGGER group_move_restrict BEFORE UPDATE OF system_ref ON protocol_groups
WHEN NEW.system_ref != OLD.system_ref
BEGIN
 SELECT RAISE(ABORT, '分组仍包含协议或流程，不能更改归属系统') WHERE
 EXISTS (SELECT 1 FROM protocols WHERE group_ref=OLD.id) OR EXISTS (SELECT 1 FROM business_flows WHERE group_ref=OLD.id);
END;
CREATE TRIGGER group_delete_restrict BEFORE DELETE ON protocol_groups
BEGIN
 SELECT RAISE(ABORT, '分组仍包含字典或流程，请先移动或删除所属数据') WHERE
 EXISTS (SELECT 1 FROM dict_sets WHERE group_ref=OLD.id) OR EXISTS (SELECT 1 FROM business_flows WHERE group_ref=OLD.id);
END;
CREATE TRIGGER terminal_profiles_scope_insert BEFORE INSERT ON terminal_profiles
BEGIN
 SELECT RAISE(ABORT, '请选择有效的设备归属系统') WHERE NEW.system_ref IS NULL OR NOT EXISTS (SELECT 1 FROM protocol_systems WHERE id=NEW.system_ref);
END;
CREATE TRIGGER terminal_profiles_scope_update BEFORE UPDATE ON terminal_profiles
BEGIN
 SELECT RAISE(ABORT, '请选择有效的设备归属系统') WHERE NEW.system_ref IS NULL OR NOT EXISTS (SELECT 1 FROM protocol_systems WHERE id=NEW.system_ref);
END;
