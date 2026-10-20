-- 字典重构: 扁平的 name→value 常量表升级为两层结构「码表(dict_sets) → 条目(dict_items)」
--
-- 旧结构的问题:
--   1. 一条记录只能表达一个取值, 「设备类型」这类多取值码表只能拆成
--      设备类型_温度 / 设备类型_湿度 …… 平铺在同一张表里, 无法区分归属
--   2. name 全库 UNIQUE + ON CONFLICT DO UPDATE, 不同系统的同名字典会静默互相覆盖
--   3. 字段与字典没有绑定关系, 取值靠自由文本 {字典名}, 长度也不校验
--
-- 新结构:
--   dict_sets   一张码表, 如「设备类型」; system_ref 为 NULL 表示全局共享
--   dict_items  码表下的条目, 值(hex) + 含义, 同码表内条目长度统一为 byte_len
--   字段通过 fields_json 里的 dict 键绑定码表 Key（无需 DDL, 随协议导出到 .md）

CREATE TABLE IF NOT EXISTS dict_sets (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    -- ASCII 标识, 模板中以 {dict:key} 引用; 中文放在 name 上
    key         TEXT    NOT NULL,
    name        TEXT    NOT NULL,
    -- NULL = 全局共享码表; 否则仅在该系统内可见, 并覆盖同 Key 的全局码表
    system_ref  INTEGER REFERENCES protocol_systems(id) ON DELETE CASCADE,
    byte_len    INTEGER NOT NULL DEFAULT 1 CHECK (byte_len BETWEEN 1 AND 64),
    description TEXT,
    created_at  TEXT    NOT NULL DEFAULT (datetime('now', 'localtime'))
);

-- SQLite 中 UNIQUE(key, system_ref) 对 NULL 不生效, 因此拆成两个部分唯一索引:
-- 系统内 Key 唯一, 全局 Key 也唯一; 两者可以同 Key 共存（系统专属覆盖全局）
CREATE UNIQUE INDEX IF NOT EXISTS idx_dict_sets_key_sys
    ON dict_sets (key, system_ref) WHERE system_ref IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_dict_sets_key_global
    ON dict_sets (key) WHERE system_ref IS NULL;

CREATE TABLE IF NOT EXISTS dict_items (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    set_ref    INTEGER NOT NULL REFERENCES dict_sets(id) ON DELETE CASCADE,
    -- 紧凑大写十六进制, 如 '01' / '0A00'
    value      TEXT    NOT NULL,
    label      TEXT    NOT NULL,
    -- 越小越前; 排序最前的一条即 {dict:key} 的默认取值
    sort_order INTEGER NOT NULL DEFAULT 0,
    UNIQUE (set_ref, value)
);
CREATE INDEX IF NOT EXISTS idx_dict_items_set ON dict_items (set_ref);

-- ---- 存量数据迁移 ----
-- 老记录按「字节长度」分组落入 legacy_Nb 码表: 同一码表内条目长度必须一致,
-- 按长度分组既满足约束, 又保留全部原始取值。名称即老字典名, 作为条目含义。
INSERT INTO dict_sets (key, name, system_ref, byte_len, description)
SELECT 'legacy_' || (length(replace(value, ' ', '')) / 2) || 'b',
       '未分类 ' || (length(replace(value, ' ', '')) / 2) || ' 字节（旧字典迁移）',
       NULL,
       length(replace(value, ' ', '')) / 2,
       '由旧版扁平字典自动迁移而来, 建议按业务拆分为独立码表后删除本表'
FROM dictionaries
WHERE length(replace(value, ' ', '')) >= 2
  AND length(replace(value, ' ', '')) % 2 = 0
  AND length(replace(value, ' ', '')) / 2 <= 64
GROUP BY length(replace(value, ' ', '')) / 2;

INSERT OR IGNORE INTO dict_items (set_ref, value, label, sort_order)
SELECT s.id, upper(replace(d.value, ' ', '')), d.name, d.id
FROM dictionaries d
JOIN dict_sets s
  ON s.system_ref IS NULL
 AND s.key = 'legacy_' || (length(replace(d.value, ' ', '')) / 2) || 'b';

-- 老表改名保留, 不直接 DROP: 迁移若有偏差仍可人工找回原始数据
ALTER TABLE dictionaries RENAME TO dictionaries_v005_backup;
