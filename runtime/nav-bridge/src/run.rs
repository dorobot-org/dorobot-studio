//! The Dora side of the bridge, testable without Dora: `main` turns each Dora event
//! into a [`NodeEvent`] for [`Bridge::on_event`], which only queues inputs (and keeps
//! the latest gate state), and owns the process exit (see `main.rs`). The bridge owns
//! the Zenoh session: the worker publishes on it, and Studio's commands arrive on it as
//! queries on `dviz/<robot_id>/cmd` (see `forward.rs` for what happens to them).

use crate::command::{expand_home, load_token, CmdPolicy, TokenBucket};
use crate::config::{BridgeConfig, CmdConfig, InputFormat};
use crate::forward::{CommandLanes, GateSlot, Job, Replier};
use crate::logs::LogSources;
use crate::publish::{open_session, zenoh_publisher_on, Publish};
use crate::worker::{spawn_worker_with_logs, Inbox};
use dora_node_api::DoraArray;
use dviz_protocol::{cmd_key, parse_cmd_request, CmdAck};
use std::ffi::OsString;
use std::io::Write;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, PoisonError, RwLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use zenoh::query::{Query, Queryable};
use zenoh::Wait;

/// Inputs queued for the worker; when full the oldest is dropped.
pub const INBOX_CAP: usize = 64;

pub const USAGE: &str = "usage: dorobot-nav-bridge <config.json>  (a node of a Dora dataflow; see README.md)";

/// The config path: the one argument. Dora splits a node's `args` shell-style, so a
/// path with a space arrives as two arguments.
pub fn config_arg(args: &[OsString]) -> Result<PathBuf, String> {
    match args {
        [config] => Ok(PathBuf::from(config)),
        _ => Err(format!(
            "expected one argument (the config path), got {}: {args:?}; a path with a space is split by Dora ({USAGE})",
            args.len()
        )),
    }
}

/// One Dora event as the bridge sees it.
pub enum NodeEvent {
    /// An input; `data` is `None` when it is neither bytes (UInt8) nor a string.
    Input { id: String, data: Option<Vec<u8>> },
    /// Dora's `Stop`.
    Stop,
    /// Any other event (ignored).
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Stop,
}

/// An input's bytes: a UInt8 array as is, a string array's text as UTF-8.
pub fn input_bytes(data: &DoraArray) -> Option<Vec<u8>> {
    if let Ok(bytes) = <&[u8]>::try_from(data) {
        return Some(bytes.to_vec());
    }
    <&str>::try_from(data).ok().map(|s| s.as_bytes().to_vec())
}

/// Where Dora writes this dataflow's node logs: `override_dir` (env `NAV_VIZ_LOG_DIR`)
/// if set and not empty, else `<cwd>/out/<dataflow_id>`.
pub fn log_dir(override_dir: Option<OsString>, cwd: &Path, dataflow_id: &str) -> PathBuf {
    match override_dir.filter(|d| !d.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => cwd.join("out").join(dataflow_id),
    }
}

/// The command lanes, shared by the queryable callback (which submits) and the shutdown
/// (which takes them out to stop them); `None` once stopping.
type SharedLanes = Arc<RwLock<Option<CommandLanes>>>;

/// The goal token bucket's burst (its rate is `cmd.max_goal_per_s`).
const GOAL_BURST: u32 = 2;

/// The running bridge: inbox, worker thread and its stop flag; the Zenoh session it
/// owns, with the command queryable, the command lanes and the latest gate state.
pub struct Bridge {
    inbox: Arc<Inbox>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    worker_dead: bool,
    /// Started without a worker (see [`Bridge::disabled`]).
    disabled: bool,
    inputs: u64,
    not_bytes: u64,
    /// The Zenoh session (the worker's publisher holds a clone); `wait` closes it.
    session: Option<zenoh::Session>,
    /// `dviz/<robot_id>/cmd`; none without a session or with a bad `cmd.forward`.
    queryable: Option<Queryable<()>>,
    lanes: SharedLanes,
    /// The latest gate state, stored by `on_event`, read by the goal lane.
    gate: Arc<GateSlot>,
    /// Dora input ids whose format is `gate_state`.
    gate_inputs: Vec<String>,
    /// Undeclares the queryable, stops the lanes and closes the session (see `wait`).
    shutdown: Option<JoinHandle<()>>,
}

/// Answers one command query with the JSON [`CmdAck`], on the query's own key. Never
/// panics; a reply that cannot be sent (session closed, asker gone) is dropped.
struct ZenohReplier(Query);

impl Replier for ZenohReplier {
    fn reply(self: Box<Self>, ack: &CmdAck) {
        let Ok(bytes) = serde_json::to_vec(ack) else { return };
        let query = &self.0;
        let _ = query.reply(query.key_expr().clone(), bytes).wait();
    }
}

/// The queryable on `key`. Its callback only parses the request and submits it, so it
/// never blocks on nav_command; a query the lanes refuse (full, or stopping) is dropped,
/// which ends it without a reply.
fn declare_commands(session: &zenoh::Session, key: &str, lanes: SharedLanes) -> Result<Queryable<()>, String> {
    session
        .declare_queryable(key.to_owned())
        .callback(move |query: Query| {
            let payload = query.payload().map(|p| p.to_bytes().into_owned()).unwrap_or_default();
            let job = Job { request: parse_cmd_request(&payload), replier: Box::new(ZenohReplier(query)) };
            let guard = lanes.read().unwrap_or_else(PoisonError::into_inner);
            if let Some(lanes) = guard.as_ref() {
                let _accepted = lanes.submit(job);
            }
        })
        .wait()
        .map_err(|e| format!("zenoh queryable {key}: {e}"))
}

/// The goal / initialpose policy from `cmd`, and the one start line saying whether those
/// commands are accepted. The token file is read only with `goal_enabled`; a leading
/// `~/` in its path is `home`. Neither the policy's errors nor the line show the token.
pub fn command_policy(cmd: &CmdConfig, home: Option<&Path>) -> (CmdPolicy, String) {
    let token = if cmd.goal_enabled {
        load_token(&expand_home(&cmd.token_file, home)).map_err(|e| format!("token file {e}"))
    } else {
        Err("goal_enabled is false in the config".to_owned())
    };
    let line = match &token {
        Ok(_) => "nav-bridge: goal/initialpose commands enabled".to_owned(),
        Err(why) => format!("nav-bridge: goal/initialpose commands closed: {why}"),
    };
    let bucket = TokenBucket::new(cmd.max_goal_per_s, GOAL_BURST);
    (CmdPolicy { goal_enabled: cmd.goal_enabled, token: token.ok(), bucket }, line)
}

/// Waits until `t` has finished or `deadline` passed; true if it finished.
fn finished_by(t: &JoinHandle<()>, deadline: Instant) -> bool {
    while !t.is_finished() {
        let now = Instant::now();
        if now >= deadline {
            return false;
        }
        thread::sleep((deadline - now).min(Duration::from_millis(5)));
    }
    true
}

impl Bridge {
    /// Opens the Zenoh session (peer, listening on `cfg.listen`) and its publishers, then
    /// [`Bridge::start_with`]. Any failure, a panic included, is an `Err` naming the cause.
    pub fn start(cfg: BridgeConfig, sources: LogSources) -> Result<Bridge, String> {
        let start = move || -> Result<Bridge, String> {
            let session = open_session(&cfg)?;
            let publisher = zenoh_publisher_on(&session, &cfg)?;
            Bridge::start_with(cfg, sources, session, publisher)
        };
        panic::catch_unwind(AssertUnwindSafe(start)).unwrap_or_else(|_| Err("start panicked (see above)".into()))
    }

    /// The bridge on an open `session`, publishing through `publisher` (tests inject
    /// one). In order: reads the command token (only with `cmd.goal_enabled`) and prints
    /// one line about goals / initial poses; starts the worker; starts the command lanes;
    /// declares the queryable on `dviz/<robot_id>/cmd`. A `cmd.forward` other than
    /// `127.0.0.1:<port>` is one ERROR line and no command channel at all (no queryable,
    /// `cmd_goal_enabled` false), but data is still published.
    pub fn start_with(
        cfg: BridgeConfig,
        sources: LogSources,
        session: zenoh::Session,
        publisher: Box<dyn Publish>,
    ) -> Result<Bridge, String> {
        let commands = match cfg.cmd.forward_addr() {
            Ok(target) => {
                let home = std::env::var_os("HOME").map(PathBuf::from);
                let (policy, line) = command_policy(&cfg.cmd, home.as_deref());
                println!("{line}");
                Some((target, policy))
            }
            Err(e) => {
                eprintln!(
                    "nav-bridge: ERROR: {e}; no command channel (estop, disarm, goal and initialpose from Studio \
                     get no reply), data is still published"
                );
                None
            }
        };
        let goal_open = commands.as_ref().is_some_and(|(_, policy)| policy.goal_channel_open());
        let key = cmd_key("dviz", &cfg.robot_id);
        let timeout = Duration::from_millis(cfg.cmd.reply_timeout_ms);
        let mut bridge = Bridge::assemble(cfg, sources, publisher, goal_open);
        bridge.session = Some(session);
        if let (Some((target, policy)), Some(session)) = (commands, bridge.session.as_ref()) {
            let lanes = CommandLanes::spawn(policy, bridge.gate.clone(), target, timeout, Box::new(std::io::stdout()));
            *bridge.lanes.write().unwrap_or_else(PoisonError::into_inner) = Some(lanes);
            // On an error `bridge` is dropped, which stops the worker and the lanes.
            bridge.queryable = Some(declare_commands(session, &key, bridge.lanes.clone())?);
        }
        Ok(bridge)
    }

    /// Without Zenoh: the worker publishes through `publisher`; no command channel.
    pub fn with_publisher(cfg: BridgeConfig, sources: LogSources, publisher: Box<dyn Publish>) -> Bridge {
        Bridge::assemble(cfg, sources, publisher, false)
    }

    fn assemble(cfg: BridgeConfig, sources: LogSources, publisher: Box<dyn Publish>, cmd_goal_enabled: bool) -> Bridge {
        let gate_inputs =
            cfg.inputs.iter().filter(|(_, f)| **f == InputFormat::GateState).map(|(id, _)| id.clone()).collect();
        let inbox = Inbox::new(INBOX_CAP);
        let stop = Arc::new(AtomicBool::new(false));
        let worker = spawn_worker_with_logs(cfg, inbox.clone(), publisher, stop.clone(), sources, cmd_goal_enabled);
        Bridge {
            inbox,
            stop,
            worker: Some(worker),
            worker_dead: false,
            disabled: false,
            inputs: 0,
            not_bytes: 0,
            session: None,
            queryable: None,
            lanes: SharedLanes::default(),
            gate: Arc::new(GateSlot::new()),
            gate_inputs,
            shutdown: None,
        }
    }

    /// For a bridge that could not start (bad config, Zenoh error): no worker, no
    /// Zenoh. It consumes and drops every event until Stop, so the node never fails
    /// the dataflow; the caller reports the cause once.
    pub fn disabled() -> Bridge {
        Bridge {
            inbox: Inbox::new(1),
            stop: Arc::new(AtomicBool::new(true)),
            worker: None,
            worker_dead: true,
            disabled: true,
            inputs: 0,
            not_bytes: 0,
            session: None,
            queryable: None,
            lanes: SharedLanes::default(),
            gate: Arc::new(GateSlot::new()),
            gate_inputs: Vec::new(),
            shutdown: None,
        }
    }

    /// One Dora event; never blocks. A gate state input is stored for the goal lane
    /// first, whatever the worker's state; then an input goes into the inbox. If the
    /// worker thread has died, one ERROR line goes to `err` (stderr, which Dora records)
    /// and inputs are dropped from then on; events are still consumed, so the dataflow
    /// is not disturbed.
    pub fn on_event(&mut self, event: NodeEvent, err: &mut dyn Write) -> Flow {
        match event {
            NodeEvent::Input { id, data } => {
                self.inputs += 1;
                if let Some(bytes) = &data {
                    if self.gate_inputs.contains(&id) {
                        self.gate.store(bytes, Instant::now());
                    }
                }
                match data {
                    None => self.not_bytes += 1,
                    Some(bytes) if self.worker_alive(err) => self.inbox.push(id, bytes),
                    Some(_) => {}
                }
                Flow::Continue
            }
            NodeEvent::Stop => Flow::Stop,
            NodeEvent::Other => {
                self.worker_alive(err);
                Flow::Continue
            }
        }
    }

    /// Whether the worker runs; says once on `err` when it no longer does.
    fn worker_alive(&mut self, err: &mut dyn Write) -> bool {
        if !self.worker_dead && self.worker.as_ref().is_some_and(JoinHandle::is_finished) {
            self.worker_dead = true;
            let _ = writeln!(
                err,
                "nav-bridge: ERROR: the worker thread has stopped (see its panic above); nothing is published \
                 any more, Dora inputs are consumed and dropped"
            );
        }
        !self.worker_dead
    }

    /// Tells the worker to stop: it leaves its loop within ~10 ms, then drops its
    /// publisher. The session stays open for the command channel until `wait`.
    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    /// Stops everything, in order, and waits until `deadline`; true if it all ended.
    /// First the worker (a dead, panicked one counts as ended); then, on a helper thread:
    /// the queryable is undeclared, so no new command comes in; the command lanes stop
    /// (commands already queued are still answered); the Zenoh session closes. Each of
    /// those can block on Zenoh (a stalled Studio reader) or nav_command, so whatever is
    /// still running at `deadline` is left behind (`main` then ends the process).
    pub fn wait(&mut self, deadline: Instant) -> bool {
        self.request_stop();
        if let Some(worker) = self.worker.take() {
            if !finished_by(&worker, deadline) {
                self.worker = Some(worker);
                return false;
            }
            // A panic was already printed by the panic hook; nothing more to report.
            let _ = worker.join();
        }
        self.begin_shutdown();
        let Some(helper) = self.shutdown.take() else { return true };
        if !finished_by(&helper, deadline) {
            self.shutdown = Some(helper);
            return false;
        }
        let _ = helper.join();
        true
    }

    /// Starts the command side's shutdown on a helper thread, once (see [`Bridge::wait`]).
    fn begin_shutdown(&mut self) {
        if self.shutdown.is_some() || (self.session.is_none() && self.queryable.is_none()) {
            return;
        }
        let (queryable, lanes, session) = (self.queryable.take(), self.lanes.clone(), self.session.take());
        let shut = move || {
            if let Some(queryable) = queryable {
                let _ = queryable.undeclare().wait();
            }
            let taken = lanes.write().unwrap_or_else(PoisonError::into_inner).take();
            if let Some(lanes) = taken {
                lanes.stop();
            }
            if let Some(session) = session {
                let _ = session.close().wait();
            }
        };
        self.shutdown = thread::Builder::new().name("nav-bridge-shutdown".into()).spawn(shut).ok();
    }

    pub fn summary(&self) -> String {
        if self.disabled {
            return format!("nav-bridge: {} inputs, all dropped (not running)", self.inputs);
        }
        format!(
            "nav-bridge: {} inputs, {} dropped (inbox full), {} not bytes",
            self.inputs,
            self.inbox.dropped(),
            self.not_bytes
        )
    }
}

impl Drop for Bridge {
    /// Stops the worker and starts the command side's shutdown, waiting for neither.
    fn drop(&mut self) {
        self.request_stop();
        self.begin_shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::GateView;
    use crate::config::{CmdConfig, InputFormat};
    use dora_node_api::IntoArrow;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    #[test]
    fn input_bytes_from_u8_and_string_arrays() {
        assert_eq!(input_bytes(&vec![1u8, 2, 3].into_arrow()), Some(vec![1, 2, 3]));
        assert_eq!(input_bytes(&r#"{"id":"C1"}"#.into_arrow()), Some(br#"{"id":"C1"}"#.to_vec()));
        assert_eq!(input_bytes(&vec![1.0f32].into_arrow()), None);
    }

    #[test]
    fn config_arg_is_the_one_argument() {
        assert_eq!(config_arg(&["/c/tracer.json".into()]), Ok(PathBuf::from("/c/tracer.json")));
        let none = config_arg(&[]).unwrap_err();
        assert!(none.starts_with("expected one argument (the config path), got 0: []"), "{none}");
        let split = config_arg(&["/home/my".into(), "config.json".into()]).unwrap_err();
        assert!(split.contains("got 2: [\"/home/my\", \"config.json\"]"), "{split}");
    }

    #[test]
    fn log_dir_from_env_or_cwd_and_dataflow_id() {
        let cwd = Path::new("/apps/tracer");
        assert_eq!(log_dir(None, cwd, "01a2"), Path::new("/apps/tracer/out/01a2"));
        assert_eq!(log_dir(Some("".into()), cwd, "01a2"), Path::new("/apps/tracer/out/01a2"));
        assert_eq!(log_dir(Some("/var/logs".into()), cwd, "01a2"), Path::new("/var/logs"));
    }

    /// Records every put; wants everything.
    #[derive(Clone, Default)]
    struct Recorder(Arc<Mutex<Vec<String>>>);

    impl Publish for Recorder {
        fn wants(&self, _: &str) -> bool {
            true
        }
        fn put(&mut self, entity: &str, _: Vec<u8>) {
            self.0.lock().unwrap().push(entity.to_owned());
        }
    }

    fn odom() -> Vec<u8> {
        br#"{"header":{"timestamp":1.0},"pose":{"position":{"x":1.0,"y":2.0},"orientation":{"x":0,"y":0,"z":0,"w":1}},"twist":{"linear":{"x":0},"angular":{"z":0}}}"#.to_vec()
    }

    #[test]
    fn inputs_reach_the_worker_and_stop_ends_the_loop() {
        let out = Recorder::default();
        let mut bridge = Bridge::with_publisher(BridgeConfig::default(), LogSources::default(), Box::new(out.clone()));
        let mut err = Vec::new();
        let input = |data: Option<Vec<u8>>| NodeEvent::Input { id: "pose".into(), data };
        assert_eq!(bridge.on_event(input(Some(odom())), &mut err), Flow::Continue);
        assert_eq!(bridge.on_event(input(None), &mut err), Flow::Continue);
        assert_eq!(bridge.on_event(NodeEvent::Other, &mut err), Flow::Continue);
        let sent = |e: &str| out.0.lock().unwrap().iter().any(|x| x == e);
        let deadline = Instant::now() + Duration::from_secs(2);
        while !sent("robot/pose") && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(sent("robot/pose"), "the input went through the inbox to the worker");
        assert_eq!(bridge.on_event(NodeEvent::Stop, &mut err), Flow::Stop);
        let t = Instant::now();
        assert!(bridge.wait(t + Duration::from_secs(1)), "worker finished");
        assert!(t.elapsed() < Duration::from_millis(200), "{:?}", t.elapsed());
        assert!(err.is_empty(), "{}", String::from_utf8_lossy(&err));
        assert_eq!(bridge.summary(), "nav-bridge: 2 inputs, 0 dropped (inbox full), 1 not bytes");
    }

    /// Output whose first put panics: the worker thread dies.
    struct Panics;

    impl Publish for Panics {
        fn wants(&self, _: &str) -> bool {
            true
        }
        fn put(&mut self, _: &str, _: Vec<u8>) {
            panic!("test: worker output failed");
        }
    }

    #[test]
    fn dead_worker_is_reported_once_and_events_are_still_consumed() {
        let mut bridge = Bridge::with_publisher(BridgeConfig::default(), LogSources::default(), Box::new(Panics));
        let mut err = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(2);
        // The first tick sends nav/status, so the worker dies at once.
        while bridge.worker.as_ref().is_some_and(|w| !w.is_finished()) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        for _ in 0..3 {
            let event = NodeEvent::Input { id: "pose".into(), data: Some(odom()) };
            assert_eq!(bridge.on_event(event, &mut err), Flow::Continue);
        }
        let text = String::from_utf8(err).unwrap();
        assert_eq!(text.matches("ERROR").count(), 1, "{text}");
        assert!(text.contains("worker thread has stopped"), "{text}");
        assert_eq!(bridge.on_event(NodeEvent::Stop, &mut Vec::new()), Flow::Stop);
        assert!(bridge.wait(Instant::now() + Duration::from_millis(100)), "a dead worker counts as finished");
        assert_eq!(bridge.summary(), "nav-bridge: 3 inputs, 0 dropped (inbox full), 0 not bytes");
    }

    /// Output that blocks for 3 s when dropped, like a Zenoh session whose reader
    /// stalled (SO_LINGER on close).
    struct SlowClose;

    impl Publish for SlowClose {
        fn wants(&self, _: &str) -> bool {
            false
        }
        fn put(&mut self, _: &str, _: Vec<u8>) {}
    }

    impl Drop for SlowClose {
        fn drop(&mut self) {
            thread::sleep(Duration::from_secs(3));
        }
    }

    #[test]
    fn wait_gives_up_at_the_deadline() {
        let mut bridge = Bridge::with_publisher(BridgeConfig::default(), LogSources::default(), Box::new(SlowClose));
        bridge.request_stop();
        let t = Instant::now();
        assert!(!bridge.wait(t + Duration::from_millis(300)), "still closing");
        let took = t.elapsed();
        assert!(took >= Duration::from_millis(300) && took < Duration::from_millis(400), "{took:?}");
    }

    /// The gate slot is fed from the Dora event loop itself, so goals are judged on the
    /// latest gate state even with the worker dead. The gate inputs are the config's
    /// `gate_state` entries, whatever their id.
    #[test]
    fn gate_input_updates_slot_even_when_worker_dead() {
        let mut cfg = BridgeConfig::default();
        cfg.inputs.insert("safety".into(), InputFormat::GateState);
        let mut bridge = Bridge::with_publisher(cfg, LogSources::default(), Box::new(Panics));
        let deadline = Instant::now() + Duration::from_secs(2);
        while bridge.worker.as_ref().is_some_and(|w| !w.is_finished()) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        let mut err = Vec::new();
        let input = |id: &str, json: &str| NodeEvent::Input { id: id.into(), data: Some(json.as_bytes().to_vec()) };
        assert_eq!(bridge.gate.view(Instant::now()), GateView::Unknown);
        bridge.on_event(input("gate_state", r#"{"armed":true,"estop":false}"#), &mut err);
        assert!(String::from_utf8_lossy(&err).contains("worker thread has stopped"), "the worker is dead");
        assert_eq!(bridge.gate.view(Instant::now()), GateView::Armed);
        // Another input is not the gate, even if it looks like one.
        bridge.on_event(input("chassis", r#"{"armed":false,"estop":true}"#), &mut err);
        assert_eq!(bridge.gate.view(Instant::now()), GateView::Armed);
        bridge.on_event(input("safety", r#"{"armed":true,"estop":true}"#), &mut err);
        assert_eq!(bridge.gate.view(Instant::now()), GateView::Estop);
        assert!(bridge.wait(Instant::now() + Duration::from_millis(100)));
    }

    /// The one start line about goals and initial poses; it never shows the token.
    #[test]
    fn command_policy_line() {
        let dir = std::env::temp_dir().join(format!("navbridge-run-policy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let token = |name: &str, mode: u32| {
            let p = dir.join(name);
            std::fs::write(&p, "run-s3cret\n").unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(mode)).unwrap();
            p.display().to_string()
        };
        let cmd = |goal_enabled: bool, token_file: String| CmdConfig { goal_enabled, token_file, ..CmdConfig::default() };

        let (policy, line) = command_policy(&cmd(true, token("ok", 0o600)), None);
        assert!(policy.goal_channel_open());
        assert_eq!(line, "nav-bridge: goal/initialpose commands enabled");
        // `~/` is the home directory
        token("home_tok", 0o600);
        let (policy, line) = command_policy(&cmd(true, "~/home_tok".into()), Some(&dir));
        assert!(policy.goal_channel_open(), "{line}");

        let (policy, line) = command_policy(&cmd(false, token("off", 0o600)), None);
        assert!(!policy.goal_channel_open());
        assert_eq!(line, "nav-bridge: goal/initialpose commands closed: goal_enabled is false in the config");

        let (policy, line) = command_policy(&cmd(true, token("open", 0o644)), None);
        assert!(!policy.goal_channel_open());
        assert!(line.starts_with("nav-bridge: goal/initialpose commands closed: "), "{line}");
        assert!(line.contains("too open") && !line.contains("s3cret"), "{line}");

        let (policy, line) = command_policy(&cmd(true, dir.join("missing").display().to_string()), None);
        assert!(!policy.goal_channel_open());
        assert!(line.contains("missing"), "{line}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn disabled_bridge_consumes_events_until_stop() {
        let mut bridge = Bridge::disabled();
        let mut err = Vec::new();
        for data in [Some(odom()), None] {
            let event = NodeEvent::Input { id: "pose".into(), data };
            assert_eq!(bridge.on_event(event, &mut err), Flow::Continue);
        }
        assert_eq!(bridge.on_event(NodeEvent::Other, &mut err), Flow::Continue);
        assert!(err.is_empty(), "the cause was reported at startup: {}", String::from_utf8_lossy(&err));
        assert_eq!(bridge.on_event(NodeEvent::Stop, &mut err), Flow::Stop);
        assert!(bridge.wait(Instant::now()), "nothing to wait for");
        assert_eq!(bridge.summary(), "nav-bridge: 2 inputs, all dropped (not running)");
    }
}
