-- 触发定时发送: 收到某协议(如注册响应)后自动开始周期发送指定协议(心跳保活)
ALTER TABLE protocols ADD COLUMN auto_periodic TEXT;
ALTER TABLE protocols ADD COLUMN auto_periodic_interval_ms INTEGER NOT NULL DEFAULT 0;
