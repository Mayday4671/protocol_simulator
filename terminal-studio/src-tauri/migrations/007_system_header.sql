-- 系统级公共 UDP 头: 对接系统的报文在协议字段前统一拼接的头部
--   header_enabled      该系统是否启用公共头 (0/1)
--   header_fields_json  公共头字段表 (ProtocolField 数组, 复用协议字段结构含码表绑定)
ALTER TABLE protocol_systems ADD COLUMN header_enabled INTEGER NOT NULL DEFAULT 0;
ALTER TABLE protocol_systems ADD COLUMN header_fields_json TEXT;
