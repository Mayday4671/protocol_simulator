-- 响应字段捕获(按终端分桶): 节点保存从收包捕获的字段值
-- JSON: { 终端标识(源设备/序列号值, 无则为 "_"): { 字段名: 原始值, "__session": hex } }
-- 一个通道模拟多个终端: 各终端的安全信息/会话/分配值互不干扰, 发送按帧内终端标识取自己的
ALTER TABLE nodes ADD COLUMN captures_json TEXT;
