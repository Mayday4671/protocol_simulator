-- 节点认证会话: 注册应答返回的 Session 存入节点, 后续发送经 {session} 变量自动携带
ALTER TABLE nodes ADD COLUMN session_hex TEXT;
