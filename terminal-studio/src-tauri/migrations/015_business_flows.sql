-- 业务流程: OA 式可编排交互链(步骤图)。引擎按步骤驱动
-- 发送协议(带终端上下文/档案参数) → 期待应答(超时) → 按结果分支路由。
-- 捕获(令牌/会话/字段)沿用 role 语义零配置; 流程随 XML 套件导入导出。
CREATE TABLE business_flows (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL,
  system_ref INTEGER,
  description TEXT,
  -- 步骤图: [{id,name,x,y,protocol_id,terminal_mode(single|each),terminal_id,
  --   params{字段:值},expect_protocol,timeout_ms,delay_ms,
  --   next,on_success,on_failure,on_timeout}]
  steps_json TEXT NOT NULL DEFAULT '[]',
  -- 收到这些协议自动运行: JSON [protocol_id,...]
  trigger_protocols_json TEXT,
  -- 定时触发间隔(毫秒), 0 = 不定时
  schedule_interval_ms INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL DEFAULT (datetime('now','localtime')),
  updated_at TEXT NOT NULL DEFAULT (datetime('now','localtime'))
);
