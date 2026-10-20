CREATE TABLE terminal_profiles (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    node_ref INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    system_ref INTEGER NOT NULL,
    protocol_ref TEXT NOT NULL,
    device_key TEXT NOT NULL,
    fields_json TEXT NOT NULL DEFAULT '{}',
    session_hex TEXT,
    UNIQUE(node_ref, system_ref, device_key)
);
