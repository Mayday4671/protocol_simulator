//! Project archive validation; nothing is extracted to paths supplied by the archive.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{Cursor, Read, Write},
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub format_version: u32,
    pub product_version: String,
    pub name: String,
    pub checksums: BTreeMap<String, String>,
}
pub fn pack(name: &str, entries: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>, String> {
    let manifest = Manifest {
        format_version: 1,
        product_version: env!("CARGO_PKG_VERSION").into(),
        name: name.into(),
        checksums: entries
            .iter()
            .map(|(name, bytes)| (name.clone(), format!("{:x}", Sha256::digest(bytes))))
            .collect(),
    };
    let cursor = Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(cursor);
    let opts = zip::write::SimpleFileOptions::default();
    writer
        .start_file("manifest.json", opts)
        .map_err(|e| e.to_string())?;
    writer
        .write_all(&serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    for (path, bytes) in entries {
        safe_path(path)?;
        writer.start_file(path, opts).map_err(|e| e.to_string())?;
        writer.write_all(bytes).map_err(|e| e.to_string())?;
    }
    Ok(writer.finish().map_err(|e| e.to_string())?.into_inner())
}
pub fn unpack(bytes: &[u8]) -> Result<(Manifest, BTreeMap<String, Vec<u8>>), String> {
    if bytes.len() > 20_000_000 {
        return Err("ZIP压缩文件超过20MB限制".into());
    }
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    if zip.len() > 2048 {
        return Err("ZIP文件数超过2048限制".into());
    }
    let mut entries = BTreeMap::new();
    let mut total = 0;
    for i in 0..zip.len() {
        let file = zip.by_index(i).map_err(|e| e.to_string())?;
        let name = file.name().to_string();
        safe_path(&name)?;
        if file.is_dir() {
            continue;
        }
        if file.size() > 2_000_000 {
            return Err("单个项目文件超过2MB限制".into());
        }
        let mut data = Vec::new();
        file.take(2_000_001)
            .read_to_end(&mut data)
            .map_err(|e| e.to_string())?;
        total += data.len();
        if data.len() > 2_000_000 || total > 50_000_000 {
            return Err("ZIP展开超出安全上限".into());
        }
        if entries.insert(name, data).is_some() {
            return Err("ZIP包含重复路径".into());
        }
    }
    let manifest: Manifest =
        serde_json::from_slice(&entries.remove("manifest.json").ok_or("缺少manifest.json")?)
            .map_err(|e| e.to_string())?;
    if manifest.format_version != 1 {
        return Err("不支持的项目格式版本".into());
    }
    if entries.len() != manifest.checksums.len() {
        return Err("项目清单与文件数量不一致".into());
    }
    for (name, data) in &entries {
        let actual = format!("{:x}", Sha256::digest(data));
        if manifest.checksums.get(name) != Some(&actual) {
            return Err(format!("项目文件校验失败: {name}"));
        }
    }
    Ok((manifest, entries))
}
fn safe_path(path: &str) -> Result<(), String> {
    if path.starts_with(['/', '\\'])
        || path.contains('\\')
        || path.contains(':')
        || path.split('/').any(|p| p == ".." || p == ".")
    {
        return Err("ZIP包含不安全路径".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn archive_roundtrip_checksums_and_no_traversal() {
        let entries = BTreeMap::from([("protocols/example.xml".into(), b"<protocol/>".to_vec())]);
        let bytes = pack("测试项目", &entries).unwrap();
        let (m, out) = unpack(&bytes).unwrap();
        assert_eq!(m.name, "测试项目");
        assert_eq!(entries, out);
        assert!(pack("invalid", &BTreeMap::from([("../escape".into(), vec![])])).is_err());
        assert!(safe_path("C:/Windows/example").is_err());
    }
    #[test]
    fn tampered_manifest_is_rejected() {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default();
        zip.start_file("manifest.json", opts).unwrap();
        zip.write_all(br#"{"formatVersion":1,"productVersion":"0.1.0","name":"bad","checksums":{"devices/data.json":"wrong"}}"#).unwrap();
        zip.start_file("devices/data.json", opts).unwrap();
        zip.write_all(b"[]").unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        assert!(unpack(&bytes).unwrap_err().contains("校验失败"));
    }
}
