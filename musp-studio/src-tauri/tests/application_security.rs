use musp_studio::{
    application::{DEMO_XML, Studio},
    archive,
    protocol::{format_hex, parse_hex},
};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{Cursor, Write},
    sync::Arc,
    time::Duration,
};

fn application() -> (tempfile::TempDir, Arc<Studio>) {
    let dir = tempfile::tempdir().unwrap();
    let (app, _) = Studio::open(dir.path()).unwrap();
    (dir, app)
}

#[test]
fn project_roundtrip_preserves_draft_and_each_immutable_protocol_source() {
    let (_dir, app) = application();
    let original = app.project();
    let first = app
        .db
        .save_protocol(&original, None, "版本隔离协议", DEMO_XML, true)
        .unwrap();
    let second_source = DEMO_XML.replace("version=\"0.8.0\"", "version=\"0.9.0\"");
    let second = app
        .db
        .save_protocol(
            &original,
            Some(&first.id),
            "版本隔离协议",
            &second_source,
            true,
        )
        .unwrap();
    let draft_source = "<protocol id=\"unfinished\" version=\"draft\">";
    app.db
        .save_protocol(
            &original,
            Some(&first.id),
            "版本隔离协议",
            draft_source,
            false,
        )
        .unwrap();
    let flow = json!({"id":"frozen-flow","nameZh":"旧版快照","version":"snapshot-old","protocol":{"id":first.id},"nodes":[{"id":"start","type":"START"},{"id":"end","type":"END"}],"edges":[{"source":"start","target":"end"}]});
    app.db
        .save_entity(&original, "flow_versions", "snapshot-old", &flow)
        .unwrap();
    let exported = app.export_project().unwrap();
    let imported = app
        .import_project(exported["path"].as_str().unwrap())
        .unwrap();
    let imported_id = imported["id"].as_str().unwrap();
    assert_ne!(original, imported_id);
    assert_eq!(
        app.project(),
        original,
        "import must not silently switch the active project"
    );
    let records = app.db.protocols(imported_id).unwrap();
    let selected: Vec<_> = records
        .iter()
        .filter(|p| p.name == "版本隔离协议")
        .collect();
    assert_eq!(selected.len(), 3);
    for (version, expected) in [
        (0, draft_source),
        (1, DEMO_XML),
        (2, second_source.as_str()),
    ] {
        let record = selected.iter().find(|p| p.version == version).unwrap();
        assert!(
            app.db.protocol_source(&record.id).unwrap() == expected,
            "version {version} source was overwritten during import"
        );
        assert_ne!(record.id, first.id);
        assert_ne!(record.id, second.id);
    }
    let frozen = app.db.entities(imported_id, "flow_versions").unwrap();
    let target = frozen[0]["protocol"]["id"].as_str().unwrap();
    assert_eq!(target, selected.iter().find(|p| p.version == 1).unwrap().id);
    assert!(app.db.protocol_source(&first.id).unwrap() == DEMO_XML);
    assert!(app.db.protocol_source(&second.id).unwrap() == second_source);
    assert!(app.db.protocol_source(&first.protocol_id).unwrap() == draft_source);
}

fn tampered_zip(manifest: &archive::Manifest, entries: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    zip.start_file("manifest.json", options).unwrap();
    zip.write_all(&serde_json::to_vec(manifest).unwrap())
        .unwrap();
    for (name, data) in entries {
        zip.start_file(name, options).unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
fn project_import_rejects_checksum_tampering_before_creating_any_project() {
    let (dir, app) = application();
    let exported = app.export_project().unwrap();
    let (manifest, mut entries) =
        archive::unpack(&std::fs::read(exported["path"].as_str().unwrap()).unwrap()).unwrap();
    entries.get_mut("project.json").unwrap().push(b' ');
    let path = dir.path().join("tampered.zip");
    std::fs::write(&path, tampered_zip(&manifest, &entries)).unwrap();
    let before = app.db.projects().unwrap().len();
    let error = app.import_project(path.to_str().unwrap()).unwrap_err();
    assert!(error.contains("校验失败"));
    assert_eq!(app.db.projects().unwrap().len(), before);
}

#[test]
fn checksums_do_not_replace_validation_of_published_xml() {
    let (dir, app) = application();
    let exported = app.export_project().unwrap();
    let (manifest, mut entries) =
        archive::unpack(&std::fs::read(exported["path"].as_str().unwrap()).unwrap()).unwrap();
    let payload: Value = serde_json::from_slice(&entries["project.json"]).unwrap();
    let id = payload["protocols"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["published"] == true)
        .unwrap()["id"]
        .as_str()
        .unwrap();
    entries.insert(
        format!("protocols/{id}.xml"),
        b"<!DOCTYPE protocol><protocol/>".to_vec(),
    );
    let path = dir.path().join("invalid-schema.zip");
    std::fs::write(&path, archive::pack(&manifest.name, &entries).unwrap()).unwrap();
    let before = app.db.projects().unwrap().len();
    assert!(app.import_project(path.to_str().unwrap()).is_err());
    assert_eq!(app.db.projects().unwrap().len(), before);
}

#[test]
fn duplicate_protocol_version_records_are_rejected_before_import() {
    let (dir, app) = application();
    let exported = app.export_project().unwrap();
    let (manifest, mut entries) =
        archive::unpack(&std::fs::read(exported["path"].as_str().unwrap()).unwrap()).unwrap();
    let mut payload: Value = serde_json::from_slice(&entries["project.json"]).unwrap();
    let duplicate = payload["protocols"][0].clone();
    payload["protocols"].as_array_mut().unwrap().push(duplicate);
    entries.insert("project.json".into(), serde_json::to_vec(&payload).unwrap());
    let path = dir.path().join("duplicate-version.zip");
    std::fs::write(&path, archive::pack(&manifest.name, &entries).unwrap()).unwrap();
    let before = app.db.projects().unwrap().len();
    assert!(app.import_project(path.to_str().unwrap()).is_err());
    assert_eq!(app.db.projects().unwrap().len(), before);
}

async fn wait_logs(app: &Arc<Studio>, minimum: u64) -> Value {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let logs = app.request(json!({"action":"packet_query"})).await.unwrap();
            if logs["total"].as_u64().unwrap() >= minimum {
                return logs;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("UDP logs were not persisted")
}

#[tokio::test]
async fn crc_failures_mask_header_and_auth_issue_tokens_before_persistence() {
    let dir = tempfile::tempdir().unwrap();
    let (app, rx) = Studio::open(dir.path()).unwrap();
    app.start_receiver(rx);
    let snap = app.request(json!({"action":"snapshot"})).await.unwrap();
    let protocol = snap["protocols"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["published"] == true)
        .unwrap()["id"]
        .as_str()
        .unwrap();
    let a = app
        .request(json!({"action":"endpoint_start","bind":"127.0.0.1:0","protocolId":protocol}))
        .await
        .unwrap();
    let b = app
        .request(json!({"action":"endpoint_start","bind":"127.0.0.1:0","protocolId":protocol}))
        .await
        .unwrap();
    let vectors: Value = serde_json::from_str(include_str!(
        "../../../docs/06_示例与测试数据/golden-vectors.json"
    ))
    .unwrap();
    let mut request = parse_hex(vectors["request"]["hex"].as_str().unwrap()).unwrap();
    *request.last_mut().unwrap() ^= 1;
    let schema = app.schema(protocol).unwrap();
    let mut auth=schema.encode(0x11,&json!({"header":{"srcAddr":1,"dstAddr":59,"transactionId":20,"sessionId":4660},"body":{"tokenValidSeconds":1800,"tokenLength":4,"token":"CA FE BA BE"}})).unwrap();
    *auth.last_mut().unwrap() ^= 1;
    let mut short_header_token = request.clone();
    short_header_token[24] = 1;
    let mut short_body_token = auth.clone();
    short_body_token[27] = 1;
    let mut broken_magic = request.clone();
    broken_magic[0] ^= 1;
    let mut broken_auth_flag = request.clone();
    broken_auth_flag[13] &= !1;
    let mut broken_auth_magic = auth.clone();
    broken_auth_magic[0] ^= 1;
    let mut broken_auth_code = auth.clone();
    broken_auth_code[6] ^= 1;
    let mut unknown = schema.encode(0x32,&json!({"srcAddr":1,"dstAddr":59,"sessionId":1,"transactionId":90,"resultCode":0,"serverTimeSeconds":0})).unwrap();
    unknown[6] = 0xFE;
    let unknown_hex = format_hex(&unknown);
    for bytes in [
        request,
        auth,
        short_header_token,
        short_body_token,
        broken_magic,
        broken_auth_flag,
        broken_auth_magic,
        broken_auth_code,
        unknown,
    ] {
        app.request(json!({"action":"udp_send","endpointId":a["id"],"remote":b["bind"],"hex":format_hex(&bytes)})).await.unwrap();
    }
    let logs = wait_logs(&app, 18).await;
    let text = logs.to_string();
    assert!(!text.contains("DE AD BE EF"));
    assert!(!text.contains("CA FE BA BE"));
    assert!(
        !text.contains("AD BE EF"),
        "corrupted header token length must not disclose the token suffix"
    );
    assert!(
        !text.contains("FE BA BE"),
        "corrupted AUTH_ISSUE length must not disclose token suffix"
    );
    assert!(text.contains("** ** ** **"));
    assert!(
        text.contains(&unknown_hex),
        "ordinary unknown packets should retain inspectable bytes"
    );
    assert!(
        logs["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p["status"] == "解析失败")
    );
    assert_eq!(app.domain.lock().unwrap().devices.len(), 0);
    app.shutdown.cancel();
    app.udp.stop_all().await;
}

#[tokio::test]
async fn transaction_remote_and_session_mismatch_cannot_apply_configuration() {
    let dir = tempfile::tempdir().unwrap();
    let (app, rx) = Studio::open(dir.path()).unwrap();
    app.start_receiver(rx);
    let snap = app.request(json!({"action":"snapshot"})).await.unwrap();
    let protocol = snap["protocols"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["published"] == true)
        .unwrap()["id"]
        .as_str()
        .unwrap();
    let system = &snap["systems"][0]["id"];
    let endpoint = app
        .request(json!({"action":"endpoint_start","bind":"127.0.0.1:0","protocolId":protocol}))
        .await
        .unwrap();
    let peer = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let wrong_peer = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let flow = json!({"id":"secure-config","protocol":{"id":protocol},"nodes":[{"id":"start","type":"START"},{"id":"send","type":"SEND_AND_WAIT","data":{"code":33,"expectedCodes":[34],"timeoutMs":10000,"values":{"srcAddr":"00 00 01","dstAddr":"00 00 3B","sessionId":4660,"body":{"configMode":1,"targetDeviceAddr":0,"items":[{"deviceAddr":59,"deviceType":1,"deviceNumericId":59,"initialParameter":1}]}}}},{"id":"end","type":"END"}],"edges":[{"source":"start","target":"send"},{"source":"send","target":"end","label":"matched"},{"source":"send","target":"end","label":"timeout"}]});
    app.request(json!({"action":"flow_save","flow":flow}))
        .await
        .unwrap();
    app.request(json!({"action":"flow_publish","flowId":"secure-config"}))
        .await
        .unwrap();
    app.request(json!({"action":"flow_run","flowId":"secure-config","endpointId":endpoint["id"],"remote":peer.local_addr().unwrap().to_string(),"variables":{"systemId":system}})).await.unwrap();
    let mut buffer = [0u8; 2048];
    let (len, source) = tokio::time::timeout(Duration::from_secs(3), peer.recv_from(&mut buffer))
        .await
        .unwrap()
        .unwrap();
    let schema = app.schema(protocol).unwrap();
    let request = schema.decode(&buffer[..len]).unwrap();
    let header = &request.values["header"];
    let correct = json!({"header":{"srcAddr":header["dstAddr"],"dstAddr":header["srcAddr"],"transactionId":header["transactionId"],"sessionId":header["sessionId"],"flags":2},"body":{"results":[{"result":170}]}});
    let mut wrong_txn = correct.clone();
    wrong_txn["header"]["transactionId"] = json!(header["transactionId"].as_u64().unwrap() + 1);
    peer.send_to(&schema.encode(0x22, &wrong_txn).unwrap(), source)
        .await
        .unwrap();
    wait_logs(&app, 2).await;
    assert_eq!(app.domain.lock().unwrap().devices.len(), 0);
    wrong_peer
        .send_to(&schema.encode(0x22, &correct).unwrap(), source)
        .await
        .unwrap();
    wait_logs(&app, 3).await;
    assert_eq!(app.domain.lock().unwrap().devices.len(), 0);
    let mut wrong_session = correct.clone();
    wrong_session["header"]["sessionId"] = json!(4661);
    peer.send_to(&schema.encode(0x22, &wrong_session).unwrap(), source)
        .await
        .unwrap();
    wait_logs(&app, 4).await;
    assert_eq!(app.domain.lock().unwrap().devices.len(), 0);
    assert_eq!(app.run_views().1.len(), 1);
    peer.send_to(&schema.encode(0x22, &correct).unwrap(), source)
        .await
        .unwrap();
    wait_logs(&app, 5).await;
    assert_eq!(app.domain.lock().unwrap().devices.len(), 1);
    assert!(app.run_views().1.is_empty());
    peer.send_to(&schema.encode(0x22, &correct).unwrap(), source)
        .await
        .unwrap();
    wait_logs(&app, 6).await;
    assert_eq!(app.domain.lock().unwrap().devices.len(), 1);
    app.shutdown.cancel();
    app.udp.stop_all().await;
}

#[tokio::test]
async fn queued_packets_keep_their_original_project_after_endpoint_stop_and_switch() {
    let dir = tempfile::tempdir().unwrap();
    let (app, mut receiver) = Studio::open(dir.path()).unwrap();
    let original = app.project();
    let protocol = app
        .db
        .protocols(&original)
        .unwrap()
        .into_iter()
        .find(|p| p.published)
        .unwrap()
        .id;
    let endpoint = app
        .request(json!({"action":"endpoint_start","bind":"127.0.0.1:0","protocolId":protocol}))
        .await
        .unwrap();
    let packet = app.schema(&protocol).unwrap().encode(0x32,&json!({"srcAddr":1,"dstAddr":59,"sessionId":1,"transactionId":77,"resultCode":0,"serverTimeSeconds":0})).unwrap();
    let sender = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    sender
        .send_to(&packet, endpoint["bind"].as_str().unwrap())
        .await
        .unwrap();
    let queued = tokio::time::timeout(Duration::from_secs(3), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    app.request(json!({"action":"endpoint_stop","id":endpoint["id"]}))
        .await
        .unwrap();
    app.request(json!({"action":"project_create","name":"隔离的新项目"}))
        .await
        .unwrap();
    app.record_packet(queued).unwrap();
    assert_eq!(
        app.db.packets(&app.project(), 0, 10, "").unwrap()["total"],
        0,
        "queued packet was attributed to the newly opened project"
    );
    assert_eq!(app.db.packets(&original, 0, 10, "").unwrap()["total"], 1);
    app.shutdown.cancel();
}

#[tokio::test]
async fn delayed_peer_response_does_not_escape_through_a_restarted_endpoint() {
    let dir = tempfile::tempdir().unwrap();
    let (app, receiver) = Studio::open(dir.path()).unwrap();
    app.start_receiver(receiver);
    let protocol = app
        .db
        .protocols(&app.project())
        .unwrap()
        .into_iter()
        .find(|p| p.published)
        .unwrap()
        .id;
    let endpoint = app.request(json!({"action":"endpoint_start","id":"reusable","bind":"127.0.0.1:0","protocolId":protocol,"virtualPeer":true,"fault":{"delayMs":400}})).await.unwrap();
    let packet = app.schema(&protocol).unwrap().encode(0x31,&json!({"srcAddr":1,"dstAddr":59,"sessionId":1,"transactionId":77,"uptimeSeconds":1,"statusFlags":0})).unwrap();
    let sender = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    sender
        .send_to(&packet, endpoint["bind"].as_str().unwrap())
        .await
        .unwrap();
    wait_logs(&app, 1).await;
    app.request(json!({"action":"endpoint_stop","id":"reusable"}))
        .await
        .unwrap();
    app.request(json!({"action":"endpoint_start","id":"reusable","bind":"127.0.0.1:0","protocolId":protocol})).await.unwrap();
    let mut buffer = [0; 2048];
    let result =
        tokio::time::timeout(Duration::from_millis(700), sender.recv_from(&mut buffer)).await;
    app.shutdown.cancel();
    app.udp.stop_all().await;
    assert!(
        result.is_err(),
        "old delayed response was transmitted from the replacement socket"
    );
}

#[tokio::test]
async fn session_token_alias_is_redacted_from_snapshots_and_finished_run_storage() {
    let (_dir, app) = application();
    let protocol = app
        .db
        .protocols(&app.project())
        .unwrap()
        .into_iter()
        .find(|p| p.published)
        .unwrap()
        .id;
    let system = app.db.entities(&app.project(), "systems").unwrap()[0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let device = {
        let mut domain = app.domain.lock().unwrap();
        let device = domain
            .create_device(&system, "认证设备", "MUSP", vec![0, 0, 59], "MANUAL")
            .unwrap();
        let now = musp_studio::network::now_ms();
        let session = domain
            .permit_session(
                &device,
                &format!("endpoint|{protocol}|127.0.0.1:5000"),
                9,
                now + 60_000,
                true,
            )
            .unwrap();
        domain
            .issue_token(&session, vec![0xCA, 0xFE, 0xBA, 0xBE], now + 60_000, now)
            .unwrap();
        device
    };
    let flow = json!({"id":"secret-alias","protocol":{"id":protocol},"nodes":[{"id":"start","type":"START"},{"id":"secret","type":"UPDATE_SESSION","data":{"operation":"session_token","deviceId":device,"sessionId":9}},{"id":"alias","type":"ASSIGN","data":{"set":[{"path":"credential","from":"step.token"}]}},{"id":"end","type":"END"}],"edges":[{"source":"start","target":"secret"},{"source":"secret","target":"alias"},{"source":"alias","target":"end"}]});
    app.request(json!({"action":"flow_save","flow":flow}))
        .await
        .unwrap();
    app.request(json!({"action":"flow_publish","flowId":"secret-alias"}))
        .await
        .unwrap();
    app.request(json!({"action":"flow_run","flowId":"secret-alias","endpointId":"endpoint","remote":"127.0.0.1:5000"})).await.unwrap();
    let views = app.run_views();
    assert_eq!(
        views.0[0]["status"], "COMPLETED",
        "fixture must actually retrieve and alias the runtime token"
    );
    let public = json!(views).to_string();
    let persisted = json!(app.db.entities(&app.project(), "runs").unwrap()).to_string();
    assert!(
        !public.contains("CA FE BA BE"),
        "renamed token leaked through snapshot"
    );
    assert!(
        !persisted.contains("CA FE BA BE"),
        "renamed token leaked into finished run storage"
    );
    app.shutdown.cancel();
}

#[test]
fn virtual_peer_resources_are_isolated_by_remote_controller() {
    let schema = musp_studio::protocol::compile_xml(musp_studio::application::SEED_XML).unwrap();
    let mut peer = musp_studio::simulator::PeerState::default();
    let request=schema.decode(&schema.encode(0x70,&json!({"srcAddr":1,"dstAddr":59,"sessionId":1,"transactionId":1,"body":{"callId":10,"resourceId":10,"assignedRxKbps":60,"rxFreqKHz":145000,"slotIndex":1,"resourceVersion":1,"applyFlag":1}})).unwrap()).unwrap();
    let applied = musp_studio::simulator::respond(&schema, &request, &mut peer, "127.0.0.1:10001")
        .unwrap()
        .unwrap();
    assert_eq!(
        schema.decode(&applied).unwrap().values["body"]["resultCode"],
        0
    );
    let release=schema.decode(&schema.encode(0x76,&json!({"srcAddr":1,"dstAddr":59,"sessionId":1,"transactionId":2,"body":{"callId":10,"resourceId":10,"direction":1,"resourceVersion":1,"releaseReason":0}})).unwrap()).unwrap();
    let reply = musp_studio::simulator::respond(&schema, &release, &mut peer, "127.0.0.1:10002")
        .unwrap()
        .unwrap();
    assert_eq!(
        schema.decode(&reply).unwrap().values["body"]["resultCode"],
        1,
        "different controller must not release another controller's resource"
    );
}

#[test]
fn session_token_requires_matching_endpoint_protocol_and_remote() {
    let mut domain = musp_studio::domain::DomainStore::default();
    let device = domain
        .create_device("system", "device", "MUSP", vec![0, 0, 59], "MANUAL")
        .unwrap();
    let now = musp_studio::network::now_ms();
    let session = domain
        .permit_session(
            &device,
            "endpoint|version|127.0.0.1:5000",
            9,
            now + 60_000,
            true,
        )
        .unwrap();
    domain
        .issue_token(&session, vec![0xCA, 0xFE, 0xBA, 0xBE], now + 60_000, now)
        .unwrap();
    let request = json!({"operation":"session_token","deviceId":device,"sessionId":9,"endpointId":"endpoint","protocolVersionId":"version","remote":"127.0.0.1:5000"});
    assert!(musp_studio::business::update_domain(&mut domain, "project", "run", &request).is_ok());
    for (key, value) in [
        ("endpointId", "other-endpoint"),
        ("protocolVersionId", "other-version"),
        ("remote", "127.0.0.1:5001"),
    ] {
        let mut invalid = request.clone();
        invalid[key] = json!(value);
        assert!(
            musp_studio::business::update_domain(&mut domain, "project", "run", &invalid).is_err(),
            "scope mismatch {key} returned another session's token"
        );
    }
}

#[tokio::test]
async fn auth_issue_routes_to_the_registered_session_in_a_second_system() {
    let dir = tempfile::tempdir().unwrap();
    let (app, rx) = Studio::open(dir.path()).unwrap();
    app.start_receiver(rx);
    let protocol = app
        .db
        .protocols(&app.project())
        .unwrap()
        .into_iter()
        .find(|p| p.published)
        .unwrap()
        .id;
    let second = app
        .request(json!({"action":"system_create","name":"第二个独立控制系统"}))
        .await
        .unwrap();
    let endpoint = app
        .request(json!({"action":"endpoint_start","bind":"127.0.0.1:0","protocolId":protocol}))
        .await
        .unwrap();
    let sender = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let session = {
        let mut domain = app.domain.lock().unwrap();
        let device = domain
            .create_device(
                second["id"].as_str().unwrap(),
                "认证设备",
                "MUSP",
                vec![0, 0, 59],
                "MANUAL",
            )
            .unwrap();
        domain
            .permit_session(
                &device,
                &format!(
                    "{}|{}|{}",
                    endpoint["id"].as_str().unwrap(),
                    protocol,
                    sender.local_addr().unwrap()
                ),
                9,
                musp_studio::network::now_ms() + 60_000,
                true,
            )
            .unwrap()
    };
    let bytes=app.schema(&protocol).unwrap().encode(0x11,&json!({"srcAddr":1,"dstAddr":59,"sessionId":9,"transactionId":1,"tokenValidSeconds":60,"tokenLength":4,"token":"CA FE BA BE"})).unwrap();
    sender
        .send_to(&bytes, endpoint["bind"].as_str().unwrap())
        .await
        .unwrap();
    wait_logs(&app, 1).await;
    let result = app
        .domain
        .lock()
        .unwrap()
        .session_token(&session, musp_studio::network::now_ms())
        .map(|b| b.to_vec());
    app.shutdown.cancel();
    app.udp.stop_all().await;
    assert_eq!(
        result.unwrap(),
        vec![0xCA, 0xFE, 0xBA, 0xBE],
        "auth packet should use its scoped active session instead of the first system"
    );
}

#[test]
fn virtual_peer_registration_sessions_are_isolated_by_remote_controller() {
    let schema = musp_studio::protocol::compile_xml(musp_studio::application::SEED_XML).unwrap();
    let mut peer = musp_studio::simulator::PeerState::default();
    let request=schema.decode(&schema.encode(1,&json!({"srcAddr":59,"dstAddr":1,"sessionId":0,"transactionId":1,"deviceType":1,"deviceNumericId":59,"bootReason":1,"capabilityFlags":3,"configVersion":1})).unwrap()).unwrap();
    let first = musp_studio::simulator::respond(&schema, &request, &mut peer, "127.0.0.1:10001")
        .unwrap()
        .unwrap();
    let first_session = schema.decode(&first).unwrap().values["header"]["sessionId"].clone();
    musp_studio::simulator::respond(&schema, &request, &mut peer, "127.0.0.1:10002")
        .unwrap()
        .unwrap();
    let confirm=schema.decode(&schema.encode(3,&json!({"srcAddr":59,"dstAddr":1,"sessionId":first_session,"transactionId":2,"deviceNumericId":59,"confirmResult":0})).unwrap()).unwrap();
    let reply = musp_studio::simulator::respond(&schema, &confirm, &mut peer, "127.0.0.1:10001")
        .unwrap()
        .unwrap();
    assert_eq!(
        schema.decode(&reply).unwrap().values["body"]["resultCode"],
        0,
        "second remote must not replace the first remote's registration session"
    );
}

#[cfg(windows)]
#[tokio::test]
async fn closed_remote_udp_port_does_not_disable_the_receiving_endpoint() {
    use musp_studio::network::{Direction, UdpHub};
    let (hub, mut events) = UdpHub::new(8);
    let endpoint = hub
        .start(
            "icmp-check".into(),
            "端口离线测试".into(),
            "127.0.0.1:0",
            "schema".into(),
        )
        .await
        .unwrap();
    let closed = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let address = closed.local_addr().unwrap();
    drop(closed);
    hub.send(&endpoint.id, &address.to_string(), &[1])
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(150)).await;
    let sender = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
    sender.send_to(&[2], &endpoint.bind).await.unwrap();
    let received = tokio::time::timeout(Duration::from_secs(1), async {
        while let Some(packet) = events.recv().await {
            if packet.direction == Direction::Rx && packet.bytes == [2] {
                return true;
            }
        }
        false
    })
    .await
    .unwrap_or(false);
    let running = hub.list().await[0].running;
    hub.stop_all().await;
    assert!(
        running && received,
        "an unreachable remote disabled the shared UDP receiving socket"
    );
}
