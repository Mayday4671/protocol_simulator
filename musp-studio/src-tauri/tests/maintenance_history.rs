use musp_studio::{application::Studio, domain::DeviceState, network::now_ms};
use serde_json::json;
use std::{sync::atomic::Ordering, time::Duration};

async fn check_expiry(heartbeat: bool) {
    let directory = tempfile::tempdir().unwrap();
    let (app, _) = Studio::open(directory.path()).unwrap();
    app.heartbeat_timeout_ms
        .store(if heartbeat { 50 } else { 0 }, Ordering::Relaxed);
    let (device, session) = {
        let mut domain = app.domain.lock().unwrap();
        let device = domain
            .create_device("system", "idle device", "MUSP", vec![0, 0, 59], "TEST")
            .unwrap();
        let session = domain
            .permit_session(
                &device,
                "test-peer",
                1,
                now_ms() + if heartbeat { 60000 } else { 50 },
                false,
            )
            .unwrap();
        domain.complete_registration(&session, 0, now_ms()).unwrap();
        app.db
            .save_entity(&app.project(), "domain", "current", &json!(*domain))
            .unwrap();
        (device, session)
    };
    app.start_scheduler();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let persisted = app.db.entities(&app.project(), "domain").unwrap();
            if persisted[0]["devices"][&device]["state"] == "OFFLINE" {
                assert_eq!(persisted[0]["sessions"][&session]["active"], false);
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("expiry without call cleanup must still persist OFFLINE");
    assert_eq!(
        app.domain.lock().unwrap().devices[&device].state,
        DeviceState::Offline
    );
    let history = app
        .request(json!({"action":"device_history","id":device}))
        .await
        .unwrap();
    let entries = history.as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert!(
        entries
            .iter()
            .any(|event| event["before"]["state"] == "ONLINE"
                && event["after"]["state"] == "OFFLINE")
    );
    tokio::time::sleep(Duration::from_millis(1100)).await;
    assert_eq!(
        app.request(json!({"action":"device_history","id":device}))
            .await
            .unwrap(),
        history
    );
    app.shutdown.cancel();
}

#[tokio::test]
async fn idle_device_heartbeat_expiry_persists_offline_and_one_history_event() {
    check_expiry(true).await;
}

#[tokio::test]
async fn idle_device_session_expiry_persists_offline_and_one_history_event() {
    check_expiry(false).await;
}

#[tokio::test]
async fn uncompleted_session_license_expires_and_is_persisted() {
    let directory = tempfile::tempdir().unwrap();
    let (app, _) = Studio::open(directory.path()).unwrap();
    app.heartbeat_timeout_ms.store(0, Ordering::Relaxed);
    let session = {
        let mut domain = app.domain.lock().unwrap();
        let device = domain
            .create_device("system", "registering", "MUSP", vec![0, 0, 59], "TEST")
            .unwrap();
        let session = domain
            .permit_session(&device, "test-peer", 1, now_ms() + 50, false)
            .unwrap();
        app.db
            .save_entity(&app.project(), "domain", "current", &json!(*domain))
            .unwrap();
        session
    };
    app.start_scheduler();
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            let persisted = app.db.entities(&app.project(), "domain").unwrap();
            if persisted[0]["sessions"][&session]["active"] == false {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("uncompleted license must expire even while device is already OFFLINE");
    assert_eq!(
        app.db
            .entities(&app.project(), "device_events")
            .unwrap()
            .len(),
        1
    );
    app.shutdown.cancel();
}
