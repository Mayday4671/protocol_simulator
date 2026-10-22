CREATE TABLE business_record_types (
    system_ref INTEGER NOT NULL REFERENCES protocol_systems(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    definition_json TEXT NOT NULL,
    PRIMARY KEY(system_ref,kind)
);

-- Explicit scope keeps legacy channel-wide keys unchanged. Device-scoped data uses
-- a separate scope key, so identical port numbers on different devices can coexist.
ALTER TABLE business_records RENAME TO business_records_legacy;
CREATE TABLE business_records (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    node_ref INTEGER NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    system_ref INTEGER NOT NULL REFERENCES protocol_systems(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    scope_key TEXT NOT NULL DEFAULT '',
    record_key TEXT NOT NULL,
    name TEXT NOT NULL,
    device_ref INTEGER REFERENCES terminal_profiles(id) ON DELETE CASCADE,
    properties_json TEXT NOT NULL,
    request_ref TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now','localtime')),
    UNIQUE(node_ref,system_ref,kind,scope_key,record_key)
);
INSERT INTO business_records(id,node_ref,system_ref,kind,record_key,name,device_ref,properties_json,request_ref,updated_at)
SELECT id,node_ref,system_ref,kind,record_key,name,device_ref,properties_json,request_ref,updated_at FROM business_records_legacy;
DROP TABLE business_records_legacy;
