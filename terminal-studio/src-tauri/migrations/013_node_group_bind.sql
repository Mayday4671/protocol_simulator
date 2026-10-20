-- 通道 ↔ 协议分组 绑定(多选, JSON 数组): 交互控制台按所选分组自动定位通道, 不再手选
ALTER TABLE nodes ADD COLUMN group_bind_json TEXT;
