use crate::{
    application::{Studio, recover_domain, required},
    archive,
    protocol::compile_xml,
    runtime::FlowDefinition,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

fn remap(value: &mut Value, ids: &BTreeMap<String, String>) {
    match value {
        Value::String(s) => {
            if let Some(id) = ids.get(s) {
                *s = id.clone();
            }
        }
        Value::Object(map) => {
            for v in map.values_mut() {
                remap(v, ids)
            }
        }
        Value::Array(a) => {
            for v in a {
                remap(v, ids)
            }
        }
        _ => {}
    }
}
impl Studio {
    pub fn export_project(&self) -> Result<Value, String> {
        let project = self.project();
        let name = self
            .db
            .projects()?
            .into_iter()
            .find(|p| p.id == project)
            .ok_or("项目不存在")?
            .name;
        let records = self.db.protocols(&project)?;
        let mut entries = BTreeMap::new();
        for p in &records {
            entries.insert(
                format!("protocols/{}.xml", p.id),
                self.db.protocol_source(&p.id)?.into_bytes(),
            );
        }
        let mut payload = json!({"id":project,"name":name,"protocols":records,"domain":*self.domain.lock().unwrap()});
        for kind in [
            "systems",
            "endpoints",
            "flows",
            "flow_versions",
            "reconciliations",
        ] {
            payload[kind] = json!(self.db.entities(&project, kind)?);
        }
        payload["settings"] = json!(self.db.entities(&project, "settings")?);
        payload["deviceEvents"] = json!(self.db.entities(&project, "device_events")?);
        crate::flow_host::redact(&mut payload);
        entries.insert(
            "project.json".into(),
            serde_json::to_vec_pretty(&payload).map_err(|e| e.to_string())?,
        );
        let bytes = archive::pack(&name, &entries)?;
        let dir = self.data_dir.join("export");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let path = dir.join(format!("project-{}.zip", crate::network::now_ms()));
        std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
        Ok(json!({"path":path}))
    }
    pub fn import_protocol(&self, r: &Value) -> Result<Value, String> {
        let path = Path::new(required(r, "path")?);
        if std::fs::metadata(path).map_err(|e| e.to_string())?.len() > 2_000_000 {
            return Err("XML超过2MB限制".into());
        }
        let source = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let schema = compile_xml(&source).map_err(|e| e.to_string())?;
        let record = self.db.save_protocol(
            &self.project(),
            None,
            r["name"].as_str().unwrap_or(&schema.name_zh),
            &source,
            false,
        )?;
        Ok(json!(record))
    }
    pub fn import_project(&self, path: &str) -> Result<Value, String> {
        if std::fs::metadata(path).map_err(|e| e.to_string())?.len() > 20_000_000 {
            return Err("ZIP超过20MB限制".into());
        }
        let (manifest, entries) =
            archive::unpack(&std::fs::read(path).map_err(|e| e.to_string())?)?;
        let mut payload: Value =
            serde_json::from_slice(entries.get("project.json").ok_or("缺少project.json")?)
                .map_err(|e| e.to_string())?;
        let protocols = payload["protocols"]
            .as_array()
            .ok_or("协议清单无效")?
            .clone();
        let old_id = required(&payload, "id")?.to_string();
        let mut sources = BTreeMap::new();
        let mut record_ids = BTreeSet::new();
        let mut versions: BTreeMap<String, BTreeSet<u64>> = BTreeMap::new();
        for record in &protocols {
            let id = required(record, "id")?;
            let base = required(record, "protocolId")?;
            let published = record["published"]
                .as_bool()
                .ok_or("协议published必须是布尔值")?;
            let version = record["version"]
                .as_u64()
                .ok_or("协议version必须是非负整数")?;
            if !record_ids.insert(id.to_owned())
                || !versions.entry(base.into()).or_default().insert(version)
            {
                return Err("协议版本记录重复".into());
            }
            if published != (version > 0) || (!published && id != base) {
                return Err("协议草稿/发布版本标识不一致".into());
            }
            let id = required(record, "id")?;
            let source = String::from_utf8(
                entries
                    .get(&format!("protocols/{id}.xml"))
                    .ok_or("协议文件缺失")?
                    .clone(),
            )
            .map_err(|e| e.to_string())?;
            if record["published"] == true {
                compile_xml(&source).map_err(|e| e.to_string())?;
            }
            sources.insert(id.to_string(), source);
        }
        for numbers in versions.values() {
            if numbers.iter().copied().ne(0..numbers.len() as u64) {
                return Err("协议必须包含草稿和连续的发布版本".into());
            }
        }
        let mut domain: crate::domain::DomainStore =
            serde_json::from_value(payload["domain"].clone()).map_err(|e| e.to_string())?;
        recover_domain(&mut domain);
        let settings = payload.get("settings").cloned().unwrap_or(json!([]));
        for setting in settings.as_array().ok_or("settings清单无效")? {
            setting["heartbeatTimeoutMs"]
                .as_u64()
                .filter(|n| *n == 0 || (1000..=3600000).contains(n))
                .ok_or("心跳超时设置无效")?;
        }
        let device_events = payload.get("deviceEvents").cloned().unwrap_or(json!([]));
        for event in device_events.as_array().ok_or("deviceEvents清单无效")? {
            required(event, "id")?;
            required(event, "deviceId")?;
        }
        for kind in [
            "systems",
            "endpoints",
            "flows",
            "flow_versions",
            "reconciliations",
        ] {
            for value in payload[kind].as_array().ok_or(format!("{kind}清单无效"))? {
                required(
                    value,
                    if kind == "flow_versions" {
                        "version"
                    } else if kind == "reconciliations" {
                        "resourceId"
                    } else {
                        "id"
                    },
                )?;
            }
        }
        for value in payload["flow_versions"].as_array().unwrap() {
            let def: FlowDefinition =
                serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
            def.validate()?;
            if !protocols
                .iter()
                .any(|p| p["id"] == def.protocol["id"] && p["published"] == true)
            {
                return Err("流程引用缺失的协议发布版".into());
            }
        }
        let project = self
            .db
            .create_project(&format!("{}（导入）", manifest.name))?;
        let result = (|| {
            let mut ids = BTreeMap::from([(old_id, project.id.clone())]);
            // Publishing updates the database draft, so restore each original draft last.
            let mut records = protocols.clone();
            records.sort_by_key(|p| {
                (
                    p["protocolId"].as_str().unwrap_or("").to_owned(),
                    p["published"] != true,
                    p["version"].as_u64().unwrap_or(0),
                )
            });
            let mut bases: BTreeMap<String, String> = BTreeMap::new();
            for record in records {
                let id = required(&record, "id")?;
                let base = required(&record, "protocolId")?;
                let saved = self.db.save_protocol(
                    &project.id,
                    bases.get(base).map(String::as_str),
                    record["name"].as_str().unwrap_or("导入协议"),
                    &sources[id],
                    record["published"] == true,
                )?;
                bases.insert(base.into(), saved.protocol_id.clone());
                ids.insert(id.into(), saved.id);
            }
            payload["domain"] = json!(domain);
            remap(&mut payload, &ids);
            for setting in settings.as_array().unwrap() {
                self.db
                    .save_entity(&project.id, "settings", "current", setting)?;
            }
            for event in device_events.as_array().unwrap() {
                self.db
                    .save_entity(&project.id, "device_events", required(event, "id")?, event)?;
            }
            self.db
                .save_entity(&project.id, "domain", "current", &payload["domain"])?;
            for kind in [
                "systems",
                "endpoints",
                "flows",
                "flow_versions",
                "reconciliations",
            ] {
                for value in payload[kind].as_array().unwrap() {
                    let id = if kind == "flow_versions" {
                        value["version"].as_str().unwrap()
                    } else if kind == "reconciliations" {
                        value["resourceId"].as_str().unwrap()
                    } else {
                        value["id"].as_str().unwrap()
                    };
                    self.db.save_entity(&project.id, kind, id, value)?;
                }
            }
            Ok(json!({"id":project.id,"name":project.name,"imported":true}))
        })();
        if result.is_err() {
            self.db.delete_project(&project.id)?;
        }
        result
    }
}
