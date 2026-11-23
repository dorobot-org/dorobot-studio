//! Real Zenoh over loopback TCP: the bridge (`zenoh_publisher` + `spawn_worker`)
//! listens on its own free `tcp/127.0.0.1:<port>` with multicast scouting off, and
//! the test subscriber is a client that connects only to that endpoint, so the
//! tests never see the LAN (or the rmw_zenohd router on 7447).
//!
//! The tests run one at a time (`serial`): `encoded_count` is process-wide, and the
//! push-latency test should not share the CPU with another bridge.

use dorobot_nav_bridge::config::BridgeConfig;
use dorobot_nav_bridge::publish::{zenoh_publisher, Publish};
use dorobot_nav_bridge::worker::{encoded_count, spawn_worker, Inbox};
use dviz_protocol::{
    nav_entities, parse_grid2d, parse_message, parse_points_xyz_f32, Grid2DHeader,
    LineStrips3DData, Transform3DData,
};
use serde_json::json;
use std::{
    net::TcpListener,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard,
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
    max_push: Duration,
    preempted: usize,
}

/// Pushes 20 000-point clouds at 200 Hz for `duration`; only the call itself is timed.
fn flood_clouds(inbox: &Inbox, duration: Duration) -> Flood {
    let payload = cloud(20_000);
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
    assert!(f.preempted * 2 < f.pushes, "{} of {} pushes preempted", f.preempted, f.pushes);
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
fn slow_subscriber_never_blocks_the_dora_side() {
    let _serial = serial();
    let port = free_port();
    let mut bridge = Bridge::start(bridge_cfg(port));
    let sub = Subscriber::connect(port, Duration::from_millis(200));

    // Wait until the bridge is publishing to the subscriber, so the flood below
    // really goes to a stuck reader.
    let linked = wait_until(Duration::from_secs(5), || {
        bridge.push("pose", odom(0.0, 0.0, 0.0));
        thread::sleep(Duration::from_millis(50));
        sub.has(nav_entities::ROBOT_POSE)
    });
    assert!(linked, "subscriber never got robot/pose");

    let f = flood_clouds(&bridge.inbox, Duration::from_secs(5));
    println!(
        "slow subscriber: max push {:?} ({} of {} preempted, not timed), dropped {}, clouds received {}",
        f.max_push,
        f.preempted,
        f.pushes,
        bridge.inbox.dropped(),
        sub.count(nav_entities::SENSORS_CLOUD)
    );
    assert!(f.max_push < Duration::from_millis(1), "push took {:?}", f.max_push);
    // With drop-style congestion control the worker is not held up by the stuck
    // reader either: it keeps up with the flood and the inbox empties at once.
    assert!(wait_until(Duration::from_secs(1), || bridge.inbox.is_empty()), "worker is stuck");
    assert!(!bridge.worker.as_ref().unwrap().is_finished(), "worker died");
    assert!(bridge.stop() < Duration::from_secs(1));
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

    let f = flood_clouds(&inbox, Duration::from_secs(2));
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
