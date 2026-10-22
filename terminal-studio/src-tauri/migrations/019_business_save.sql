CREATE TABLE business_save_rules (
    request_ref TEXT PRIMARY KEY REFERENCES protocols(protocol_id) ON DELETE CASCADE,
    response_ref TEXT NOT NULL REFERENCES protocols(protocol_id) ON DELETE CASCADE,
    rule_json TEXT NOT NULL
);
CREATE TABLE business_records (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    node_ref INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    system_ref INTEGER NOT NULL REFERENCES protocol_systems(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    record_key TEXT NOT NULL,
    name TEXT NOT NULL,
    device_ref INTEGER REFERENCES terminal_profiles(id) ON DELETE CASCADE,
    properties_json TEXT NOT NULL,
    request_ref TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now','localtime')),
    UNIQUE(node_ref,system_ref,kind,record_key)
);
