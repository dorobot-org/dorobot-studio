//! Real Zenoh over loopback TCP. Every test uses its own explicit
//! `tcp/127.0.0.1:<port>` endpoint and its own topic prefix, with multicast
//! scouting off, so tests running in parallel never see each other or the LAN.

use dorobot_plugin_dviz::{DvizSession, SessionOptions, Sink, ViewerState, RING_CAP};
use dviz_protocol::{binary_formats, serialize_message, MvizMessage};
use serde_json::{json, Value};
use std::{
    net::TcpListener,
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Mutex,
    },
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

/// A `DvizSession` listening on a fresh loopback port: open, but with no
/// peer yet, so not connected.
fn listening_session(prefix: &str) -> (DvizSession, u16) {
    let port = free_port();
    let session = DvizSession::new();
    session.connect(listen_options(port, prefix)).unwrap();
    assert!(
        wait_until(SETTLE, || session.snapshot().listening_for.is_some()),
        "session never opened: {:?}",
        session.snapshot()
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

/// A listening session plus the test's publishing peer connected to it;
/// returns once the session counts that peer as its link.
fn linked_session(prefix: &str) -> (DvizSession, zenoh::Session) {
    let (session, port) = listening_session(prefix);
    let peer = publisher_peer(port);
    assert!(
        wait_until(SETTLE, || session.snapshot().connected),
        "the session never linked to the test peer: {:?}",
        session.snapshot()
    );
    (session, peer)
}

/// A test peer listening on a fresh loopback port, for a session to connect to.
fn listening_peer() -> (zenoh::Session, String) {
    let endpoint = format!("tcp/127.0.0.1:{}", free_port());
    let mut config = zenoh::Config::default();
    config.insert_json5("mode", r#""peer""#).unwrap();
    config
        .insert_json5("listen/endpoints", &format!(r#"["{endpoint}"]"#))
        .unwrap();
    config.insert_json5("connect/endpoints", "[]").unwrap();
    config
        .insert_json5("scouting/multicast/enabled", "false")
        .unwrap();
    config
        .insert_json5("transport/shared_memory/enabled", "false")
        .unwrap();
    (zenoh::open(config).wait().unwrap(), endpoint)
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

/// Panics on every message it is given.
struct PanickingSink;

impl Sink for PanickingSink {
    fn log(&mut self, _: &str, _: &MvizMessage, _: Option<&[u8]>) -> Result<(), String> {
        panic!("sink bug");
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
    let (session, peer) = linked_session(prefix);
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

/// Alone: open and listening, with no peer and no network. That is not a
/// connection, and not an error either.
#[test]
fn listen_only_session_without_a_peer_is_not_connected() {
    let session = DvizSession::new();
    session
        .connect(listen_options(free_port(), "dviz_test_alone"))
        .unwrap();
    thread::sleep(Duration::from_millis(2200));
    let snapshot = session.snapshot();
    assert!(!snapshot.connected, "connected with no peer: {snapshot:?}");
    assert_eq!(snapshot.generation, 0);
    assert!(snapshot.last_error.is_none(), "{:?}", snapshot.last_error);
    let waited = snapshot.listening_for.expect("open and listening");
    assert!(waited >= Duration::from_secs(2), "listening for {waited:?}");
    assert_eq!(snapshot.receivers_alive, 1);
}

#[test]
fn listen_only_session_connects_once_a_peer_links() {
    let (session, port) = listening_session("dviz_test_first_peer");
    assert!(!session.snapshot().connected);
    let _peer = publisher_peer(port);
    assert!(
        wait_until(SETTLE, || session.snapshot().connected),
        "{:?}",
        session.snapshot()
    );
    let snapshot = session.snapshot();
    assert_eq!(snapshot.generation, 1);
    assert_eq!(snapshot.listening_for, None);
    assert!(snapshot.last_error.is_none(), "{:?}", snapshot.last_error);
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
    // Every connect links at once to this peer, so each counts as a connection.
    let (_peer, endpoint) = listening_peer();
    let options = SessionOptions {
        prefix: "dviz_test_reconnect".into(),
        ..connect_options("peer", &endpoint)
    };
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
    let (session, peer) = linked_session(prefix);
    session.set_sink(Box::new(SlowSink));
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

#[test]
fn fake_publisher_feeds_every_displayed_kind() {
    use dorobot_plugin_dviz::fake::{run_for, FakeOptions};

    let prefix = "dviz_test_fake";
    let (session, port) = listening_session(prefix);
    run_for(
        FakeOptions {
            listen_endpoints: Vec::new(),
            connect_endpoints: vec![format!("tcp/127.0.0.1:{port}")],
            prefix: prefix.into(),
            hz: 10.0,
        },
        Duration::from_secs(3),
    )
    .unwrap();

    // run_for returns only after its session is closed; what it sent is
    // already queued in the receiver.
    assert!(
        wait_until(SETTLE, || {
            let snapshot = session.snapshot();
            snapshot.nodes.len() == 3 && !snapshot.logs.is_empty()
        }),
        "{:?}",
        session.snapshot()
    );
    let snapshot = session.snapshot();
    for (entity, msg_type) in [
        ("world/lidar", "points3d"),
        ("world/vehicle", "transform3d"),
        ("world/path", "linestrips3d"),
        ("imu/accel_x", "scalar"),
    ] {
        assert_eq!(snapshot.entities[entity].msg_type, msg_type, "{entity}");
    }
    let mut ids: Vec<_> = snapshot.nodes.iter().map(|n| n.id.as_str()).collect();
    ids.sort_unstable();
    assert_eq!(ids, ["bridge", "planner", "sim"]);
    assert!(!snapshot.edges.is_empty());
    assert_eq!(snapshot.malformed, 0);
    assert_eq!(snapshot.dropped, 0);
}

#[test]
fn panicking_sink_is_replaced() {
    let prefix = "dviz_test_sink_panic";
    let (session, peer) = linked_session(prefix);
    session.set_sink(Box::new(PanickingSink));
    let lidar = matched_publisher(&peer, format!("{prefix}/world/lidar"));

    lidar.put(points()).wait().unwrap();
    assert!(
        wait_until(SETTLE, || session.snapshot().sink_errors == 1),
        "{:?}",
        session.snapshot()
    );
    match session.snapshot().viewer {
        ViewerState::Failed(error) => {
            assert!(error.contains("crashed"), "{error}");
            assert!(error.contains("sink bug"), "{error}");
        }
        other => panic!("the crash was not reported: {other:?}"),
    }

    // Still receiving, and the panicking sink is gone: it would panic again.
    for _ in 0..3 {
        lidar.put(points()).wait().unwrap();
    }
    assert!(
        wait_until(SETTLE, || session.snapshot().received == 4),
        "{:?}",
        session.snapshot()
    );
    let snapshot = session.snapshot();
    assert_eq!(snapshot.sink_errors, 1);
    assert_eq!(snapshot.malformed, 0);
    assert_eq!(snapshot.entities["world/lidar"].count, 4);
    assert_eq!(snapshot.receivers_alive, 1);
    assert!(snapshot.connected);
}

#[test]
fn reconnect_elsewhere_clears_state() {
    let (peer, endpoint) = listening_peer();
    let at = |prefix: &str| SessionOptions {
        prefix: prefix.into(),
        ..connect_options("peer", &endpoint)
    };
    let (first, second) = ("dviz_test_reset_a", "dviz_test_reset_b");
    let session = DvizSession::new();
    session.connect(at(first)).unwrap();
    assert!(wait_until(SETTLE, || session.snapshot().connected));

    // A backlog behind a slow sink is still draining when the prefix changes.
    session.set_sink(Box::new(SlowSink));
    let lidar = matched_publisher(&peer, format!("{first}/world/lidar"));
    let logs = matched_publisher(&peer, format!("{first}/logs"));
    let graph = matched_publisher(&peer, format!("{first}/graph"));
    let plan = matched_publisher(&peer, format!("{first}/tracer/nav/plan"));
    let plan_data = json!({"strips": [[[0.0, 0.0, 0.0], [1.0, 2.0, 0.0]]]});
    plan.put(message("linestrips3d", plan_data, None))
        .wait()
        .unwrap();
    let line = json!({"level": "INFO", "message": "before the switch", "node_id": "fake"});
    logs.put(message("log", line, None)).wait().unwrap();
    let update = json!({
        "nodes": [{"id": "lidar", "status": "active", "last_seen": 1.0}],
        "edges": [{"from_node": "lidar", "from_port": "points",
                   "to_node": "planner", "to_port": "cloud"}],
        "timestamp": 1.0,
    });
    graph
        .put(message("graph_update", update, None))
        .wait()
        .unwrap();
    for _ in 0..20 {
        lidar.put(points()).wait().unwrap();
    }
    assert!(
        wait_until(SETTLE, || {
            let snapshot = session.snapshot();
            snapshot.entities.contains_key("world/lidar")
                && !snapshot.logs.is_empty()
                && !snapshot.edges.is_empty()
                && snapshot.nav.plan.len() == 2
        }),
        "{:?}",
        session.snapshot()
    );

    session.connect(at(second)).unwrap();
    let snapshot = session.snapshot();
    assert!(snapshot.entities.is_empty(), "{:?}", snapshot.entities);
    assert_eq!(snapshot.received, 0);
    assert!(snapshot.logs.is_empty());
    assert!(snapshot.nodes.is_empty() && snapshot.edges.is_empty());
    assert!(snapshot.nav.plan.is_empty() && snapshot.nav.updated == 0);
    assert_eq!(
        (snapshot.dropped, snapshot.malformed, snapshot.sink_errors),
        (0, 0, 0)
    );
    // Nothing of the old prefix's backlog comes in afterwards.
    thread::sleep(Duration::from_millis(500));
    let snapshot = session.snapshot();
    assert!(snapshot.entities.is_empty(), "{:?}", snapshot.entities);
    assert_eq!(snapshot.received, 0);

    // Connecting again with the same options keeps what came in.
    assert!(wait_until(SETTLE, || session.snapshot().connected));
    let scan = matched_publisher(&peer, format!("{second}/world/scan"));
    scan.put(points()).wait().unwrap();
    assert!(
        wait_until(SETTLE, || session
            .snapshot()
            .entities
            .contains_key("world/scan")),
        "{:?}",
        session.snapshot()
    );
    session.connect(at(second)).unwrap();
    let snapshot = session.snapshot();
    assert_eq!(snapshot.entities["world/scan"].count, 1);
    assert_eq!(snapshot.received, 1);
}

/// What a sink was asked to do, and on which thread.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SinkCall {
    kind: &'static str,
    entity: String,
    thread: Option<String>,
}

/// Records every call with the name of the thread it ran on.
#[derive(Clone, Default)]
struct ThreadRecordingSink(Arc<Mutex<Vec<SinkCall>>>);

impl ThreadRecordingSink {
    fn record(&self, kind: &'static str, entity: &str) {
        self.0.lock().unwrap().push(SinkCall {
            kind,
            entity: entity.to_string(),
            thread: thread::current().name().map(str::to_string),
        });
    }

    fn calls(&self, kind: &str, entity: &str) -> Vec<SinkCall> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter(|call| call.kind == kind && call.entity == entity)
            .cloned()
            .collect()
    }
}

impl Sink for ThreadRecordingSink {
    fn log(&mut self, entity: &str, _: &MvizMessage, _: Option<&[u8]>) -> Result<(), String> {
        self.record("log", entity);
        Ok(())
    }

    fn clear(&mut self, entity: &str) -> Result<(), String> {
        self.record("clear", entity);
        Ok(())
    }
}

#[test]
fn disable_sends_clear_from_the_worker_once() {
    let prefix = "dviz_test_clear";
    let (session, peer) = linked_session(prefix);
    let sink = ThreadRecordingSink::default();
    session.set_sink(Box::new(sink.clone()));
    let lidar = matched_publisher(&peer, format!("{prefix}/world/lidar"));
    lidar.put(points()).wait().unwrap();
    assert!(
        wait_until(SETTLE, || sink.calls("log", "world/lidar").len() == 1),
        "{:?}",
        session.snapshot()
    );

    // From this (the test's) thread, with no traffic: the idle worker clears.
    session.set_entity_enabled("world/lidar", false);
    session.set_entity_enabled("world/lidar", false);
    assert!(
        wait_until(Duration::from_secs(1), || !sink
            .calls("clear", "world/lidar")
            .is_empty()),
        "no clear within 1 s"
    );
    thread::sleep(Duration::from_millis(300));
    let clears = sink.calls("clear", "world/lidar");
    assert_eq!(clears.len(), 1, "{clears:?}");
    let me = thread::current().name().map(str::to_string);
    assert_ne!(clears[0].thread, me, "cleared on the caller's thread");
    assert_eq!(
        clears[0].thread.as_deref(),
        Some("dviz-zenoh"),
        "not the worker"
    );

    // Switched off, its messages are counted but not sent.
    lidar.put(points()).wait().unwrap();
    assert!(wait_until(SETTLE, || session.snapshot().entities
        ["world/lidar"]
        .count
        == 2));
    assert_eq!(sink.calls("log", "world/lidar").len(), 1);

    // Switched on again, the next message is sent as before, with no clear.
    session.set_entity_enabled("world/lidar", true);
    lidar.put(points()).wait().unwrap();
    assert!(
        wait_until(SETTLE, || sink.calls("log", "world/lidar").len() == 2),
        "{:?}",
        session.snapshot()
    );
    assert_eq!(sink.calls("clear", "world/lidar").len(), 1);
    assert_eq!(session.snapshot().sink_errors, 0);
}

#[test]
fn disable_clears_while_zenoh_is_down() {
    // A client with nothing to connect to: Zenoh fails to open, and the
    // receiver waits to retry.
    let endpoint = format!("tcp/127.0.0.1:{}", free_port());
    let session = DvizSession::new();
    let sink = ThreadRecordingSink::default();
    session.set_sink(Box::new(sink.clone()));
    session
        .connect(connect_options("client", &endpoint))
        .unwrap();
    assert!(
        wait_until(SETTLE, || session.snapshot().last_error.is_some()),
        "{:?}",
        session.snapshot()
    );

    // Cleared between attempts: at most one blocked open (the 1 s connect
    // timeout) later.
    session.set_entity_enabled("world/lidar", false);
    assert!(
        wait_until(STOP_WAIT, || !sink.calls("clear", "world/lidar").is_empty()),
        "no clear while Zenoh is down"
    );
    let clears = sink.calls("clear", "world/lidar");
    assert_eq!(clears.len(), 1, "{clears:?}");
    assert_eq!(clears[0].thread.as_deref(), Some("dviz-zenoh"));
    session.disconnect();
}

/// A status from a bridge configured with the Tracer Mini body model.
fn status_with_model() -> Vec<u8> {
    let data = json!({"loc_ok": true, "loc_reason": "", "armed": false, "estop": false,
                      "gate_reason": "", "robot_model": "tracer_mini"});
    message("status", data, None)
}

const MODEL: &str = "tracer/robot/pose/model";

#[test]
fn model_ref_sent_once_per_connection() {
    let prefix = "dviz_test_model";
    let (peer, endpoint) = listening_peer();
    let options = SessionOptions {
        prefix: prefix.into(),
        ..connect_options("peer", &endpoint)
    };
    let session = DvizSession::new();
    let sink = ThreadRecordingSink::default();
    session.set_sink(Box::new(sink.clone()));
    for connection in 1..=2 {
        session.connect(options.clone()).unwrap();
        assert!(
            wait_until(SETTLE, || session.snapshot().connected),
            "{:?}",
            session.snapshot().last_error
        );
        let status = matched_publisher(&peer, format!("{prefix}/tracer/nav/status"));
        let before = session.snapshot().received;
        for _ in 0..3 {
            status.put(status_with_model()).wait().unwrap();
        }
        assert!(
            wait_until(SETTLE, || session.snapshot().received == before + 3),
            "{:?}",
            session.snapshot()
        );
        thread::sleep(Duration::from_millis(300));
        let models = sink.calls("log", MODEL);
        assert_eq!(
            models.len(),
            connection,
            "connection {connection}: {models:?}"
        );
        assert!(
            models
                .iter()
                .all(|call| call.thread.as_deref() == Some("dviz-zenoh")),
            "{models:?}"
        );
        drop(status);
        session.disconnect();
        assert!(wait_until(STOP_WAIT, || session.snapshot().receivers_alive == 0));
    }
    assert_eq!(session.snapshot().sink_errors, 0);
}

#[test]
fn model_resent_to_a_new_viewer() {
    let prefix = "dviz_test_model_viewer";
    let (session, peer) = linked_session(prefix);
    let first = ThreadRecordingSink::default();
    session.set_sink(Box::new(first.clone()));
    let status = matched_publisher(&peer, format!("{prefix}/tracer/nav/status"));
    status.put(status_with_model()).wait().unwrap();
    assert!(
        wait_until(SETTLE, || first.calls("log", MODEL).len() == 1),
        "{:?}",
        session.snapshot()
    );

    // A viewer opened later gets the model too, with no traffic.
    let second = ThreadRecordingSink::default();
    let viewer = second.clone();
    session
        .start_viewer(move || Ok((Box::new(viewer) as Box<dyn Sink>, "Test".to_string())))
        .unwrap();
    assert!(
        wait_until(Duration::from_secs(1), || second.calls("log", MODEL).len()
            == 1),
        "the new viewer never got the model"
    );
    thread::sleep(Duration::from_millis(300));
    assert_eq!(first.calls("log", MODEL).len(), 1);
    assert_eq!(second.calls("log", MODEL).len(), 1);
    assert_eq!(
        second.calls("log", MODEL)[0].thread.as_deref(),
        Some("dviz-zenoh")
    );
}
