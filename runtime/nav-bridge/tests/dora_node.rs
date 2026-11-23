//! The real `dorobot-nav-bridge` binary as a Dora node, without a Dora daemon:
//! dora-node-api's integration-testing mode (`DORA_TEST_WITH_INPUTS`) feeds it the
//! inputs and the Stop of a JSON file. Zenoh is loopback only: the bridge listens on
//! its own free `tcp/127.0.0.1:<port>` with multicast off, and the test subscriber is
//! a client connected only to that endpoint.

use dviz_protocol::{nav_entities, parse_message, GraphUpdate, LogData};
use serde_json::{json, Value};
use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Read};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
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

fn tmpdir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("navbridge-node-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

/// `config.json` in `dir`: listen on `port`, plus `extra` fields.
fn write_config(dir: &Path, port: u16, extra: Value) -> PathBuf {
    let mut cfg = json!({"robot_id": "tracer", "listen": [format!("tcp/127.0.0.1:{port}")]});
    cfg.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
    let path = dir.join("config.json");
    std::fs::write(&path, cfg.to_string()).unwrap();
    path
}

/// A free `w` x `h` trinary map in `dir`; returns the YAML path.
fn write_map(dir: &Path, w: usize, h: usize) -> PathBuf {
    let yaml = dir.join("map.yaml");
    std::fs::write(
        &yaml,
        "image: map.pgm\nmode: trinary\nresolution: 0.05\norigin: [0.0, 0.0, 0.0]\nnegate: 0\noccupied_thresh: 0.65\nfree_thresh: 0.196\n",
    )
    .unwrap();
    let mut pgm = format!("P5\n{w} {h}\n255\n").into_bytes();
    pgm.resize(pgm.len() + w * h, 254);
    std::fs::write(dir.join("map.pgm"), pgm).unwrap();
    yaml
}

/// Dora integration-test input: `pose` (UInt8 JSON odometry) every 100 ms in
/// [from, to), one `goal` as a string input at `from`, and a Stop at `stop` (seconds
/// after the node starts); without a Stop the event stream just ends. Returns the
/// number of inputs.
fn write_inputs(dir: &Path, from: f64, to: f64, stop: Option<f64>) -> usize {
    let pose = json!({
        "header": {"timestamp": 1.0, "frame_id": "map", "child_frame_id": "base_footprint", "seq": 0},
        "pose": {"position": {"x": 1.0, "y": 2.0, "z": 0.0}, "orientation": {"x": 0.0, "y": 0.0, "z": 0.0, "w": 1.0}},
        "twist": {"linear": {"x": 0.0, "y": 0.0, "z": 0.0}, "angular": {"x": 0.0, "y": 0.0, "z": 0.0}}
    })
    .to_string()
    .into_bytes();
    let goal = r#"{"id":"C1","x":3.0,"y":4.0,"yaw":0.0}"#;
    let mut events = vec![json!({"time_offset_secs": from, "type": "Input", "id": "goal", "data": goal})];
    let mut t = from;
    while t < to {
        events.push(json!({"time_offset_secs": t, "type": "Input", "id": "pose", "data": pose, "data_type": "UInt8"}));
        t += 0.1;
    }
    let inputs = events.len();
    if let Some(stop) = stop {
        events.push(json!({"time_offset_secs": stop, "type": "Stop"}));
    }
    let input = json!({"id": "viz_bridge", "events": events});
    std::fs::write(dir.join("inputs.json"), input.to_string()).unwrap();
    inputs
}

/// The bridge process; stdout lines are kept with the time they were read.
struct Node {
    child: Child,
    stdout: Arc<Mutex<Vec<(Instant, String)>>>,
    stderr: Option<JoinHandle<String>>,
}

impl Node {
    fn spawn(dir: &Path, config: &Path, env: &[(&str, &Path)]) -> Node {
        Node::spawn_args(dir, &[config.as_os_str()], env)
    }

    /// The bridge with these arguments, as Dora's integration-testing mode starts it.
    fn spawn_args(dir: &Path, args: &[&OsStr], env: &[(&str, &Path)]) -> Node {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_dorobot-nav-bridge"));
        cmd.args(args)
            .current_dir(dir)
            .env("DORA_TEST_WITH_INPUTS", dir.join("inputs.json"))
            .env("DORA_TEST_WRITE_OUTPUTS_TO", dir.join("outputs.jsonl"))
            .env_remove("DORA_NODE_CONFIG")
            .env_remove("NAV_VIZ_LOG_DIR")
            .env_remove("NAV_VIZ_DATAFLOW")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in env {
            cmd.env(key, value);
        }
        let mut child = cmd.spawn().expect("start dorobot-nav-bridge");
        let stdout = Arc::new(Mutex::new(Vec::new()));
        let lines = stdout.clone();
        let out = child.stdout.take().unwrap();
        thread::spawn(move || {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                lines.lock().unwrap().push((Instant::now(), line));
            }
        });
        let mut err = child.stderr.take().unwrap();
        let stderr = Some(thread::spawn(move || {
            let mut text = String::new();
            let _ = err.read_to_string(&mut text);
            text
        }));
        Node { child, stdout, stderr }
    }

    /// Waits up to `timeout` for the process to exit (kills it after that); returns its
    /// status, when it exited, and its stderr.
    fn wait(&mut self, timeout: Duration) -> (ExitStatus, Instant, String) {
        let deadline = Instant::now() + timeout;
        let (status, at) = loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                break (status, Instant::now());
            }
            if Instant::now() >= deadline {
                let _ = self.child.kill();
                panic!("the bridge did not exit within {timeout:?}; stdout: {:?}", self.stdout_text());
            }
            thread::sleep(Duration::from_millis(2));
        };
        let stderr = self.stderr.take().unwrap().join().unwrap();
        (status, at, stderr)
    }

    /// When the first stdout line containing `text` was read.
    fn line_at(&self, text: &str) -> Option<Instant> {
        self.stdout.lock().unwrap().iter().find(|(_, l)| l.contains(text)).map(|(t, _)| *t)
    }

    fn stdout_text(&self) -> Vec<String> {
        self.stdout.lock().unwrap().iter().map(|(_, l)| l.clone()).collect()
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

type Got = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

/// A client session connected only to the bridge. A stalling subscription blocks in
/// its first callback until this is dropped: the session stays connected but reads
/// nothing more (Zenoh runs callbacks on the link's receive task), so whatever the
/// bridge sends after that fills its TCP buffers.
struct Subscriber {
    got: Got,
    stalled: Arc<AtomicBool>,
    release: Arc<AtomicBool>,
    subs: Vec<zenoh::pubsub::Subscriber<()>>,
    session: zenoh::Session,
}

impl Subscriber {
    fn connect(port: u16) -> Subscriber {
        let listening = wait_until(Duration::from_secs(10), || TcpStream::connect(("127.0.0.1", port)).is_ok());
        assert!(listening, "the bridge never listened on {port}");
        let mut config = zenoh::Config::default();
        config.insert_json5("mode", r#""client""#).unwrap();
        config.insert_json5("connect/endpoints", &format!(r#"["tcp/127.0.0.1:{port}"]"#)).unwrap();
        config.insert_json5("listen/endpoints", "[]").unwrap();
        config.insert_json5("scouting/multicast/enabled", "false").unwrap();
        let session = zenoh::open(config).wait().expect("subscriber session");
        Subscriber {
            got: Got::default(),
            stalled: Arc::default(),
            release: Arc::default(),
            subs: Vec::new(),
            session,
        }
    }

    /// Subscribes to `dviz/tracer/<keys>`; with `stall` the first sample blocks.
    fn subscribe(&mut self, keys: &str, stall: bool) {
        let (sink, stalled, release) = (self.got.clone(), self.stalled.clone(), self.release.clone());
        let sub = self
            .session
            .declare_subscriber(format!("dviz/tracer/{keys}"))
            .callback(move |s: Sample| {
                sink.lock().unwrap().push((s.key_expr().as_str().to_owned(), s.payload().to_bytes().into_owned()));
                if stall {
                    stalled.store(true, Ordering::Relaxed);
                    while !release.load(Ordering::Relaxed) {
                        thread::sleep(Duration::from_millis(10));
                    }
                }
            })
            .wait()
            .expect("subscriber");
        self.subs.push(sub);
    }

    /// (type, data) of every message received on `dviz/tracer/<entity>`.
    fn messages(&self, entity: &str) -> Vec<(String, Value)> {
        let key = format!("dviz/tracer/{entity}");
        let got = self.got.lock().unwrap();
        got.iter()
            .filter(|(k, _)| *k == key)
            .map(|(_, b)| parse_message(b).map(|(m, _)| (m.msg_type, m.data)).expect("dviz message"))
            .collect()
    }

    fn logs(&self) -> Vec<LogData> {
        self.messages("logs").into_iter().map(|(_, d)| serde_json::from_value(d).expect("LogData")).collect()
    }
}

impl Drop for Subscriber {
    fn drop(&mut self) {
        self.release.store(true, Ordering::Relaxed);
    }
}

/// Requirement: after Dora's Stop the bridge exits within 2 s, without a panic, even
/// while a Studio reader is connected but stalled. Closing the Zenoh session then
/// blocks (zenoh-link-tcp's 10 s SO_LINGER with unsent data), so the bridge must not
/// wait for it.
#[test]
fn stop_exits_within_two_seconds_with_a_stalled_reader() {
    let _serial = serial();
    let dir = tmpdir("stall");
    let port = free_port();
    let map = write_map(&dir, 2000, 2000);
    let config = write_config(&dir, port, json!({"map_yaml": map}));
    write_inputs(&dir, 0.2, 3.9, Some(4.0));
    let mut node = Node::spawn(&dir, &config, &[]);
    // The reader stalls on its first small nav/status message ...
    let mut sub = Subscriber::connect(port);
    sub.subscribe("nav/status", true);
    assert!(
        wait_until(Duration::from_secs(3), || sub.stalled.load(Ordering::Relaxed)),
        "nothing reached the reader before Stop"
    );
    // ... and only then asks for the maps: the 4 MB map/grid goes into a link that
    // nobody reads, so unsent data is queued when the bridge closes Zenoh.
    sub.subscribe("map/**", false);

    let (status, exited, stderr) = node.wait(Duration::from_secs(15));
    let stopping = node.line_at("nav-bridge: stopping").expect("no stop line");
    let took = exited.duration_since(stopping);
    println!("stalled reader: exit {took:?} after Stop; stdout {:?}", node.stdout_text());
    assert!(took < Duration::from_secs(2), "exit took {took:?} after Stop");
    assert!(status.success(), "{status}; stderr: {stderr}");
    assert!(!stderr.to_lowercase().contains("panic"), "stderr: {stderr}");
    assert!(
        node.line_at("Zenoh still closing").is_some(),
        "the stalled reader did not hold up the Zenoh close, so this test proved nothing: {:?}",
        node.stdout_text()
    );
    drop(sub);
    let _ = std::fs::remove_dir_all(dir);
}

/// The whole node: Dora inputs (bytes and a string) reach Studio, node logs written
/// while Studio listens are forwarded with their levels, the graph comes from
/// `NAV_VIZ_DATAFLOW`, a missing map is a WARN log naming its path, and Stop exits
/// cleanly and fast.
#[test]
fn node_forwards_inputs_logs_graph_and_a_map_warning() {
    let _serial = serial();
    let dir = tmpdir("logs");
    let port = free_port();
    let log_dir = dir.join("out");
    std::fs::create_dir_all(&log_dir).unwrap();
    let missing = dir.join("missing/map.yaml");
    let config = write_config(&dir, port, json!({"map_yaml": missing, "map_pcd": null}));
    write_inputs(&dir, 0.2, 5.9, Some(6.0));
    let dataflow = fixture("tracer_nav.yml");
    let mut node = Node::spawn(&dir, &config, &[("NAV_VIZ_LOG_DIR", &log_dir), ("NAV_VIZ_DATAFLOW", &dataflow)]);
    let mut sub = Subscriber::connect(port);
    sub.subscribe("**", false);

    // The graph is sent as soon as someone listens (and every 5 s).
    assert!(wait_until(Duration::from_secs(4), || !sub.messages("graph").is_empty()), "no graph_update");
    let (kind, data) = sub.messages("graph").remove(0);
    assert_eq!(kind, "graph_update");
    let graph: GraphUpdate = serde_json::from_value(data).unwrap();
    assert_eq!(graph.nodes.len(), 14);
    assert!(graph.edges.iter().any(|e| e.from_node == "astar_planner" && e.from_port == "path" && e.to_node == "pure_pursuit"));
    let defs = sub.messages("nodes");
    assert!(defs.len() >= 14 && defs.iter().all(|(t, _)| t == "node_definition"));

    // Node logs written while Studio listens arrive within the next 1 s poll.
    for name in ["log_astar_planner.jsonl", "log_pose_fuser.jsonl"] {
        std::fs::copy(fixture(&format!("dora_logs/{name}")), log_dir.join(name)).unwrap();
    }
    let levels = |logs: &[LogData], node: &str| -> Vec<String> {
        logs.iter().filter(|l| l.node_id == node).map(|l| l.level.clone()).collect()
    };
    let forwarded = wait_until(Duration::from_secs(3), || {
        let logs = sub.logs();
        levels(&logs, "astar_planner").len() == 2 && levels(&logs, "pose_fuser").len() == 2
    });
    let logs = sub.logs();
    assert!(forwarded, "node logs not forwarded: {logs:?}");
    assert_eq!(levels(&logs, "astar_planner"), ["INFO", "INFO"]);
    assert_eq!(levels(&logs, "pose_fuser"), ["WARN", "WARN"]);
    let warning = logs.iter().find(|l| l.message.contains("missing/map.yaml")).expect("missing-map WARN");
    assert_eq!(warning.level, "WARN");
    assert!(warning.message.starts_with("map/grid not published: "), "{}", warning.message);

    // Inputs went through Dora, the inbox and the worker.
    assert!(wait_until(Duration::from_secs(2), || !sub.messages(nav_entities::ROBOT_POSE).is_empty()), "no robot/pose");
    let goal_sent = |sub: &Subscriber| {
        sub.messages(nav_entities::NAV_GOAL).iter().any(|(_, d)| d["origins"] == json!([[3.0, 4.0, 0.0]]))
    };
    // The goal came before Studio connected; it is resent every 2 s.
    assert!(wait_until(Duration::from_secs(3), || goal_sent(&sub)), "the string goal input did not arrive");

    let (status, exited, stderr) = node.wait(Duration::from_secs(10));
    let took = exited.duration_since(node.line_at("nav-bridge: stopping").expect("no stop line"));
    println!("healthy reader: exit {took:?} after Stop; stdout {:?}", node.stdout_text());
    assert!(took < Duration::from_secs(2), "exit took {took:?}");
    assert!(status.success(), "{status}; stderr: {stderr}");
    assert!(!stderr.to_lowercase().contains("panic"), "stderr: {stderr}");
    assert!(node.line_at("Zenoh still closing").is_none(), "a healthy reader does not hold up the close");
    let out = node.stdout_text();
    assert!(out.iter().any(|l| l.contains(" inputs, ") && l.ends_with(", 0 not bytes")), "{out:?}");
    drop(sub);
    let _ = std::fs::remove_dir_all(dir);
}

/// Ruling: inside a dataflow the bridge never fails. A startup failure is one ERROR
/// line on stderr naming the cause; then the node consumes (drops) its inputs until
/// Stop or the end of the event stream and exits 0, so it cannot fail DORA_NAV.
fn assert_degraded(node: &mut Node, inputs: usize, cause: &str) {
    let (status, _, stderr) = node.wait(Duration::from_secs(10));
    let out = node.stdout_text();
    assert!(status.success(), "{status}; stdout {out:?}; stderr: {stderr}");
    let errors: Vec<&str> = stderr.lines().filter(|l| l.contains("ERROR")).collect();
    assert_eq!(errors.len(), 1, "stderr: {stderr}");
    assert!(errors[0].contains(cause), "the ERROR line names {cause:?}: {}", errors[0]);
    println!("degraded: {}; {:?}", errors[0], out.last());
    assert!(!stderr.to_lowercase().contains("panic"), "stderr: {stderr}");
    // The first line, right after Dora's init: DORA_NAV's autostart.sh waits for it.
    assert!(out.first().is_some_and(|l| l.starts_with("nav-bridge: started by Dora (dataflow ")), "{out:?}");
    let consumed = format!("nav-bridge: {inputs} inputs");
    assert!(out.iter().any(|l| l.starts_with(&consumed)), "all {inputs} inputs consumed: {out:?}");
}

#[test]
fn missing_config_consumes_inputs_and_exits_0_when_the_stream_ends() {
    let _serial = serial();
    let dir = tmpdir("noconfig");
    let inputs = write_inputs(&dir, 0.1, 1.0, None);
    let missing = dir.join("no/such/config.json");
    let mut node = Node::spawn(&dir, &missing, &[]);
    assert_degraded(&mut node, inputs, "no/such/config.json");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn busy_port_consumes_inputs_and_exits_0_after_stop() {
    let _serial = serial();
    let dir = tmpdir("busyport");
    let held = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = held.local_addr().unwrap().port();
    let config = write_config(&dir, port, json!({}));
    let inputs = write_inputs(&dir, 0.1, 1.0, Some(1.5));
    let mut node = Node::spawn(&dir, &config, &[]);
    assert_degraded(&mut node, inputs, &format!("127.0.0.1:{port}"));
    assert!(node.line_at("nav-bridge: stopping").is_some(), "{:?}", node.stdout_text());
    drop(held);
    let _ = std::fs::remove_dir_all(dir);
}

/// Dora 1.1 fails the start of every node of the dataflow when one node stops before
/// it subscribes, so bad arguments (a config path with a space is split into two by
/// Dora) must not end the bridge before Dora is initialized: they are one ERROR line,
/// then the node idles like any other startup failure.
#[test]
fn bad_arguments_under_dora_consume_inputs_and_exit_0() {
    let _serial = serial();
    let dir = tmpdir("badargs");
    let inputs = write_inputs(&dir, 0.1, 1.0, Some(1.5));
    let cases: [(&[&OsStr], &str); 3] = [
        (&[], "got 0"),
        (&[OsStr::new("/home/nvidia/my"), OsStr::new("config.json")], "got 2"),
        (&[OsStr::new("--help"), OsStr::new("extra")], "got 2"),
    ];
    for (args, cause) in cases {
        let mut node = Node::spawn_args(&dir, args, &[]);
        assert_degraded(&mut node, inputs, cause);
    }
    let _ = std::fs::remove_dir_all(dir);
}

/// `--help` works without Dora (the overlay in DORA_NAV smoke-runs it before it adds
/// the node); anything else outside Dora is "not started by Dora", exit 1.
#[test]
fn help_without_dora_exits_0_and_anything_else_exits_1() {
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_dorobot-nav-bridge"))
            .args(args)
            .env_remove("DORA_TEST_WITH_INPUTS")
            .env_remove("DORA_NODE_CONFIG")
            .stdin(Stdio::null())
            .output()
            .expect("run dorobot-nav-bridge")
    };
    let help = run(&["--help"]);
    assert!(help.status.success(), "{help:?}");
    assert!(String::from_utf8_lossy(&help.stdout).starts_with("usage: dorobot-nav-bridge "), "{help:?}");
    for args in [&[][..], &["config.json"][..], &["a", "b"][..]] {
        let out = run(args);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(1), "{args:?}: {out:?}");
        assert!(stderr.contains("not started by Dora"), "{args:?}: {stderr}");
        assert!(stderr.contains("usage: dorobot-nav-bridge "), "{args:?}: {stderr}");
    }
}
