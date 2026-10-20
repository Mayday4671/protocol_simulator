-- 多协议 UDP 终端模拟系统 - 初始表结构
-- 节点表 / 协议表 / 心跳绑定表 / 通信日志表

-- 1. 节点表：本地模拟终端 或 远端目标系统
CREATE TABLE IF NOT EXISTS nodes (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT    NOT NULL,                          -- 系统名称
    role        TEXT    NOT NULL DEFAULT 'simulator'
                CHECK (role IN ('simulator', 'target')),   -- 模拟端 / 目标端
    local_ip    TEXT    NOT NULL DEFAULT '0.0.0.0',        -- 绑定的本地 IP
    local_port  INTEGER NOT NULL CHECK (local_port BETWEEN 1 AND 65535),
    remote_ip   TEXT    NOT NULL,                          -- 目标系统 IP
    remote_port INTEGER NOT NULL CHECK (remote_port BETWEEN 1 AND 65535),
    node_id_hex TEXT,                                      -- {node_id} 变量取值, 如 '0A 00 00 01'
    enabled     INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT    NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at  TEXT    NOT NULL DEFAULT (datetime('now', 'localtime')),
    UNIQUE (local_ip, local_port)                          -- 防止本地端口冲突
);

-- 2. 协议表：由 Markdown 导入或 UI 手工维护
CREATE TABLE IF NOT EXISTS protocols (
    id               INTEGER PRIMARY KEY AUTOINCREMENT,
    protocol_id      TEXT    NOT NULL UNIQUE,              -- 如 login_req_01
    name             TEXT    NOT NULL,                     -- 协议名称, 如 入网请求
    format           TEXT    NOT NULL DEFAULT 'hex'
                     CHECK (format IN ('hex', 'text', 'json')),
    msg_type         TEXT    NOT NULL DEFAULT 'Request'
                     CHECK (msg_type IN ('Request', 'Response', 'Heartbeat', 'Event')),
    payload_template TEXT    NOT NULL,                     -- 含 {变量} 的模板原文
    match_rule       TEXT,                                 -- 针对 hex 串的正则, 如 ^FF 00 01
    auto_respond     TEXT,                                 -- 命中后自动回复的 protocol_id
    respond_delay_ms INTEGER NOT NULL DEFAULT 0,           -- 延迟回复毫秒数
    description      TEXT,
    created_at       TEXT NOT NULL DEFAULT (datetime('now', 'localtime')),
    updated_at       TEXT NOT NULL DEFAULT (datetime('now', 'localtime'))
);
CREATE INDEX IF NOT EXISTS idx_protocols_match ON protocols (match_rule);

-- 3. 心跳绑定表：节点 ↔ 心跳协议 ↔ 间隔, 持久化, 重启后可恢复
CREATE TABLE IF NOT EXISTS heartbeat_bindings (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    node_ref     INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    protocol_ref TEXT    NOT NULL REFERENCES protocols(protocol_id) ON DELETE CASCADE,
    interval_ms  INTEGER NOT NULL DEFAULT 30000 CHECK (interval_ms >= 100),
    auto_start   INTEGER NOT NULL DEFAULT 0,               -- 入网成功后自动启动
    UNIQUE (node_ref, protocol_ref)
);

-- 4. 通信日志表
CREATE TABLE IF NOT EXISTS logs (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    ts          TEXT    NOT NULL DEFAULT (datetime('now', 'localtime')),
    direction   TEXT    NOT NULL CHECK (direction IN ('in', 'out')),
    node_ref    INTEGER REFERENCES nodes(id) ON DELETE SET NULL,
    local_addr  TEXT    NOT NULL,
    remote_addr TEXT    NOT NULL,
    protocol_id TEXT,                                      -- 命中的协议, 可为 NULL
    raw_hex     TEXT,                                      -- 十六进制空格分隔
    raw_text    TEXT,                                      -- UTF-8 尽力解码
    parsed      TEXT,                                      -- 解析说明
    status      TEXT    NOT NULL DEFAULT 'ok'              -- ok / sent / error / no-match
);
CREATE INDEX IF NOT EXISTS idx_logs_ts  ON logs (ts DESC);
CREATE INDEX IF NOT EXISTS idx_logs_node ON logs (node_ref);
