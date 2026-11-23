//! Bounded inbox (filled by the Dora event loop) and the worker thread that turns
//! inputs into dviz messages: parse, place in the map frame, rate-limit, encode, publish.
//! The worker also forwards node logs and the dataflow graph ([`LogForward`]).

use crate::config::{BridgeConfig, InputFormat};
use crate::encode::{enc_goal, enc_grid, enc_points, enc_pose, enc_status, enc_strip};
use crate::formats::{self, GoalMsg, Odom};
use crate::geom::{footprint_in_map, scan_points, to_map};
use crate::logs::{LogForward, LogSources};
use crate::map::{load_occupancy, load_pcd_xyz, voxel_downsample};
use crate::publish::Publish;
use crate::rate::RateLimiter;
use crate::status::StatusBuilder;
use crate::trail::Trail;
use dviz_protocol::nav_entities as ent;
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Bounded FIFO from the Dora event loop to the worker. When full, the oldest item
/// is dropped and counted. The worker polls it and nothing wakes the worker, so an
/// uncontended push makes no system call and never hands its CPU to the worker.
pub struct Inbox {
    cap: usize,
    queue: Mutex<VecDeque<(String, Vec<u8>)>>,
    dropped: AtomicU64,
}

impl Inbox {
    /// `cap` is at least 1.
    pub fn new(cap: usize) -> Arc<Inbox> {
        let cap = cap.max(1);
        Arc::new(Inbox {
            cap,
            queue: Mutex::new(VecDeque::with_capacity(cap)),
            dropped: AtomicU64::new(0),
        })
    }

    /// O(1) and never waits for the worker's processing: both sides hold the lock
    /// only for one queue operation (the buffer never grows past `cap`), and an
    /// evicted item is freed after unlocking.
    pub fn push(&self, input_id: String, bytes: Vec<u8>) {
        let evicted = {
            let mut q = self.lock();
            let evicted = if q.len() >= self.cap { q.pop_front() } else { None };
            q.push_back((input_id, bytes));
            evicted
        };
        if evicted.is_some() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Items dropped because the inbox was full.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Nothing is waiting for the worker.
    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    fn pop(&self) -> Option<(String, Vec<u8>)> {
        self.lock().pop_front()
    }

    fn lock(&self) -> MutexGuard<'_, VecDeque<(String, Vec<u8>)>> {
        self.queue.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// How long the worker sleeps when the inbox is empty; also bounds how late it sees
/// the stop flag (plus the item in hand).
const POLL: Duration = Duration::from_millis(10);
/// Periodic sends (trail, goal, status, maps) are checked this often.
const TICK: Duration = Duration::from_millis(50);
/// Resend rates (Hz): grid map every 5 s, PCD map every 10 s, goal every 2 s.
const GRID_HZ: f32 = 0.2;
const MAP_CLOUD_HZ: f32 = 0.1;
const GOAL_HZ: f32 = 0.5;

static ENCODED: AtomicU64 = AtomicU64::new(0);
static BAD_INPUTS: AtomicU64 = AtomicU64::new(0);

/// Messages encoded by all workers in this process.
pub fn encoded_count() -> u64 {
    ENCODED.load(Ordering::Relaxed)
}

/// Inputs that failed to parse, all workers in this process.
pub fn bad_input_count() -> u64 {
    BAD_INPUTS.load(Ordering::Relaxed)
}

/// Starts the worker thread: takes inputs from `inbox`, parses, rate-limits, encodes
/// and hands messages to `publisher`; resends the grid map every 5 s and the PCD map
/// every 10 s. `stop` is checked between items and at least every `POLL`; on exit the
/// publisher (and its Zenoh session) is dropped. No node logs or graph: see
/// [`spawn_worker_with_logs`].
pub fn spawn_worker(
    cfg: BridgeConfig,
    inbox: Arc<Inbox>,
    publisher: Box<dyn Publish>,
    stop: Arc<AtomicBool>,
) -> JoinHandle<()> {
    spawn_worker_with_logs(cfg, inbox, publisher, stop, LogSources::default())
}

/// [`spawn_worker`] that also forwards the node logs and the dataflow graph named in
/// `sources`, and its own warnings (e.g. a map it could not load), as `logs`,
/// `nodes` and `graph` messages.
pub fn spawn_worker_with_logs(
    cfg: BridgeConfig,
    inbox: Arc<Inbox>,
    publisher: Box<dyn Publish>,
    stop: Arc<AtomicBool>,
    sources: LogSources,
) -> JoinHandle<()> {
    thread::Builder::new()
        .name("nav-bridge-worker".into())
        .spawn(move || {
            let mut worker = Worker::new(cfg, publisher, sources);
            let mut next_tick = Instant::now();
            while !stop.load(Ordering::Relaxed) {
                let now = Instant::now();
                if now >= next_tick {
                    worker.tick(now);
                    next_tick = now + TICK;
                }
                match inbox.pop() {
                    Some((id, bytes)) => worker.handle(&id, &bytes, now),
                    None => thread::sleep(POLL),
                }
            }
        })
        .expect("spawn nav-bridge worker thread")
}

/// Encodes and publishes only if someone subscribes to `entity`.
fn send(out: &mut dyn Publish, entity: &'static str, encode: impl FnOnce() -> (&'static str, Vec<u8>)) {
    if out.wants(entity) {
        let (entity, bytes) = encode();
        ENCODED.fetch_add(1, Ordering::Relaxed);
        out.put(entity, bytes);
    }
}

type Encoder = Box<dyn FnOnce() -> (&'static str, Vec<u8>) + Send>;

/// A static map message: encoded on its first send, then reused.
enum StaticMsg {
    Missing,
    Pending(Encoder),
    Ready(&'static str, Vec<u8>),
}

impl StaticMsg {
    fn send(&mut self, out: &mut dyn Publish) {
        *self = match std::mem::replace(self, StaticMsg::Missing) {
            StaticMsg::Pending(encode) => {
                let (entity, bytes) = encode();
                ENCODED.fetch_add(1, Ordering::Relaxed);
                StaticMsg::Ready(entity, bytes)
            }
            other => other,
        };
        if let StaticMsg::Ready(entity, bytes) = self {
            out.put(entity, bytes.clone());
        }
    }
}

/// A map that cannot be used is reported on stderr (Dora's log) and as a WARN log
/// entry for Studio; the bridge goes on without it.
fn map_problem(logs: &mut LogForward, entity: &str, path: &std::path::Path, problem: &str) -> StaticMsg {
    let text = format!("{entity} not published: {}: {problem}", path.display());
    eprintln!("nav-bridge: {text}");
    logs.warn(text);
    StaticMsg::Missing
}

fn load_grid(cfg: &BridgeConfig, logs: &mut LogForward) -> StaticMsg {
    let Some(path) = &cfg.map_yaml else { return StaticMsg::Missing };
    match load_occupancy(path) {
        Ok((header, cells)) => StaticMsg::Pending(Box::new(move || enc_grid(&header, &cells))),
        Err(e) => map_problem(logs, ent::MAP_GRID, path, &e),
    }
}

fn load_map_cloud(cfg: &BridgeConfig, logs: &mut LogForward) -> StaticMsg {
    let Some(path) = &cfg.map_pcd else { return StaticMsg::Missing };
    match load_pcd_xyz(path) {
        Ok(points) => {
            let points = voxel_downsample(&points, cfg.map_cloud_voxel_m, cfg.map_cloud_max_points);
            if points.is_empty() {
                return map_problem(logs, ent::MAP_CLOUD, path, "no finite points");
            }
            let color = cfg.colors.map_cloud;
            StaticMsg::Pending(Box::new(move || enc_points(ent::MAP_CLOUD, &points, Some(color))))
        }
        Err(e) => map_problem(logs, ent::MAP_CLOUD, path, &e),
    }
}

struct Worker {
    cfg: BridgeConfig,
    out: Box<dyn Publish>,
    /// Latest `map -> base_footprint` pose; body-frame data is placed with it.
    pose: Option<Odom>,
    goal: Option<GoalMsg>,
    trail: Trail,
    status: StatusBuilder,
    grid: StaticMsg,
    map_cloud: StaticMsg,
    pose_rate: RateLimiter,
    scan_rate: RateLimiter,
    cloud_rate: RateLimiter,
    trail_rate: RateLimiter,
    status_rate: RateLimiter,
    goal_rate: RateLimiter,
    grid_rate: RateLimiter,
    map_cloud_rate: RateLimiter,
    logs: LogForward,
}

impl Worker {
    fn new(cfg: BridgeConfig, out: Box<dyn Publish>, sources: LogSources) -> Worker {
        let mut logs = LogForward::new(sources, cfg.log_forward_per_node_hz);
        let (grid, map_cloud) = (load_grid(&cfg, &mut logs), load_map_cloud(&cfg, &mut logs));
        let r = &cfg.rates_hz;
        Worker {
            out,
            pose: None,
            goal: None,
            trail: Trail::new(cfg.trail_min_step_m, cfg.trail_max_points),
            status: StatusBuilder::default(),
            grid,
            map_cloud,
            pose_rate: RateLimiter::new(r.pose),
            scan_rate: RateLimiter::new(r.scan),
            cloud_rate: RateLimiter::new(r.cloud),
            trail_rate: RateLimiter::new(r.trail),
            status_rate: RateLimiter::new(r.status),
            goal_rate: RateLimiter::new(GOAL_HZ),
            grid_rate: RateLimiter::new(GRID_HZ),
            map_cloud_rate: RateLimiter::new(MAP_CLOUD_HZ),
            logs,
            cfg,
        }
    }

    /// One input. Unknown ids are ignored; bad data is skipped and counted.
    fn handle(&mut self, id: &str, b: &[u8], now: Instant) {
        let Some(format) = self.cfg.inputs.get(id).copied() else { return };
        let result = match format {
            InputFormat::Odometry => self.on_odom(b, now),
            InputFormat::Path => self.on_path(b),
            InputFormat::Goal => self.on_goal(b),
            InputFormat::GoalStatus => self.on_goal_status(b),
            InputFormat::LaserScan => self.on_scan(b, now),
            InputFormat::CloudXyzi => self.on_cloud(b, now),
            InputFormat::Trust => self.on_state(b, StatusBuilder::on_trust),
            InputFormat::RefineState => self.on_state(b, StatusBuilder::on_refine),
            InputFormat::GateState => self.on_state(b, StatusBuilder::on_gate),
            InputFormat::ChassisState => self.on_state(b, StatusBuilder::on_chassis),
        };
        if result.is_err() {
            BAD_INPUTS.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Periodic sends, each only while someone subscribes; node logs and the graph.
    fn tick(&mut self, now: Instant) {
        self.logs.tick(now, &mut *self.out);
        let out = &mut *self.out;
        if !matches!(self.grid, StaticMsg::Missing) && out.wants(ent::MAP_GRID) && self.grid_rate.ready(now) {
            self.grid.send(out);
        }
        if !matches!(self.map_cloud, StaticMsg::Missing)
            && out.wants(ent::MAP_CLOUD)
            && self.map_cloud_rate.ready(now)
        {
            self.map_cloud.send(out);
        }
        if out.wants(ent::NAV_TRAIL) && self.trail_rate.ready(now) {
            let (trail, color) = (self.trail.points(), self.cfg.colors.trail);
            send(out, ent::NAV_TRAIL, || enc_strip(ent::NAV_TRAIL, trail, color));
        }
        if out.wants(ent::NAV_GOAL) && self.goal_rate.ready(now) {
            let (goal, color) = (self.goal.as_ref(), self.cfg.colors.goal);
            send(out, ent::NAV_GOAL, || enc_goal(goal, color));
        }
        if out.wants(ent::NAV_STATUS) && self.status_rate.ready(now) {
            let (status, _) = self.status.build();
            send(out, ent::NAV_STATUS, || enc_status(&status));
        }
    }

    fn on_odom(&mut self, b: &[u8], now: Instant) -> Result<(), String> {
        let o = formats::parse_odometry(b)?;
        self.status.on_odom(&o);
        self.trail.push([o.x, o.y]);
        let out = &mut *self.out;
        if (out.wants(ent::ROBOT_POSE) || out.wants(ent::ROBOT_FOOTPRINT)) && self.pose_rate.ready(now) {
            send(out, ent::ROBOT_POSE, || enc_pose(&o));
            let (poly, color) = (&self.cfg.footprint, self.cfg.colors.footprint);
            send(out, ent::ROBOT_FOOTPRINT, || {
                let pts: Vec<[f32; 2]> = footprint_in_map(&o, poly).iter().map(|p| [p[0], p[1]]).collect();
                enc_strip(ent::ROBOT_FOOTPRINT, &pts, color)
            });
        }
        self.pose = Some(o);
        Ok(())
    }

    /// Path points are already in the map frame; an error path clears the plan.
    fn on_path(&mut self, b: &[u8]) -> Result<(), String> {
        if !self.out.wants(ent::NAV_PLAN) {
            return Ok(());
        }
        let path = formats::parse_path(b)?;
        let pts: Vec<[f32; 2]> = if path.error {
            Vec::new()
        } else {
            path.points.iter().map(|p| [p[0] as f32, p[1] as f32]).collect()
        };
        let color = self.cfg.colors.plan;
        send(&mut *self.out, ent::NAV_PLAN, || enc_strip(ent::NAV_PLAN, &pts, color));
        Ok(())
    }

    /// A new goal clears the trail and is sent at once.
    fn on_goal(&mut self, b: &[u8]) -> Result<(), String> {
        let goal = formats::parse_goal(b)?;
        self.trail.on_goal(&goal.id);
        if self.goal.as_ref() != Some(&goal) {
            self.goal = Some(goal);
            self.send_goal();
        }
        Ok(())
    }

    /// A finished current goal (SUCCEEDED, ABORTED, CANCELED) is no longer drawn.
    fn on_goal_status(&mut self, b: &[u8]) -> Result<(), String> {
        let (id, state, reason) = formats::parse_goal_status(b)?;
        self.status.on_goal_status(&id, &state, &reason);
        if state != "ACTIVE" && self.goal.as_ref().is_some_and(|g| g.id == id) {
            self.goal = None;
            self.send_goal();
        }
        self.send_status_if_changed();
        Ok(())
    }

    fn on_state(&mut self, b: &[u8], apply: fn(&mut StatusBuilder, &Value)) -> Result<(), String> {
        let v = formats::parse_json(b)?;
        apply(&mut self.status, &v);
        self.send_status_if_changed();
        Ok(())
    }

    /// Scan points (body frame, z = 0) placed in the map frame with the latest pose.
    fn on_scan(&mut self, b: &[u8], now: Instant) -> Result<(), String> {
        let Some(pose) = &self.pose else { return Ok(()) };
        if !self.out.wants(ent::SENSORS_SCAN) || !self.scan_rate.ready(now) {
            return Ok(());
        }
        let scan = formats::parse_scan(b)?;
        let pts: Vec<[f32; 3]> = scan_points(&scan)
            .into_iter()
            .map(|p| {
                let m = to_map(pose, p);
                [m[0] as f32, m[1] as f32, 0.0]
            })
            .collect();
        let color = self.cfg.colors.scan;
        send(&mut *self.out, ent::SENSORS_SCAN, || enc_points(ent::SENSORS_SCAN, &pts, Some(color)));
        Ok(())
    }

    /// Body-frame cloud: non-finite points dropped, cut to `cloud_max_points`, then
    /// x, y placed in the map frame with the latest pose (z kept: flat floor, 2D pose).
    fn on_cloud(&mut self, b: &[u8], now: Instant) -> Result<(), String> {
        let Some(pose) = &self.pose else { return Ok(()) };
        if !self.out.wants(ent::SENSORS_CLOUD) || !self.cloud_rate.ready(now) {
            return Ok(());
        }
        let (_, raw) = formats::parse_cloud_xyzi(b)?;
        let body: Vec<[f32; 3]> = raw
            .iter()
            .filter(|p| p[..3].iter().all(|v| v.is_finite()))
            .map(|p| [p[0], p[1], p[2]])
            .collect();
        let pts: Vec<[f32; 3]> = voxel_downsample(&body, 0.0, self.cfg.cloud_max_points)
            .into_iter()
            .map(|p| {
                let m = to_map(pose, [f64::from(p[0]), f64::from(p[1])]);
                [m[0] as f32, m[1] as f32, p[2]]
            })
            .collect();
        send(&mut *self.out, ent::SENSORS_CLOUD, || enc_points(ent::SENSORS_CLOUD, &pts, None));
        Ok(())
    }

    fn send_goal(&mut self) {
        let (goal, color) = (self.goal.as_ref(), self.cfg.colors.goal);
        send(&mut *self.out, ent::NAV_GOAL, || enc_goal(goal, color));
    }

    /// Status goes out at once when it changed, besides the periodic send.
    fn send_status_if_changed(&mut self) {
        if self.out.wants(ent::NAV_STATUS) {
            let (status, changed) = self.status.build();
            if changed {
                send(&mut *self.out, ent::NAV_STATUS, || enc_status(&status));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logs;
    use dviz_protocol::{parse_message, parse_points_xyz_f32, Arrows3DData, LogData};
    use std::path::PathBuf;

    type Sent = Vec<(String, Vec<u8>)>;

    /// Wants everything, records every put.
    #[derive(Clone, Default)]
    struct Fake(Arc<Mutex<Sent>>);

    impl Publish for Fake {
        fn wants(&self, _: &str) -> bool {
            true
        }
        fn put(&mut self, entity: &str, bytes: Vec<u8>) {
            self.0.lock().unwrap().push((entity.to_owned(), bytes));
        }
    }

    impl Fake {
        /// Messages put for `entity` since the last call, oldest first.
        fn take(&self, entity: &str) -> Vec<Vec<u8>> {
            let mut all = self.0.lock().unwrap();
            let (hit, rest): (Sent, Sent) = all.drain(..).partition(|(e, _)| e == entity);
            *all = rest;
            hit.into_iter().map(|(_, b)| b).collect()
        }
    }

    fn worker(cfg: BridgeConfig) -> (Worker, Fake) {
        let fake = Fake::default();
        (Worker::new(cfg, Box::new(fake.clone()), LogSources::default()), fake)
    }

    fn odom(x: f64, y: f64, yaw: f64) -> Vec<u8> {
        let (s, c) = (yaw / 2.0).sin_cos();
        serde_json::json!({
            "header": {"timestamp": 1.0},
            "pose": {"position": {"x": x, "y": y}, "orientation": {"x": 0.0, "y": 0.0, "z": s, "w": c}},
            "twist": {"linear": {"x": 0.0}, "angular": {"z": 0.0}}
        })
        .to_string()
        .into_bytes()
    }

    fn tmpdir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("navbridge-worker-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn inbox_drops_oldest_first() {
        let inbox = Inbox::new(3);
        for (i, id) in ["a", "b", "c", "d", "e"].into_iter().enumerate() {
            inbox.push(id.to_owned(), vec![i as u8]);
        }
        assert_eq!(inbox.dropped(), 2);
        let ids: Vec<String> = std::iter::from_fn(|| inbox.pop()).map(|(id, _)| id).collect();
        assert_eq!(ids, ["c", "d", "e"]);
        assert!(inbox.is_empty());
    }

    #[test]
    fn static_maps_resend_every_5_and_10_s() {
        let d = tmpdir("static");
        std::fs::write(
            d.join("map.yaml"),
            "image: map.pgm\nresolution: 0.05\norigin: [0.0, 0.0, 0.0]\nnegate: 0\noccupied_thresh: 0.65\nfree_thresh: 0.196\n",
        )
        .unwrap();
        std::fs::write(d.join("map.pgm"), b"P5\n2 1\n255\n\x00\xfe").unwrap();
        std::fs::write(
            d.join("map.pcd"),
            "VERSION 0.7\nFIELDS x y z\nSIZE 4 4 4\nTYPE F F F\nCOUNT 1 1 1\nWIDTH 2\nHEIGHT 1\nPOINTS 2\nDATA ascii\n0 0 0\n1 1 1\n",
        )
        .unwrap();
        let cfg = BridgeConfig {
            map_yaml: Some(d.join("map.yaml")),
            map_pcd: Some(d.join("map.pcd")),
            ..BridgeConfig::default()
        };
        let (mut w, out) = worker(cfg);
        let t0 = Instant::now();
        let at = |s: f32| t0 + Duration::from_secs_f32(s);
        let mut counts = vec![];
        for s in [0.0, 4.9, 5.0, 9.9, 10.0] {
            w.tick(at(s));
            counts.push((out.take(ent::MAP_GRID).len(), out.take(ent::MAP_CLOUD).len()));
        }
        assert_eq!(counts, [(1, 1), (0, 0), (1, 0), (0, 0), (1, 1)]);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn missing_map_is_not_fatal() {
        let cfg = BridgeConfig {
            map_yaml: Some("/nonexistent/map.yaml".into()),
            map_pcd: Some("/nonexistent/map.pcd".into()),
            ..BridgeConfig::default()
        };
        let (mut w, out) = worker(cfg);
        w.tick(Instant::now());
        assert!(out.take(ent::MAP_GRID).is_empty() && out.take(ent::MAP_CLOUD).is_empty());
        assert_eq!(out.take(ent::NAV_STATUS).len(), 1);
        // One WARN log per map, naming the problem and the path, for Studio.
        let warnings: Vec<LogData> = (out.take(logs::LOGS).iter())
            .map(|b| serde_json::from_value(parse_message(b).unwrap().0.data).unwrap())
            .collect();
        assert_eq!(warnings.len(), 2, "{warnings:?}");
        assert!(warnings.iter().all(|w| w.level == "WARN"));
        assert!(warnings[0].message.starts_with("map/grid not published: /nonexistent/map.yaml: "));
        assert!(warnings[1].message.starts_with("map/cloud not published: /nonexistent/map.pcd: "));
        w.handle("pose", &odom(0.0, 0.0, 0.0), Instant::now());
        assert_eq!(out.take(ent::ROBOT_POSE).len(), 1);
    }

    #[test]
    fn goal_sent_on_change_and_cleared_when_finished() {
        let (mut w, out) = worker(BridgeConfig::default());
        let now = Instant::now();
        let goal = |b: &[u8]| -> Arrows3DData { serde_json::from_value(parse_message(b).unwrap().0.data).unwrap() };
        w.handle("goal", br#"{"id":"C1","x":1.0,"y":2.0,"yaw":0.0}"#, now);
        let sent = out.take(ent::NAV_GOAL);
        assert_eq!(sent.len(), 1);
        assert_eq!(goal(&sent[0]).origins, vec![[1.0, 2.0, 0.0]]);
        w.handle("goal", br#"{"id":"C1","x":1.0,"y":2.0,"yaw":0.0}"#, now);
        assert!(out.take(ent::NAV_GOAL).is_empty(), "same goal is not resent at once");
        w.handle("goal_status", br#"{"id":"C0","state":"CANCELED","reason":"preempted"}"#, now);
        assert!(out.take(ent::NAV_GOAL).is_empty(), "an older goal ending keeps the current one");
        w.handle("goal_status", br#"{"id":"C1","state":"SUCCEEDED","reason":""}"#, now);
        let sent = out.take(ent::NAV_GOAL);
        assert_eq!(sent.len(), 1);
        assert!(goal(&sent[0]).origins.is_empty());
    }

    #[test]
    fn cloud_is_cut_and_placed_in_map_frame_with_z_kept() {
        let cfg = BridgeConfig { cloud_max_points: 5, ..BridgeConfig::default() };
        let (mut w, out) = worker(cfg);
        let now = Instant::now();
        w.handle("cloud", &[0u8; 12], now);
        assert!(out.take(ent::SENSORS_CLOUD).is_empty(), "no pose yet: nothing to place it with");
        w.handle("pose", &odom(1.0, 2.0, std::f64::consts::FRAC_PI_2), now);
        let mut b = 1.0f64.to_le_bytes().to_vec();
        b.extend_from_slice(&11u32.to_le_bytes());
        for i in 0..11 {
            let p = match i {
                0 => [1.0, 0.0, 0.7, 9.0],
                1 => [f32::NAN, 0.0, 0.0, 0.0],
                _ => [i as f32, 0.0, 0.0, 0.0],
            };
            p.iter().for_each(|v| b.extend_from_slice(&v.to_le_bytes()));
        }
        w.handle("cloud", &b, now);
        let sent = out.take(ent::SENSORS_CLOUD);
        assert_eq!(sent.len(), 1);
        let (m, bin) = parse_message(&sent[0]).unwrap();
        let pts = parse_points_xyz_f32(bin.unwrap(), m.count.unwrap()).unwrap();
        assert_eq!(pts.len(), 5);
        assert!(pts.iter().flatten().all(|v| v.is_finite()));
        let p = pts[0];
        assert!((p[0] - 1.0).abs() < 1e-5 && (p[1] - 3.0).abs() < 1e-5 && p[2] == 0.7, "{p:?}");
    }

    #[test]
    fn bad_input_is_counted_and_skipped() {
        let (mut w, out) = worker(BridgeConfig::default());
        let before = bad_input_count();
        w.handle("pose", b"not json", Instant::now());
        w.handle("unknown_input", b"whatever", Instant::now());
        assert!(bad_input_count() > before);
        assert!(out.0.lock().unwrap().is_empty());
        w.handle("pose", &odom(0.0, 0.0, 0.0), Instant::now());
        assert_eq!(out.take(ent::ROBOT_POSE).len(), 1);
    }
}
