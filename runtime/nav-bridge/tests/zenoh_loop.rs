//! Real Zenoh over loopback TCP: the bridge (`zenoh_publisher` + `spawn_worker`)
//! listens on its own free `tcp/127.0.0.1:<port>` with multicast scouting off, and
//! the test subscriber is a client that connects only to that endpoint, so the
//! tests never see the LAN (or the rmw_zenohd router on 7447).
//!
//! The tests run one at a time (`serial`): `encoded_count` is process-wide, and the
//! push-latency test should not share the CPU with another bridge.
//!
//! The command tests run the whole bridge (`run::Bridge`) with its queryable on
//! `dviz/tracer/cmd`; `cmd.forward` always points at a fake nav_command on an ephemeral
//! `127.0.0.1` UDP port, never at the robot's 9998.

use dorobot_nav_bridge::config::{BridgeConfig, CmdConfig, Rates};
use dorobot_nav_bridge::logs::LogSources;
use dorobot_nav_bridge::publish::{open_session, zenoh_publisher, Publish};
use dorobot_nav_bridge::run::{Bridge as NodeBridge, NodeEvent};
use dorobot_nav_bridge::worker::{encoded_count, spawn_worker, Inbox};
use dviz_protocol::{
    cmd_key, nav_entities, parse_cmd_ack, parse_grid2d, parse_message, parse_points_xyz_f32, CmdAck,
    CmdResult, Grid2DHeader, GridKind, LineStrips3DData, NavStatus, Transform3DData,
};
use serde_json::json;
use std::{
    net::{SocketAddr, TcpListener, UdpSocket},
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex, MutexGuard,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use zenoh::{sample::Sample, Wait};

static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|e| e.into_inner())
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
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

fn bridge_cfg(port: u16) -> BridgeConfig {
    BridgeConfig { listen: vec![format!("tcp/127.0.0.1:{port}")], ..BridgeConfig::default() }
}

struct Bridge {
    inbox: Arc<Inbox>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Bridge {
    fn start(cfg: BridgeConfig) -> Bridge {
        let publisher = zenoh_publisher(&cfg).expect("bridge session");
        let inbox = Inbox::new(64);
        let stop = Arc::new(AtomicBool::new(false));
        let worker = spawn_worker(cfg, inbox.clone(), publisher, stop.clone());
        Bridge { inbox, stop, worker: Some(worker) }
    }

    fn push(&self, id: &str, bytes: Vec<u8>) {
        self.inbox.push(id.to_owned(), bytes);
    }

    /// Sets the stop flag and joins; returns how long the worker took to finish
    /// (fails instead of hanging if it does not finish within 5 s).
    fn stop(&mut self) -> Duration {
        let t = Instant::now();
        self.stop.store(true, Ordering::Relaxed);
        let worker = self.worker.take().expect("already stopped");
        assert!(wait_until(Duration::from_secs(5), || worker.is_finished()), "worker ignores stop");
        let took = t.elapsed();
        worker.join().expect("worker panicked");
        took
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}

type Got = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

/// A client session connected only to the bridge, subscribed to `dviz/tracer/**`.
/// The callback sleeps `delay` before recording each sample.
struct Subscriber {
    got: Got,
    _sub: zenoh::pubsub::Subscriber<()>,
    _session: zenoh::Session,
}

impl Subscriber {
    fn connect(port: u16, delay: Duration) -> Subscriber {
        let mut config = zenoh::Config::default();
        config.insert_json5("mode", r#""client""#).unwrap();
        config.insert_json5("connect/endpoints", &format!(r#"["tcp/127.0.0.1:{port}"]"#)).unwrap();
        config.insert_json5("listen/endpoints", "[]").unwrap();
        config.insert_json5("scouting/multicast/enabled", "false").unwrap();
        let session = zenoh::open(config).wait().expect("subscriber session");
        let got: Got = Arc::default();
        let sink = got.clone();
        let sub = session
            .declare_subscriber("dviz/tracer/**")
            .callback(move |s: Sample| {
                if !delay.is_zero() {
                    thread::sleep(delay);
                }
                let key = s.key_expr().as_str().to_owned();
                sink.lock().unwrap().push((key, s.payload().to_bytes().into_owned()));
            })
            .wait()
            .expect("subscriber");
        Subscriber { got, _sub: sub, _session: session }
    }

    fn last(&self, entity: &str) -> Option<Vec<u8>> {
        let key = format!("dviz/tracer/{entity}");
        let got = self.got.lock().unwrap();
        got.iter().rev().find(|(k, _)| *k == key).map(|(_, b)| b.clone())
    }

    fn has(&self, entity: &str) -> bool {
        self.last(entity).is_some()
    }

    fn count(&self, entity: &str) -> usize {
        let key = format!("dviz/tracer/{entity}");
        self.got.lock().unwrap().iter().filter(|(k, _)| *k == key).count()
    }
}

fn odom(x: f64, y: f64, yaw: f64) -> Vec<u8> {
    let (s, c) = (yaw / 2.0).sin_cos();
    json!({
        "header": {"timestamp": 1.0, "frame_id": "map", "child_frame_id": "base_footprint", "seq": 0},
        "pose": {"position": {"x": x, "y": y, "z": 0.0}, "orientation": {"x": 0.0, "y": 0.0, "z": s, "w": c}},
        "twist": {"linear": {"x": 0.0, "y": 0.0, "z": 0.0}, "angular": {"x": 0.0, "y": 0.0, "z": 0.0}}
    })
    .to_string()
    .into_bytes()
}

/// f64 stamp, angle_min, angle_increment, range_min, range_max, u32 n, n ranges.
fn scan(angle_min: f32, ranges: &[f32]) -> Vec<u8> {
    let mut b = 1.0f64.to_le_bytes().to_vec();
    for v in [angle_min, 0.01, 0.1, 10.0] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b.extend_from_slice(&(ranges.len() as u32).to_le_bytes());
    for r in ranges {
        b.extend_from_slice(&r.to_le_bytes());
    }
    b
}

/// f64 stamp, u32 n, n x (x, y, z, intensity).
fn cloud(n: usize) -> Vec<u8> {
    let mut b = 1.0f64.to_le_bytes().to_vec();
    b.extend_from_slice(&(n as u32).to_le_bytes());
    for i in 0..n {
        let a = i as f32 * 0.001;
        for v in [3.0 * a.cos(), 3.0 * a.sin(), 0.5, 1.0] {
            b.extend_from_slice(&v.to_le_bytes());
        }
    }
    b
}

/// This thread's involuntary context switches (Linux /proc), to tell the scheduler
/// preempting the test thread apart from `push` itself being slow.
fn preemptions() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/thread-self/status").ok()?;
    status.lines().find_map(|l| l.strip_prefix("nonvoluntary_ctxt_switches:")?.trim().parse().ok())
}

struct Flood {
    pushes: usize,
    /// Longest `push`, leaving out pushes during which this thread was preempted
    /// (that time is the OS's, not `push`'s). A push that sleeps on the lock is a
    /// voluntary switch and still counts. Without /proc every push counts.
    /// Wake-up preemption is deliberately not measured either: `push` wakes no
    /// thread (the worker polls), so a preemption during it is the scheduler's doing.
    max_push: Duration,
    preempted: usize,
}

/// Pushes `points`-point clouds at 200 Hz for `duration`; only the call itself is timed.
fn flood_clouds(inbox: &Inbox, duration: Duration, points: usize) -> Flood {
    let payload = cloud(points);
    let period = Duration::from_millis(5);
    let mut next = Instant::now();
    let mut f = Flood { pushes: 0, max_push: Duration::ZERO, preempted: 0 };
    for _ in 0..(duration.as_millis() / period.as_millis()) {
        let (id, bytes) = ("cloud".to_owned(), payload.clone());
        let before = preemptions();
        let t = Instant::now();
        inbox.push(id, bytes);
        let took = t.elapsed();
        f.pushes += 1;
        if before.is_some() && preemptions() != before {
            f.preempted += 1;
        } else {
            f.max_push = f.max_push.max(took);
        }
        next += period;
        if let Some(d) = next.checked_duration_since(Instant::now()) {
            thread::sleep(d);
        }
    }
    assert!(f.preempted * 5 < f.pushes, "{} of {} pushes preempted", f.preempted, f.pushes);
    f
}

/// A 3 x 2 trinary map in a fresh temp dir; returns (dir, yaml path).
fn temp_map(name: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("navbridge-zl-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let yaml = dir.join("map.yaml");
    std::fs::write(
        &yaml,
        "image: map.pgm\nmode: trinary\nresolution: 0.05\norigin: [-1.0, -2.0, 0.0]\nnegate: 0\noccupied_thresh: 0.65\nfree_thresh: 0.196\n",
    )
    .unwrap();
    let mut pgm = b"P5\n3 2\n255\n".to_vec();
    pgm.extend_from_slice(&[254, 0, 205, 254, 254, 0]);
    std::fs::write(dir.join("map.pgm"), pgm).unwrap();
    (dir, yaml)
}

#[test]
fn entities_arrive_in_map_frame() {
    let _serial = serial();
    let port = free_port();
    let mut bridge = Bridge::start(bridge_cfg(port));
    let sub = Subscriber::connect(port, Duration::ZERO);

    // The car at (1, 2) with yaw 0; one scan point straight ahead at 1 m, i.e. (1, 0)
    // in the body frame. Repeated until the bridge sees the subscriber and sends.
    let wanted = [nav_entities::ROBOT_POSE, nav_entities::ROBOT_FOOTPRINT, nav_entities::SENSORS_SCAN];
    let arrived = wait_until(Duration::from_secs(3), || {
        bridge.push("pose", odom(1.0, 2.0, 0.0));
        bridge.push("scan", scan(0.0, &[1.0]));
        thread::sleep(Duration::from_millis(40));
        wanted.iter().all(|e| sub.has(e))
    });
    assert!(arrived, "missing: {:?}", wanted.iter().filter(|e| !sub.has(e)).collect::<Vec<_>>());

    let bytes = sub.last(nav_entities::SENSORS_SCAN).unwrap();
    let (m, bin) = parse_message(&bytes).unwrap();
    assert_eq!(m.msg_type, "points3d");
    let pts = parse_points_xyz_f32(bin.unwrap(), m.count.unwrap()).unwrap();
    assert_eq!(pts.len(), 1);
    assert!((pts[0][0] - 2.0).abs() < 1e-3 && (pts[0][1] - 2.0).abs() < 1e-3, "{pts:?}");
    assert_eq!(m.data["color"], json!([255, 0, 0, 255]));

    let (m, _) = parse_message(&sub.last(nav_entities::ROBOT_POSE).unwrap()).unwrap();
    let pose: Transform3DData = serde_json::from_value(m.data).unwrap();
    assert_eq!(pose.translation, Some([1.0, 2.0, 0.0]));

    let (m, _) = parse_message(&sub.last(nav_entities::ROBOT_FOOTPRINT).unwrap()).unwrap();
    let fp: LineStrips3DData = serde_json::from_value(m.data).unwrap();
    assert_eq!(fp.strips.len(), 1);
    let strip = &fp.strips[0];
    assert_eq!(strip.len(), 5, "closed polygon");
    assert!((strip[0][0] - 1.09).abs() < 1e-5 && (strip[0][1] - 2.20).abs() < 1e-5, "{strip:?}");
    bridge.stop();
}

#[test]
fn late_subscriber_gets_static_map_within_six_seconds() {
    let _serial = serial();
    let port = free_port();
    let (dir, yaml) = temp_map("late");
    let cfg = BridgeConfig { map_yaml: Some(yaml), ..bridge_cfg(port) };
    let mut bridge = Bridge::start(cfg);

    thread::sleep(Duration::from_secs(2)); // the bridge runs alone for 2 s
    let sub = Subscriber::connect(port, Duration::ZERO);
    assert!(wait_until(Duration::from_secs(6), || sub.has(nav_entities::MAP_GRID)), "no map/grid in 6 s");

    let bytes = sub.last(nav_entities::MAP_GRID).unwrap();
    let (m, bin) = parse_message(&bytes).unwrap();
    assert_eq!(m.msg_type, "grid2d");
    let h: Grid2DHeader = serde_json::from_value(m.data).unwrap();
    assert_eq!((h.width, h.height, h.origin), (3, 2, [-1.0, -2.0, 0.0]));
    // PGM rows are flipped: row 0 of the grid is the bottom PGM row.
    assert_eq!(parse_grid2d(&h, bin.unwrap()), Some(&[0u8, 0, 100, 0, 100, 255][..]));
    bridge.stop();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn late_subscriber_gets_costmap_within_six_seconds() {
    let _serial = serial();
    let port = free_port();
    let mut bridge = Bridge::start(bridge_cfg(port));
    // 3 x 2 cells, header: f64 stamp, u32 w, u32 h, f32 resolution, origin x, y, yaw.
    let mut cm = 1.0f64.to_le_bytes().to_vec();
    cm.extend_from_slice(&[3, 0, 0, 0, 2, 0, 0, 0]);
    for v in [0.05f32, -1.0, -2.0, 0.0] {
        cm.extend_from_slice(&v.to_le_bytes());
    }
    cm.extend_from_slice(&[0, 1, 253, 254, 255, 7]);
    bridge.push("costmap", cm);

    thread::sleep(Duration::from_secs(2)); // the bridge runs alone for 2 s
    let sub = Subscriber::connect(port, Duration::ZERO);
    assert!(wait_until(Duration::from_secs(6), || sub.has(nav_entities::COSTMAP_GLOBAL)), "no costmap/global in 6 s");

    let bytes = sub.last(nav_entities::COSTMAP_GLOBAL).unwrap();
    let (m, bin) = parse_message(&bytes).unwrap();
    assert_eq!(m.msg_type, "grid2d");
    let h: Grid2DHeader = serde_json::from_value(m.data).unwrap();
    assert_eq!((h.width, h.height, h.origin, h.kind), (3, 2, [-1.0, -2.0, 0.0], GridKind::Costmap));
    assert_eq!(parse_grid2d(&h, bin.unwrap()), Some(&[0u8, 1, 253, 254, 255, 7][..]));
    bridge.stop();
}

#[test]
fn slow_subscriber_never_blocks_the_dora_side() {
    let _serial = serial();
    let port = free_port();
    // Every pushed cloud goes out (200 Hz x ~60 KB = ~12 MB/s, ~60 MB over the flood)
    // to a reader that takes 5 samples/s. Loopback TCP buffers hold at most ~10 MB
    // (Linux default tcp_wmem / tcp_rmem maxima, 4 + 6 MB), so the link is congested
    // after the first second.
    // 5000-point clouds fit one 64 KB Zenoh batch (no fragmentation) and keep the
    // debug-build worker well under one core, so a backlog can only mean blocking.
    let cfg = BridgeConfig { rates_hz: Rates { cloud: 200.0, ..Rates::default() }, ..bridge_cfg(port) };
    let mut bridge = Bridge::start(cfg);
    let sub = Subscriber::connect(port, Duration::from_millis(200));

    // Wait until the bridge is publishing to the subscriber, so the flood below
    // really goes to a stuck reader.
    let linked = wait_until(Duration::from_secs(5), || {
        bridge.push("pose", odom(0.0, 0.0, 0.0));
        thread::sleep(Duration::from_millis(50));
        sub.has(nav_entities::ROBOT_POSE)
    });
    assert!(linked, "subscriber never got robot/pose");

    let f = flood_clouds(&bridge.inbox, Duration::from_secs(5), 5000);
    println!(
        "slow subscriber: max push {:?} ({} of {} preempted, not timed), dropped {}, clouds received {}",
        f.max_push,
        f.preempted,
        f.pushes,
        bridge.inbox.dropped(),
        sub.count(nav_entities::SENSORS_CLOUD)
    );
    assert!(f.max_push < Duration::from_millis(1), "push took {:?}", f.max_push);
    // Drop-style congestion control: once the link is congested Zenoh drops the
    // clouds instead of waiting, so the worker is not held up by the stuck reader and
    // the inbox empties at once. With `CongestionControl::Block` each put would wait
    // for the reader (~5 samples/s) and the inbox would stay full.
    let drained = Instant::now();
    assert!(wait_until(Duration::from_secs(1), || bridge.inbox.is_empty()), "worker is stuck");
    println!("inbox drained {:?} after the flood", drained.elapsed());
    assert!(!bridge.worker.as_ref().unwrap().is_finished(), "worker died");
    // Disconnect the stuck reader before stopping: while it is connected, dropping the
    // bridge's Zenoh session blocks in close(2) on its lingering TCP socket (SO_LINGER
    // 10 s in zenoh-link-tcp; ~2.1 s here). That is Zenoh's teardown, not the worker.
    drop(sub);
    let took = bridge.stop();
    println!("slow subscriber: stop took {took:?} (reader disconnected first)");
    assert!(took < Duration::from_secs(1), "stop took {took:?}");
}

/// Output that takes 200 ms per message: a worker stuck behind its publisher.
struct StuckOutput;

impl Publish for StuckOutput {
    fn wants(&self, _: &str) -> bool {
        true
    }

    fn put(&mut self, _: &str, _: Vec<u8>) {
        thread::sleep(Duration::from_millis(200));
    }
}

/// The case the inbox is for: the worker stuck on output. The inbox overflows, the
/// oldest inputs are dropped and counted, and `push` stays fast.
#[test]
fn stuck_output_drops_oldest_and_push_stays_fast() {
    let _serial = serial();
    let inbox = Inbox::new(64);
    let stop = Arc::new(AtomicBool::new(false));
    let worker = spawn_worker(BridgeConfig::default(), inbox.clone(), Box::new(StuckOutput), stop.clone());
    inbox.push("pose".to_owned(), odom(0.0, 0.0, 0.0));

    let f = flood_clouds(&inbox, Duration::from_secs(2), 20_000);
    println!(
        "stuck output: max push {:?} ({} of {} preempted, not timed), dropped {}",
        f.max_push,
        f.preempted,
        f.pushes,
        inbox.dropped()
    );
    assert!(f.max_push < Duration::from_millis(1), "push took {:?}", f.max_push);
    assert!(inbox.dropped() > 0, "nothing was dropped");
    assert!(!worker.is_finished(), "worker died");
    stop.store(true, Ordering::Relaxed);
    worker.join().unwrap();
}

#[test]
fn no_subscriber_no_encoding() {
    let _serial = serial();
    let port = free_port();
    let (dir, yaml) = temp_map("nosub");
    let cfg = BridgeConfig { map_yaml: Some(yaml), ..bridge_cfg(port) };
    let before = encoded_count();
    let mut bridge = Bridge::start(cfg);

    bridge.push("pose", odom(1.0, 2.0, 0.0));
    bridge.push("goal", br#"{"id":"C1","x":1.0,"y":0.0,"yaw":0.0}"#.to_vec());
    bridge.push("gate_state", br#"{"armed":true,"estop":false,"reason":""}"#.to_vec());
    let payload = cloud(20_000);
    for _ in 0..100 {
        bridge.push("cloud", payload.clone());
        thread::sleep(Duration::from_millis(5));
    }
    assert!(wait_until(Duration::from_secs(3), || bridge.inbox.is_empty()), "worker did not drain");
    bridge.stop(); // joins: the last item popped has been handled
    assert_eq!(encoded_count() - before, 0);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn stop_flag_exits_within_one_second() {
    let _serial = serial();
    let port = free_port();
    let mut bridge = Bridge::start(bridge_cfg(port));
    let sub = Subscriber::connect(port, Duration::ZERO);
    assert!(wait_until(Duration::from_secs(3), || {
        bridge.push("pose", odom(0.0, 0.0, 0.0));
        thread::sleep(Duration::from_millis(20));
        sub.has(nav_entities::ROBOT_POSE)
    }));
    let took = bridge.stop();
    println!("stop took {took:?}");
    assert!(took < Duration::from_secs(1), "stop took {took:?}");
}

// ---- command channel (the whole bridge: session, queryable, lanes, gate slot) ----

const TOKEN: &str = "zl-t0ken-value";
const ARMED: &[u8] = br#"{"armed":true,"estop":false,"reason":""}"#;

/// Fake nav_command on `127.0.0.1:<ephemeral>`: answers `{"ok":true}` and hands each
/// datagram (as JSON) to `rx`.
struct FakeNav {
    addr: SocketAddr,
    rx: mpsc::Receiver<serde_json::Value>,
}

fn fake_nav() -> FakeNav {
    let sock = UdpSocket::bind("127.0.0.1:0").unwrap();
    let addr = sock.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let mut buf = [0u8; 2048];
        while let Ok((n, from)) = sock.recv_from(&mut buf) {
            let v = serde_json::from_slice(&buf[..n]).unwrap_or(serde_json::Value::Null);
            if tx.send(v).is_err() {
                return;
            }
            let _ = sock.send_to(br#"{"ok":true,"outputs":[]}"#, from);
        }
    });
    FakeNav { addr, rx }
}

/// `<tmp>/cmd_token` holding `TOKEN` and a newline, with `mode`.
fn token_file(name: &str, mode: u32) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("navbridge-zl-tok-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("cmd_token");
    std::fs::write(&path, format!("{TOKEN}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode)).unwrap();
    path
}

/// Listen on `port`, forward commands to `forward`; without `token` the token file does
/// not exist.
fn cmd_cfg(port: u16, forward: SocketAddr, goal_enabled: bool, token: Option<&Path>) -> BridgeConfig {
    let token_file = token.map_or_else(|| "/nonexistent/navbridge-zl/cmd_token".to_owned(), |p| p.display().to_string());
    let cmd = CmdConfig { goal_enabled, token_file, forward: forward.to_string(), ..CmdConfig::default() };
    BridgeConfig { cmd, ..bridge_cfg(port) }
}

fn start_node(cfg: BridgeConfig) -> NodeBridge {
    NodeBridge::start(cfg, LogSources::default()).expect("bridge start")
}

fn gate_input(bytes: &[u8]) -> NodeEvent {
    NodeEvent::Input { id: "gate_state".into(), data: Some(bytes.to_vec()) }
}

/// A Studio-like client session connected only to the bridge.
fn client(port: u16) -> zenoh::Session {
    let mut config = zenoh::Config::default();
    config.insert_json5("mode", r#""client""#).unwrap();
    config.insert_json5("connect/endpoints", &format!(r#"["tcp/127.0.0.1:{port}"]"#)).unwrap();
    config.insert_json5("listen/endpoints", "[]").unwrap();
    config.insert_json5("scouting/multicast/enabled", "false").unwrap();
    zenoh::open(config).wait().expect("client session")
}

/// One query on `dviz/tracer/cmd`: the first reply's ack, or `None` when the query ends
/// without one (no queryable, or `timeout`).
fn ask(session: &zenoh::Session, json: &str, timeout: Duration) -> Option<CmdAck> {
    let replies = session.get(cmd_key("dviz", "tracer")).payload(json.to_owned()).timeout(timeout).wait().ok()?;
    let reply = replies.recv().ok()?;
    let sample = reply.result().ok()?;
    Some(parse_cmd_ack(&sample.payload().to_bytes()).expect("reply is a CmdAck"))
}

/// Waits until the bridge answers a harmless command (`arm` is never allowed).
fn ready(session: &zenoh::Session) -> bool {
    wait_until(Duration::from_secs(5), || {
        ask(session, r#"{"cmd":"arm","id":"zl-ready"}"#, Duration::from_millis(500))
            .is_some_and(|a| a.result == CmdResult::NotAllowed)
    })
}

fn goal_json(id: &str, token: &str) -> String {
    format!(r#"{{"cmd":"goal","id":"{id}","token":"{token}","x":1.5,"y":-0.5,"yaw":0.25}}"#)
}

fn last_status(sub: &Subscriber) -> NavStatus {
    let (m, _) = parse_message(&sub.last(nav_entities::NAV_STATUS).expect("no nav/status")).unwrap();
    serde_json::from_value(m.data).unwrap()
}

#[test]
fn query_round_trip_goal_ok() {
    let _serial = serial();
    let port = free_port();
    let nav = fake_nav();
    let token = token_file("roundtrip", 0o600);
    let mut bridge = start_node(cmd_cfg(port, nav.addr, true, Some(&token)));
    let session = client(port);
    assert!(ready(&session), "the bridge never answered a command query");
    let mut err = Vec::new();
    bridge.on_event(gate_input(ARMED), &mut err);

    let ack = ask(&session, &goal_json("zl-goal-1", TOKEN), Duration::from_secs(3)).expect("no reply to the goal");
    assert_eq!(ack.result, CmdResult::Ok, "{ack:?}");
    assert_eq!((ack.id.as_str(), ack.cmd.as_str()), ("zl-goal-1", "goal"));
    let sent = nav.rx.recv_timeout(Duration::from_secs(1)).expect("nav_command got nothing");
    assert_eq!(sent["cmd"], "goal");
    assert_eq!(sent["id"], "zl-goal-1");
    assert_eq!((sent["x"].as_f64(), sent["y"].as_f64(), sent["yaw"].as_f64()), (Some(1.5), Some(-0.5), Some(0.25)));
    assert!(sent.get("token").is_none(), "{sent}");
    assert!(err.is_empty(), "{}", String::from_utf8_lossy(&err));
    assert!(bridge.wait(Instant::now() + Duration::from_secs(2)), "bridge did not stop");
}

#[test]
fn estop_works_with_channel_closed() {
    let _serial = serial();
    let port = free_port();
    let nav = fake_nav();
    let mut bridge = start_node(cmd_cfg(port, nav.addr, false, None));
    let session = client(port);
    assert!(ready(&session), "the bridge never answered a command query");

    let ack = ask(&session, r#"{"cmd":"estop","id":"zl-estop-1"}"#, Duration::from_secs(3)).expect("no reply to estop");
    assert_eq!(ack.result, CmdResult::Ok, "{ack:?}");
    assert_eq!(nav.rx.recv_timeout(Duration::from_secs(1)).expect("estop not forwarded")["cmd"], "estop");

    bridge.on_event(gate_input(ARMED), &mut Vec::new());
    let ack = ask(&session, &goal_json("zl-goal-2", TOKEN), Duration::from_secs(3)).expect("no reply to the goal");
    assert_eq!(ack.result, CmdResult::Disabled, "{ack:?}");
    assert!(nav.rx.recv_timeout(Duration::from_millis(200)).is_err(), "the goal reached nav_command");
    assert!(bridge.wait(Instant::now() + Duration::from_secs(2)), "bridge did not stop");
}

/// Output whose every put panics: the worker thread dies on its first send.
struct PanicsOnPut;

impl Publish for PanicsOnPut {
    fn wants(&self, _: &str) -> bool {
        true
    }

    fn put(&mut self, _: &str, _: Vec<u8>) {
        panic!("test: worker output failed");
    }
}

#[test]
fn estop_forwarded_after_worker_died() {
    let _serial = serial();
    let port = free_port();
    let nav = fake_nav();
    let cfg = cmd_cfg(port, nav.addr, false, None);
    let session = open_session(&cfg).expect("bridge session");
    let mut bridge = NodeBridge::start_with(cfg, LogSources::default(), session, Box::new(PanicsOnPut)).expect("start");
    // The first tick puts nav/status, so the worker dies at once; the bridge says so once.
    let mut err = Vec::new();
    let died = wait_until(Duration::from_secs(3), || {
        bridge.on_event(NodeEvent::Other, &mut err);
        !err.is_empty()
    });
    assert!(died, "the worker did not die");
    assert!(String::from_utf8_lossy(&err).contains("worker thread has stopped"));

    let client = client(port);
    assert!(ready(&client), "no command query answered after the worker died");
    let ack = ask(&client, r#"{"cmd":"estop","id":"zl-estop-2"}"#, Duration::from_secs(3)).expect("no reply to estop");
    assert_eq!(ack.result, CmdResult::Ok, "{ack:?}");
    assert_eq!(nav.rx.recv_timeout(Duration::from_secs(1)).expect("estop not forwarded")["cmd"], "estop");
    assert!(bridge.wait(Instant::now() + Duration::from_secs(2)), "bridge did not stop");
}

/// `cmd_goal_enabled` in nav/status: the switch on and a usable token; a token file
/// that group or others can read is not usable.
#[test]
fn status_reports_cmd_goal_enabled() {
    let _serial = serial();
    for (mode, want) in [(0o600, true), (0o644, false)] {
        let port = free_port();
        let nav = fake_nav();
        let token = token_file(&format!("status{mode:o}"), mode);
        let mut bridge = start_node(cmd_cfg(port, nav.addr, true, Some(&token)));
        let sub = Subscriber::connect(port, Duration::ZERO);
        assert!(wait_until(Duration::from_secs(3), || sub.has(nav_entities::NAV_STATUS)), "no nav/status");
        assert_eq!(last_status(&sub).cmd_goal_enabled, want, "token file mode {mode:o}");
        assert!(bridge.wait(Instant::now() + Duration::from_secs(2)), "bridge did not stop");
    }
}

/// Ruling R7: `cmd.forward` that is not `127.0.0.1:<port>` turns the command channel
/// off (no queryable, `cmd_goal_enabled` false) but the bridge keeps publishing.
#[test]
fn bad_forward_declares_no_queryable_and_keeps_publishing() {
    let _serial = serial();
    let port = free_port();
    let token = token_file("badforward", 0o600);
    let mut cfg = cmd_cfg(port, "127.0.0.1:1".parse().unwrap(), true, Some(&token));
    cfg.cmd.forward = "192.0.2.1:19997".into(); // TEST-NET-1: never routed, and not loopback
    let mut bridge = start_node(cfg);
    let sub = Subscriber::connect(port, Duration::ZERO);
    let published = wait_until(Duration::from_secs(3), || {
        bridge.on_event(NodeEvent::Input { id: "pose".into(), data: Some(odom(1.0, 2.0, 0.0)) }, &mut Vec::new());
        thread::sleep(Duration::from_millis(20));
        sub.has(nav_entities::ROBOT_POSE) && sub.has(nav_entities::NAV_STATUS)
    });
    assert!(published, "the bridge stopped publishing data");
    assert!(!last_status(&sub).cmd_goal_enabled);
    let session = client(port);
    let ack = ask(&session, r#"{"cmd":"estop","id":"zl-estop-3"}"#, Duration::from_secs(1));
    assert!(ack.is_none(), "a queryable answered: {ack:?}");
    assert!(bridge.wait(Instant::now() + Duration::from_secs(2)), "bridge did not stop");
}

/// `wait` closes everything within its deadline; afterwards nobody answers on `cmd`.
#[test]
fn stop_closes_within_deadline() {
    let _serial = serial();
    let port = free_port();
    let nav = fake_nav();
    let mut bridge = start_node(cmd_cfg(port, nav.addr, false, None));
    let session = client(port);
    assert!(ready(&session), "the bridge never answered a command query");

    let t = Instant::now();
    assert!(bridge.wait(t + Duration::from_secs(1)), "not closed within 1 s");
    println!("bridge closed in {:?}", t.elapsed());
    let ack = ask(&session, r#"{"cmd":"estop","id":"zl-estop-4"}"#, Duration::from_secs(1));
    assert!(ack.is_none(), "answered after stop: {ack:?}");
    assert!(nav.rx.recv_timeout(Duration::from_millis(200)).is_err(), "estop forwarded after stop");
}
