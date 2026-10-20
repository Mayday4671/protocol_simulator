-- 协议字段级定义支持: 双模式
--   template: 原始 Hex 模板字符串 (payload_template)
--   fields:   结构化字段表 (fields_json), 每字段含 名称/长度/编码/值
ALTER TABLE protocols ADD COLUMN def_mode TEXT NOT NULL DEFAULT 'template'
    CHECK (def_mode IN ('template', 'fields'));
ALTER TABLE protocols ADD COLUMN fields_json TEXT;
