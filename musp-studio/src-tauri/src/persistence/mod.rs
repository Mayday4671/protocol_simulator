use crate::network::now_ms;
use rusqlite::{Connection, params};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{path::Path, sync::Mutex};
use uuid::Uuid;

// SQLite retains TEXT compatibility while new rows use a versioned compressed BLOB.
// Compression happens before the connection lock to keep UI queries responsive.
const PACKET_JSON_PREFIX: &[u8] = b"MSPZ1";
fn compress_packet(value: &Value) -> Result<Vec<u8>> {
    use std::io::Write;
    let json = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    let mut encoder =
        flate2::write::ZlibEncoder::new(PACKET_JSON_PREFIX.to_vec(), flate2::Compression::fast());
    // A single write avoids re-entering the compressor for each tiny JSON token.
    encoder.write_all(&json).map_err(|e| e.to_string())?;
    encoder.finish().map_err(|e| e.to_string())
}
fn packet_decoded(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<Value> {
    use rusqlite::types::ValueRef;
    let value = row.get_ref(index)?;
    let decoded = match value {
        ValueRef::Text(text) => serde_json::from_slice(text).map_err(|e| e.to_string()),
        ValueRef::Blob(bytes) if bytes.starts_with(PACKET_JSON_PREFIX) => {
            serde_json::from_reader(std::io::BufReader::new(flate2::read::ZlibDecoder::new(
                &bytes[PACKET_JSON_PREFIX.len()..],
            )))
            .map_err(|e| e.to_string())
        }
        _ => Err("报文解析树存储格式无效".to_owned()),
    };
    decoded.map_err(|message| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            value.data_type(),
            Box::new(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                message,
            )),
        )
    })
}

type Result<T> = std::result::Result<T, String>;
pub struct Database {
    conn: Mutex<Connection>,
    path: std::path::PathBuf,
}
#[derive(Serialize)]
pub struct Project {
    pub id: String,
    pub name: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolRecord {
    pub id: String,
    pub protocol_id: String,
    pub name: String,
    pub version: u32,
    pub published: bool,
}
pub struct PacketInsert<'a> {
    pub project_id: &'a str,
    pub timestamp: u64,
    pub direction: &'a str,
    pub endpoint_id: &'a str,
    pub remote: &'a str,
    pub protocol_id: &'a str,
    pub msg_code: Option<u8>,
    pub length: usize,
    pub hex: &'a str,
    pub status: &'a str,
    pub decoded: &'a Value,
}

impl Database {
    pub fn delete_project(&self, id: &str) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        for sql in [
            "DELETE FROM packets WHERE project_id=?1",
            "DELETE FROM entities WHERE project_id=?1",
            "DELETE FROM protocol_versions WHERE protocol_id IN (SELECT id FROM protocols WHERE project_id=?1)",
            "DELETE FROM protocols WHERE project_id=?1",
            "DELETE FROM projects WHERE id=?1",
        ] {
            tx.execute(sql, [id]).map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())
    }
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let conn = Connection::open(path).map_err(|e| e.to_string())?;
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        conn.execute_batch(include_str!("../../migrations/0001_init.sql"))
            .map_err(|e| e.to_string())?;
        conn.execute_batch("PRAGMA synchronous=NORMAL; PRAGMA cache_size=-16384;")
            .map_err(|e| e.to_string())?;
        Ok(Self {
            conn: Mutex::new(conn),
            path: path.into(),
        })
    }
    pub fn create_project(&self, name: &str) -> Result<Project> {
        if name.trim().is_empty() || name.len() > 256 {
            return Err("项目名称长度无效".into());
        }
        let p = Project {
            id: Uuid::new_v4().to_string(),
            name: name.into(),
        };
        self.conn
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO projects VALUES(?1,?2,?3)",
                params![p.id, p.name, now_ms()],
            )
            .map_err(|e| e.to_string())?;
        Ok(p)
    }
    pub fn projects(&self) -> Result<Vec<Project>> {
        let conn = self.conn.lock().unwrap();
        let mut q = conn
            .prepare("SELECT id,name FROM projects ORDER BY created_at")
            .map_err(|e| e.to_string())?;
        q.query_map([], |r| {
            Ok(Project {
                id: r.get(0)?,
                name: r.get(1)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
    }
    pub fn save_protocol(
        &self,
        project: &str,
        id: Option<&str>,
        name: &str,
        xml: &str,
        publish: bool,
    ) -> Result<ProtocolRecord> {
        if xml.len() > 2_000_000 {
            return Err("XML超过2MB限制".into());
        }
        let mut conn = self.conn.lock().unwrap();
        let id = if let Some(id) = id {
            conn.query_row("SELECT id FROM protocols WHERE id=?1 AND project_id=?2 UNION ALL SELECT p.id FROM protocol_versions v JOIN protocols p ON p.id=v.protocol_id WHERE v.id=?1 AND p.project_id=?2",params![id,project],|r|r.get::<_,String>(0)).map_err(|_|"协议不属于当前项目".to_string())?
        } else {
            Uuid::new_v4().to_string()
        };
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        tx.execute("INSERT INTO protocols VALUES(?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET name=excluded.name,draft_xml=excluded.draft_xml,updated_at=excluded.updated_at WHERE protocols.project_id=excluded.project_id",params![id,project,name,xml,now_ms()]).map_err(|e|e.to_string())?;
        let record = if publish {
            let version: u32 = tx
                .query_row(
                    "SELECT coalesce(max(version),0)+1 FROM protocol_versions WHERE protocol_id=?1",
                    [&id],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            let version_id = Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO protocol_versions VALUES(?1,?2,?3,?4,?5,?6)",
                params![
                    version_id,
                    id,
                    version,
                    xml,
                    format!("{:x}", Sha256::digest(xml)),
                    now_ms()
                ],
            )
            .map_err(|e| e.to_string())?;
            ProtocolRecord {
                id: version_id,
                protocol_id: id,
                name: name.into(),
                version,
                published: true,
            }
        } else {
            ProtocolRecord {
                id: id.clone(),
                protocol_id: id,
                name: name.into(),
                version: 0,
                published: false,
            }
        };
        tx.commit().map_err(|e| e.to_string())?;
        Ok(record)
    }
    pub fn protocols(&self, project: &str) -> Result<Vec<ProtocolRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut q=conn.prepare("SELECT p.id,p.id,p.name,0,0 FROM protocols p WHERE project_id=?1 UNION ALL SELECT v.id,p.id,p.name,v.version,1 FROM protocol_versions v JOIN protocols p ON p.id=v.protocol_id WHERE p.project_id=?1 ORDER BY 3,4 DESC").map_err(|e|e.to_string())?;
        q.query_map([project], |r| {
            Ok(ProtocolRecord {
                id: r.get(0)?,
                protocol_id: r.get(1)?,
                name: r.get(2)?,
                version: r.get(3)?,
                published: r.get(4)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<std::result::Result<_, _>>()
        .map_err(|e| e.to_string())
    }
    pub fn protocol_source(&self, id: &str) -> Result<String> {
        self.conn.lock().unwrap().query_row("SELECT xml_source FROM protocol_versions WHERE id=?1 UNION ALL SELECT draft_xml FROM protocols WHERE id=?1",[id],|r|r.get(0)).map_err(|e|e.to_string())
    }
    pub fn save_entity(&self, project: &str, kind: &str, id: &str, value: &Value) -> Result<()> {
        use rusqlite::OptionalExtension;
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(|e| e.to_string())?;
        let timestamp = now_ms();
        if kind == "domain" {
            let previous: Option<String> = tx
                .query_row(
                    "SELECT json FROM entities WHERE project_id=?1 AND kind='domain' AND id=?2",
                    params![project, id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            let previous: Value = previous
                .map(|s| serde_json::from_str(&s))
                .transpose()
                .map_err(|e| e.to_string())?
                .unwrap_or(Value::Null);
            let mut ids = std::collections::BTreeSet::new();
            for state in [&previous, value] {
                if let Some(devices) = state["devices"].as_object() {
                    ids.extend(devices.keys().cloned());
                }
            }
            for device_id in ids {
                let before = device_audit_summary(&previous["devices"][&device_id]);
                let after = device_audit_summary(&value["devices"][&device_id]);
                if before != after {
                    let event_id = Uuid::new_v4().to_string();
                    let event = json!({"id":event_id,"deviceId":device_id,"timestamp":timestamp,"before":before,"after":after});
                    tx.execute(
                        "INSERT INTO entities VALUES(?1,'device_events',?2,?3,?4)",
                        params![project, event_id, event.to_string(), timestamp],
                    )
                    .map_err(|e| e.to_string())?;
                }
            }
        }
        tx.execute("INSERT INTO entities VALUES(?1,?2,?3,?4,?5) ON CONFLICT(project_id,kind,id) DO UPDATE SET json=excluded.json,updated_at=excluded.updated_at",params![project,kind,id,value.to_string(),timestamp]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn entities(&self, project: &str, kind: &str) -> Result<Vec<Value>> {
        let conn = self.conn.lock().unwrap();
        let mut q = conn
            .prepare(
                "SELECT json FROM entities WHERE project_id=?1 AND kind=?2 ORDER BY updated_at,rowid",
            )
            .map_err(|e| e.to_string())?;
        let rows = q
            .query_map(params![project, kind], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.map(|r| {
            serde_json::from_str(&r.map_err(|e| e.to_string())?).map_err(|e| e.to_string())
        })
        .collect()
    }
    pub fn delete_entity(&self, project: &str, kind: &str, id: &str) -> Result<()> {
        self.conn
            .lock()
            .unwrap()
            .execute(
                "DELETE FROM entities WHERE project_id=?1 AND kind=?2 AND id=?3",
                params![project, kind, id],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }
    pub fn export_packets(
        &self,
        project: &str,
        format: &str,
        writer: &mut dyn std::io::Write,
    ) -> Result<u64> {
        let conn =
            Connection::open_with_flags(&self.path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
                .map_err(|e| e.to_string())?;
        conn.busy_timeout(std::time::Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        let mut stmt=conn.prepare("SELECT id,event_time,direction,endpoint_id,remote,msg_code,byte_length,hex,status,decoded_json FROM packets WHERE project_id=?1 ORDER BY id").map_err(|e|e.to_string())?;
        let mut rows = stmt.query([project]).map_err(|e| e.to_string())?;
        let mut count = 0;
        if format == "json" {
            writer.write_all(b"[").map_err(|e| e.to_string())?;
        } else if format == "csv" {
            writer
                .write_all(
                    "\u{feff}id,timestamp,direction,endpoint,remote,msgCode,length,hex,status\r\n"
                        .as_bytes(),
                )
                .map_err(|e| e.to_string())?;
        }
        while let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let value = json!({"id":row.get::<_,i64>(0).map_err(|e|e.to_string())?,"timestamp":row.get::<_,u64>(1).map_err(|e|e.to_string())?,"direction":row.get::<_,String>(2).map_err(|e|e.to_string())?,"endpointId":row.get::<_,String>(3).map_err(|e|e.to_string())?,"remote":row.get::<_,String>(4).map_err(|e|e.to_string())?,"msgCode":row.get::<_,Option<u8>>(5).map_err(|e|e.to_string())?,"length":row.get::<_,usize>(6).map_err(|e|e.to_string())?,"hex":row.get::<_,String>(7).map_err(|e|e.to_string())?,"status":row.get::<_,String>(8).map_err(|e|e.to_string())?,"decoded":packet_decoded(row,9).map_err(|e|e.to_string())?});
            if format == "csv" {
                let cells = [
                    "id",
                    "timestamp",
                    "direction",
                    "endpointId",
                    "remote",
                    "msgCode",
                    "length",
                    "hex",
                    "status",
                ]
                .iter()
                .map(|k| {
                    let v = &value[*k];
                    let mut s = v
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| v.to_string());
                    if s.starts_with(['=', '+', '-', '@', '\t', '\r']) {
                        s.insert(0, '\'');
                    }
                    format!("\"{}\"", s.replace('"', "\"\""))
                })
                .collect::<Vec<_>>();
                writeln!(writer, "{}\r", cells.join(",")).map_err(|e| e.to_string())?;
            } else {
                if format == "json" && count > 0 {
                    writer.write_all(b",").map_err(|e| e.to_string())?;
                }
                writeln!(writer, "{value}").map_err(|e| e.to_string())?;
            }
            count += 1;
        }
        if format == "json" {
            writer.write_all(b"]").map_err(|e| e.to_string())?;
        }
        writer.flush().map_err(|e| e.to_string())?;
        Ok(count)
    }
    pub fn insert_packet(&self, p: &PacketInsert<'_>) -> Result<i64> {
        let packed = compress_packet(p.decoded)?;
        let conn = self.conn.lock().unwrap();
        conn.prepare_cached("INSERT INTO packets(project_id,event_time,direction,endpoint_id,remote,protocol_id,msg_code,byte_length,hex,status,decoded_json) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)").map_err(|e|e.to_string())?.execute(params![p.project_id,p.timestamp,p.direction,p.endpoint_id,p.remote,p.protocol_id,p.msg_code,p.length,p.hex,p.status,packed]).map_err(|e|e.to_string())?;
        let id = conn.last_insert_rowid();
        if id % 1000 == 0 {
            conn.execute("DELETE FROM packets WHERE project_id=?1 AND id < coalesce((SELECT id FROM packets WHERE project_id=?1 ORDER BY id DESC LIMIT 1 OFFSET 49999),0)",[p.project_id]).map_err(|e|e.to_string())?;
        }
        Ok(id)
    }
    pub fn packets(&self, project: &str, offset: u64, limit: u64, filter: &str) -> Result<Value> {
        self.packets_filtered(project, offset, limit, filter, "")
    }
    pub fn packets_filtered(
        &self,
        project: &str,
        offset: u64,
        limit: u64,
        filter: &str,
        direction: &str,
    ) -> Result<Value> {
        let conn = self.conn.lock().unwrap();
        let pattern = format!("%{}%", filter.chars().take(128).collect::<String>());
        let total:i64=conn.query_row("SELECT count(*) FROM packets WHERE project_id=?1 AND (hex LIKE ?2 OR remote LIKE ?2 OR direction LIKE ?2 OR status LIKE ?2 OR endpoint_id LIKE ?2) AND (?5='' OR direction=?5)",params![project,pattern,0,0,direction],|r|r.get(0)).map_err(|e|e.to_string())?;
        let mut q=conn.prepare("SELECT id,event_time,direction,endpoint_id,remote,msg_code,byte_length,hex,status,decoded_json FROM packets WHERE project_id=?1 AND (hex LIKE ?2 OR remote LIKE ?2 OR direction LIKE ?2 OR status LIKE ?2 OR endpoint_id LIKE ?2) AND (?5='' OR direction=?5) ORDER BY id DESC LIMIT ?3 OFFSET ?4").map_err(|e|e.to_string())?;
        let items=q.query_map(params![project,pattern,limit.clamp(1,500),offset,direction],|r|Ok(json!({"id":r.get::<_,i64>(0)?.to_string(),"timestamp":r.get::<_,u64>(1)?,"direction":r.get::<_,String>(2)?,"endpointId":r.get::<_,String>(3)?,"remote":r.get::<_,String>(4)?,"msgCode":r.get::<_,Option<u8>>(5)?,"length":r.get::<_,usize>(6)?,"hex":r.get::<_,String>(7)?,"status":r.get::<_,String>(8)?,"decoded":packet_decoded(r,9)?}))).map_err(|e|e.to_string())?.collect::<std::result::Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
        Ok(json!({"items":items,"total":total}))
    }
}

fn device_audit_summary(device: &Value) -> Value {
    if device.is_null() {
        return Value::Null;
    }
    let mut summary = serde_json::Map::new();
    for key in [
        "name",
        "systemId",
        "state",
        "configVersion",
        "registrationValid",
        "identities",
        "activeCalls",
    ] {
        summary.insert(key.into(), device[key].clone());
    }
    Value::Object(summary)
}
