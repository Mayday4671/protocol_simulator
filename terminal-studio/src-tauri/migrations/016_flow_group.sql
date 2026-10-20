-- 业务流程归属分组: 内嵌信令保存时自动生成的协议挂到该分组
ALTER TABLE business_flows ADD COLUMN group_ref INTEGER;
