//! Real Zenoh over loopback TCP. Every test uses its own explicit
//! `tcp/127.0.0.1:<port>` endpoint and its own topic prefix, with multicast
//! scouting off, so tests running in parallel never see each other or the LAN.

use dorobot_plugin_dviz::{DvizSession, SessionOptions, Sink, RING_CAP};
use dviz_protocol::{binary_formats, serialize_message, MvizMessage};
use serde_json::{json, Value};
use std::{
    net::TcpListener,
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
    thread,
    time::{Duration, Instant},
};
use zenoh::{qos::CongestionControl, Wait};

const SETTLE: Duration = Duration::from_secs(3);
/// `disconnect` must not wait for the receiver thread.
const DISCONNECT_MAX: Duration = Duration::from_millis(50);
/// The receiver ends within the 1 s connect timeout plus the session close.
const STOP_WAIT: Duration = Duration::from_millis(2500);

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn wait_until(timeout: Duration, mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        if done() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn listen_options(port: u16, prefix: &str) -> SessionOptions {
    SessionOptions {
        mode: "peer".into(),
        connect_endpoints: Vec::new(),
        listen_endpoints: vec![format!("tcp/127.0.0.1:{port}")],
        prefix: prefix.into(),
    }
}

/// A `DvizSession` listening on a fresh loopback port, already connected.
fn listening_session(prefix: &str) -> (DvizSession, u16) {
    let port = free_port();
    let session = DvizSession::new();
    session.connect(listen_options(port, prefix)).unwrap();
    assert!(
        wait_until(SETTLE, || session.snapshot().connected),
        "session never connected: {:?}",
        session.snapshot().last_error
    );
    (session, port)
}

/// The test's own publishing peer, connected only to `port`.
fn publisher_peer(port: u16) -> zenoh::Session {
    let mut config = zenoh::Config::default();
    config.insert_json5("mode", r#""peer""#).unwrap();
    config
        .insert_json5("connect/endpoints", &format!(r#"["tcp/127.0.0.1:{port}"]"#))
        .unwrap();
    config.insert_json5("listen/endpoints", "[]").unwrap();
    config
        .insert_json5("scouting/multicast/enabled", "false")
        .unwrap();
    config
        .insert_json5("transport/shared_memory/enabled", "false")
        .unwrap();
    zenoh::open(config).wait().unwrap()
}

/// A blocking publisher on `key`, returned once the session's subscriber is
/// known to it, so no put is lost to subscription propagation.
fn matched_publisher(peer: &zenoh::Session, key: String) -> zenoh::pubsub::Publisher<'static> {
    let publisher = peer
        .declare_publisher(key)
        .congestion_control(CongestionControl::Block)
        .wait()
        .unwrap();
    assert!(
        wait_until(SETTLE, || publisher
            .matching_status()
            .wait()
            .is_ok_and(|status| status.matching())),
        "the session's subscriber never reached the publisher"
    );
    publisher
}

fn message(msg_type: &str, data: Value, binary: Option<&[u8]>) -> Vec<u8> {
    let header = MvizMessage {
        msg_type: msg_type.to_string(),
        timestamp: Some(0.5),
        data,
        format: binary.map(|_| binary_formats::POINTS_XYZ_F32.to_string()),
        count: binary.map(|b| (b.len() / 12) as u32),
    };
    serialize_message(&header, binary)
}

fn points() -> Vec<u8> {
    let xyz: Vec<u8> = [0.0f32, 1.0, 2.0]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    message("points3d", Value::Null, Some(&xyz))
}

struct SetOnDrop<'a>(&'a AtomicBool);

impl Drop for SetOnDrop<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

struct SlowSink;

impl Sink for SlowSink {
    fn log(&mut self, _: &str, _: &MvizMessage, _: Option<&[u8]>) -> Result<(), String> {
        thread::sleep(Duration::from_millis(20));
        Ok(())
    }
}

#[test]
fn session_receives_over_loopback() {
    let prefix = "dviz_test_receive";
    let (session, port) = listening_session(prefix);
    let peer = publisher_peer(port);
    let logs = matched_publisher(&peer, format!("{prefix}/logs"));
    let lidar = matched_publisher(&peer, format!("{prefix}/world/lidar"));

    let line = json!({"level": "INFO", "message": "hello over loopback", "node_id": "fake"});
    logs.put(message("log", line, None)).wait().unwrap();
    lidar.put(points()).wait().unwrap();

    assert!(
        wait_until(SETTLE, || {
            let snapshot = session.snapshot();
            snapshot.received >= 2
                && snapshot
                    .logs
                    .iter()
                    .any(|log| log.message == "hello over loopback" && log.node_id == "fake")
        }),
        "{:?}",
        session.snapshot()
    );
    let snapshot = session.snapshot();
    assert_eq!(snapshot.entities["world/lidar"].count, 1);
    assert_eq!(snapshot.entities["world/lidar"].msg_type, "points3d");
    assert_eq!(snapshot.malformed, 0);
    assert_eq!(snapshot.generation, 1);
    assert_eq!(snapshot.receivers_alive, 1);
    assert!(snapshot.last_error.is_none());
}

fn connect_options(mode: &str, endpoint: &str) -> SessionOptions {
    SessionOptions {
        mode: mode.into(),
        connect_endpoints: vec![endpoint.to_string()],
        listen_endpoints: Vec::new(),
        prefix: "dviz".into(),
    }
}

/// Connects to `endpoint`, where nothing listens, and checks that `connect`
/// returns at once and the error names the endpoint.
fn assert_unreachable_is_reported(mode: &str, endpoint: &str) -> DvizSession {
    let session = DvizSession::new();
    let started = Instant::now();
    session.connect(connect_options(mode, endpoint)).unwrap();
    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_millis(100),
        "connect took {elapsed:?}"
    );

    assert!(
        wait_until(SETTLE, || session.snapshot().last_error.is_some()),
        "no error reported for an unreachable {mode} endpoint: {:?}",
        session.snapshot()
    );
    let snapshot = session.snapshot();
    assert!(!snapshot.connected);
    assert_eq!(snapshot.generation, 0);
    let error = snapshot.last_error.unwrap();
    assert!(
        error.contains(endpoint),
        "error does not name {endpoint}: {error}"
    );
    session
}

#[test]
fn connect_to_unreachable_endpoint_reports_error_without_blocking() {
    let endpoint = format!("tcp/127.0.0.1:{}", free_port());
    assert_unreachable_is_reported("client", &endpoint);
}

#[test]
fn peer_connect_to_unreachable_endpoint_reports_error() {
    let endpoint = format!("tcp/127.0.0.1:{}", free_port());
    let session = assert_unreachable_is_reported("peer", &endpoint);
    // Still not connected a while later: the open session alone is not a link.
    thread::sleep(Duration::from_millis(300));
    let snapshot = session.snapshot();
    assert!(!snapshot.connected);
    assert!(snapshot.last_error.is_some());
}

#[test]
fn peer_error_clears_once_the_endpoint_appears() {
    let port = free_port();
    let endpoint = format!("tcp/127.0.0.1:{port}");
    let session = assert_unreachable_is_reported("peer", &endpoint);

    let mut config = zenoh::Config::default();
    config.insert_json5("mode", r#""peer""#).unwrap();
    config
        .insert_json5("listen/endpoints", &format!(r#"["{endpoint}"]"#))
        .unwrap();
    config
        .insert_json5("scouting/multicast/enabled", "false")
        .unwrap();
    config
        .insert_json5("transport/shared_memory/enabled", "false")
        .unwrap();
    let _late_peer = zenoh::open(config).wait().unwrap();

    // Zenoh's background connector backs off up to 4 s between attempts.
    assert!(
        wait_until(Duration::from_secs(10), || session.snapshot().connected),
        "never connected after the peer appeared: {:?}",
        session.snapshot()
    );
    let snapshot = session.snapshot();
    assert!(snapshot.last_error.is_none(), "{:?}", snapshot.last_error);
    assert_eq!(snapshot.generation, 1);
}

#[test]
fn disconnect_returns_at_once_even_during_a_blocked_open() {
    let endpoint = format!("tcp/127.0.0.1:{}", free_port());
    let session = DvizSession::new();
    session
        .connect(connect_options("client", &endpoint))
        .unwrap();
    // The receiver is now inside zenoh::open, which blocks for up to 1 s.
    thread::sleep(Duration::from_millis(200));

    let started = Instant::now();
    session.disconnect();
    let elapsed = started.elapsed();
    assert!(elapsed < DISCONNECT_MAX, "disconnect took {elapsed:?}");
    assert!(!session.snapshot().connected);
    assert!(
        wait_until(STOP_WAIT, || session.snapshot().receivers_alive == 0),
        "the receiver outlived disconnect by more than {STOP_WAIT:?}"
    );
    assert!(session.snapshot().last_error.is_none());
}

#[test]
fn invalid_options_are_rejected_immediately() {
    let session = DvizSession::new();
    let client_without_endpoint = SessionOptions {
        mode: "client".into(),
        ..SessionOptions::default()
    };
    assert!(session.connect(client_without_endpoint).is_err());
    let bad_mode = SessionOptions {
        mode: "router".into(),
        ..SessionOptions::default()
    };
    assert!(session.connect(bad_mode).is_err());
    let bad_prefix = SessionOptions {
        prefix: String::new(),
        ..SessionOptions::default()
    };
    assert!(session.connect(bad_prefix).is_err());

    let snapshot = session.snapshot();
    assert_eq!(snapshot.receivers_alive, 0);
    assert!(snapshot.last_error.is_some());
    assert_eq!(SessionOptions::default().prefix, "dviz");
    assert_eq!(SessionOptions::default().mode, "peer");
}

#[test]
fn reconnect_replaces_the_single_receiver() {
    let port = free_port();
    let options = listen_options(port, "dviz_test_reconnect");
    let session = DvizSession::new();
    let most_alive = AtomicU32::new(0);
    let done = AtomicBool::new(false);

    thread::scope(|scope| {
        // Stops the watcher even when an assertion below fails, so a failure
        // cannot hang the scope.
        let _stop_watcher = SetOnDrop(&done);
        scope.spawn(|| {
            while !done.load(Ordering::Relaxed) {
                most_alive.fetch_max(session.snapshot().receivers_alive, Ordering::Relaxed);
                thread::sleep(Duration::from_millis(1));
            }
        });

        session.connect(options.clone()).unwrap();
        assert!(wait_until(SETTLE, || session.snapshot().connected));
        assert_eq!(session.snapshot().generation, 1);
        assert_eq!(session.snapshot().receivers_alive, 1);

        let started = Instant::now();
        session.disconnect();
        let elapsed = started.elapsed();
        assert!(elapsed < DISCONNECT_MAX, "disconnect took {elapsed:?}");
        assert!(!session.snapshot().connected);
        assert!(
            wait_until(STOP_WAIT, || session.snapshot().receivers_alive == 0),
            "the receiver outlived disconnect by more than {STOP_WAIT:?}"
        );

        session.connect(options.clone()).unwrap();
        assert!(
            wait_until(SETTLE, || session.snapshot().connected),
            "{:?}",
            session.snapshot().last_error
        );
        assert_eq!(session.snapshot().generation, 2);
        assert_eq!(session.snapshot().receivers_alive, 1);

        // Connecting while connected replaces the receiver without a disconnect.
        session.connect(options).unwrap();
        assert!(
            wait_until(SETTLE, || session.snapshot().generation == 3),
            "{:?}",
            session.snapshot().last_error
        );
        assert!(session.snapshot().connected);
        assert_eq!(session.snapshot().receivers_alive, 1);
    });
    assert_eq!(
        most_alive.load(Ordering::Relaxed),
        1,
        "more than one receiver was alive"
    );

    session.disconnect();
    assert!(wait_until(STOP_WAIT, || session.snapshot().receivers_alive == 0));
}

#[test]
fn burst_beyond_capacity_drops_oldest_and_counts() {
    const BURST: u64 = 500;
    let prefix = "dviz_test_burst";
    let (session, port) = listening_session(prefix);
    session.set_sink(Box::new(SlowSink));
    let peer = publisher_peer(port);
    let lidar = matched_publisher(&peer, format!("{prefix}/world/lidar"));

    let payload = points();
    for _ in 0..BURST {
        lidar.put(payload.clone()).wait().unwrap();
    }

    // Settled once nothing has moved for longer than many 20 ms sink calls.
    let deadline = Instant::now() + Duration::from_secs(15);
    let mut last = None;
    let (received, dropped) = loop {
        thread::sleep(Duration::from_millis(300));
        let snapshot = session.snapshot();
        let now = (snapshot.received, snapshot.dropped);
        if now.0 + now.1 > 0 && last == Some(now) {
            break now;
        }
        assert!(
            Instant::now() < deadline,
            "the burst never settled: {now:?}"
        );
        last = Some(now);
    };

    assert!(dropped > 0, "nothing was dropped (received {received})");
    let total = received + dropped;
    assert!(
        (BURST - RING_CAP as u64..=BURST).contains(&total),
        "received {received} + dropped {dropped} = {total}, expected about {BURST}"
    );
    assert_eq!(session.snapshot().entities["world/lidar"].count, received);
}
