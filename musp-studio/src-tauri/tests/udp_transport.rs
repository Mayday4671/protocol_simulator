use musp_studio::network::{Direction, UdpHub};
use tokio::time::{Duration, timeout};

#[tokio::test]
async fn actual_two_socket_roundtrip_and_stop_releases_port() {
    let (hub, mut events) = UdpHub::new(64);
    let a = hub
        .start("a".into(), "A".into(), "127.0.0.1:0", "schema".into())
        .await
        .unwrap();
    let b = hub
        .start("b".into(), "B".into(), "127.0.0.1:0", "schema".into())
        .await
        .unwrap();
    assert_ne!(a.bind, b.bind);
    let bytes = b"real UDP payload";
    hub.send("a", &b.bind, bytes).await.unwrap();
    let mut rx = None;
    for _ in 0..2 {
        let e = timeout(Duration::from_secs(2), events.recv())
            .await
            .unwrap()
            .unwrap();
        if e.direction == Direction::Rx {
            rx = Some(e);
        }
    }
    let received = rx.unwrap();
    assert_eq!(received.bytes, bytes);
    assert_eq!(received.endpoint_id, "b");
    assert_eq!(received.remote, a.bind);
    assert!(
        hub.start("c".into(), "C".into(), &a.bind, "schema".into())
            .await
            .is_err()
    );
    hub.stop("a").await.unwrap();
    assert!(hub.send("a", &b.bind, bytes).await.is_err());
    hub.start("c".into(), "C".into(), &a.bind, "schema".into())
        .await
        .unwrap();
    hub.stop_all().await;
    assert!(hub.list().await.is_empty());
}

#[tokio::test]
async fn full_queue_is_observable_and_packets_are_bounded() {
    let (hub, _events) = UdpHub::new(1);
    let a = hub
        .start("a".into(), "A".into(), "127.0.0.1:0", "v1".into())
        .await
        .unwrap();
    hub.send("a", &a.bind, &[1]).await.unwrap();
    hub.send("a", &a.bind, &[2]).await.unwrap();
    assert!(hub.metrics().queue_dropped >= 1);
    assert!(hub.send("a", &a.bind, &vec![0; 65508]).await.is_err());
    hub.stop_all().await;
}
