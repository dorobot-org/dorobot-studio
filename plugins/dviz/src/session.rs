//! The process-wide dviz Zenoh session.
//!
//! One background thread per connection opens Zenoh (never the caller), then
//! drains a bounded queue that the subscriber callback fills. The callback
//! only queues `(topic, payload)`, so it never blocks Zenoh; when the queue is
//! full (`RING_CAP` payloads or `INBOX_BYTES_CAP` bytes) the oldest payloads
//! are evicted and counted as dropped. A sink that panics is replaced by a
//! discarding one; receiving goes on. Only that thread calls the sink: other
//! sink work (clearing an entity switched off, sending the robot body model)
//! is queued in the state and done there before the next message.
//!
//! Navigation commands go out as Zenoh queries on the receiver's session
//! (`DvizSession::send_command`), each on a short-lived thread, and their
//! replies land in the snapshot's `command`. The robots they go to are the
//! ids seen on `<prefix>/<id>/nav/status` since the connection started.

use crate::nav_scene::NavScene;
use crate::route::{self, EntityInfo, SessionState, Sink, SinkAction, Visual};
use dviz_protocol::{
    cmd_key, parse_cmd_ack, CmdAck, CmdRequest, GraphEdge, GraphNode, LogEntry, MvizMessage,
};
use std::{
    any::Any,
    collections::{BTreeMap, VecDeque},
    fmt::Display,
    io::Read,
    mem,
    panic::{self, AssertUnwindSafe},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError, Weak,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use zenoh::{
    bytes::ZBytes,
    key_expr::KeyExpr,
    qos::{CongestionControl, Priority},
    query::Reply,
    sample::Sample,
    Wait,
};

/// Payloads queued between the Zenoh callback and the worker.
pub const RING_CAP: usize = 64;
/// Most payload bytes queued between the Zenoh callback and the worker.
pub const INBOX_BYTES_CAP: usize = 64 * 1024 * 1024;
/// Larger payloads are refused in the callback and counted as malformed.
const MAX_PAYLOAD: usize = 8 * 1024 * 1024;
/// Zenoh's connect timeout, and how long named connect endpoints may stay
/// unreachable before `last_error` says so.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(1);
/// How often the receiver checks whether a peer or router is linked.
const LINK_CHECK: Duration = Duration::from_millis(100);
const RETRY_DELAY: Duration = Duration::from_secs(1);
const POLL: Duration = Duration::from_millis(50);
/// How long a command query waits for the robot's reply.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(3);
/// Longest token file read; a longer one is no token.
const TOKEN_FILE_MAX: u64 = 4096;

/// `CommandOutcome::NotConnected`: no Zenoh session.
pub const NOT_CONNECTED: &str = "not connected";
/// `CommandOutcome::NotConnected`: connected, but no robot status yet.
pub const NO_ROBOT_SEEN: &str = "no robot seen yet";
/// `CommandOutcome::NotConnected`: a goal or pose estimate with several
/// robots on the connection.
pub const MORE_THAN_ONE_ROBOT: &str = "more than one robot on this connection";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionOptions {
    /// `"peer"` or `"client"`; client mode needs a connect endpoint.
    pub mode: String,
    /// Empty in peer mode (and no listen endpoints) means multicast discovery.
    pub connect_endpoints: Vec<String>,
    pub listen_endpoints: Vec<String>,
    /// Subscribes to `<prefix>/**`; entity paths are topics without `<prefix>/`.
    pub prefix: String,
}

impl Default for SessionOptions {
    fn default() -> Self {
        Self {
            mode: "peer".into(),
            connect_endpoints: Vec::new(),
            listen_endpoints: Vec::new(),
            prefix: "dviz".into(),
        }
    }
}

/// What "Open 3D window" last did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ViewerState {
    /// Not opened in this process yet.
    #[default]
    Closed,
    /// A launch runs on a background thread.
    Starting,
    /// Visual messages go to this viewer (its name and version).
    Open(String),
    /// Why the last launch failed (the previous sink, if any, stays), or
    /// that the sink panicked (it was replaced by a discarding one).
    Failed(String),
}

/// A copy of what the panels show.
#[derive(Debug, Clone, Default)]
pub struct SessionSnapshot {
    /// Zenoh is open and linked to at least one peer or router, in every
    /// mode: an open session alone is not a connection.
    pub connected: bool,
    /// How long the open session has waited for its first peer. `Some` only
    /// without connect endpoints (multicast discovery, or listening), while
    /// no peer or router is linked; that is not an error.
    pub listening_for: Option<Duration>,
    pub last_error: Option<String>,
    /// Successful connections so far.
    pub generation: u64,
    /// Receiver threads running (a new one starts counting only after its
    /// predecessor has ended), so never more than 1.
    pub receivers_alive: u32,
    pub logs: Vec<LogEntry>,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub entities: BTreeMap<String, EntityInfo>,
    pub dropped: u64,
    pub malformed: u64,
    /// Visual messages and clears the sink rejected or panicked on.
    pub sink_errors: u64,
    pub received: u64,
    pub nav: NavScene,
    pub viewer: ViewerState,
    /// The latest command sent, with its replies so far.
    pub command: Option<CommandRecord>,
    /// Whether the last connect read a command token (the token itself is
    /// never in the snapshot).
    pub has_token: bool,
    /// The robot ids seen on `<prefix>/<id>/nav/status` this connection,
    /// sorted; empty while disconnected.
    pub robots: Vec<String>,
}

/// A navigation command from the operator.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum NavCommand {
    /// Drive to (`x`, `y`) in the map frame, facing `yaw` (rad) if given.
    Goal { x: f64, y: f64, yaw: Option<f64> },
    /// Tell localization the robot is at (`x`, `y`) facing `yaw`.
    InitialPose { x: f64, y: f64, yaw: f64 },
    /// Latch the robot's e-stop.
    Estop,
    /// Disarm the robot.
    Disarm,
}

impl NavCommand {
    /// The request's `cmd`.
    fn name(self) -> &'static str {
        match self {
            NavCommand::Goal { .. } => "goal",
            NavCommand::InitialPose { .. } => "initialpose",
            NavCommand::Estop => "estop",
            NavCommand::Disarm => "disarm",
        }
    }

    /// E-STOP and Disarm: to every robot seen, without the token, at the
    /// highest priority and never dropped under congestion.
    fn stops(self) -> bool {
        matches!(self, NavCommand::Estop | NavCommand::Disarm)
    }

    fn request(self, id: &str, token: Option<String>) -> CmdRequest {
        let (x, y, yaw) = match self {
            NavCommand::Goal { x, y, yaw } => (Some(x), Some(y), yaw),
            NavCommand::InitialPose { x, y, yaw } => (Some(x), Some(y), Some(yaw)),
            NavCommand::Estop | NavCommand::Disarm => (None, None, None),
        };
        CmdRequest {
            cmd: self.name().to_string(),
            id: id.to_string(),
            token,
            x,
            y,
            yaw,
        }
    }
}

/// What became of a command at one robot.
#[derive(Debug, Clone, PartialEq)]
pub enum CommandOutcome {
    /// Sent; no reply yet.
    Pending,
    /// The robot's bridge replied.
    Ack(CmdAck),
    /// Not sent, and why: `NOT_CONNECTED`, `NO_ROBOT_SEEN` or
    /// `MORE_THAN_ONE_ROBOT`.
    NotConnected(&'static str),
    /// The query ended (after 3 s at most) without a valid reply.
    Timeout,
}

/// A command and its replies so far.
#[derive(Debug, Clone, PartialEq)]
pub struct CommandRecord {
    /// The request id; see `command_id`.
    pub id: String,
    /// `goal`, `initialpose`, `estop` or `disarm`.
    pub cmd: &'static str,
    pub sent: Instant,
    /// One entry per robot the command went to, sorted by robot. A command
    /// that was not sent has a single entry: an empty robot and
    /// `NotConnected`.
    pub replies: Vec<(String, CommandOutcome)>,
}

/// The operator's command token in `path`, surrounding whitespace removed;
/// `None` if the file is missing, unreadable, not UTF-8, longer than 4 KiB
/// or blank.
pub fn load_cmd_token(path: &Path) -> Option<String> {
    let mut text = String::new();
    std::fs::File::open(path)
        .ok()?
        .take(TOKEN_FILE_MAX + 1)
        .read_to_string(&mut text)
        .ok()?;
    if text.len() as u64 > TOKEN_FILE_MAX {
        return None;
    }
    let token = text.trim();
    (!token.is_empty()).then(|| token.to_string())
}

/// The request id `s<seq>-<suffix>`: `suffix` is `started_unix_s` modulo
/// 36⁴ as four lowercase base-36 digits (`s3-k9xq`). `seq` counts the
/// commands of this process from 1, and the suffix makes a restarted Studio
/// most likely use other ids.
pub fn command_id(seq: u64, started_unix_s: u64) -> String {
    const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut rest = started_unix_s % 36u64.pow(4);
    let mut suffix = [b'0'; 4];
    for digit in suffix.iter_mut().rev() {
        *digit = DIGITS[(rest % 36) as usize];
        rest /= 36;
    }
    let suffix: String = suffix.iter().map(|&digit| char::from(digit)).collect();
    format!("s{seq}-{suffix}")
}

/// Unix seconds when this process first made a session.
fn process_started_unix_s() -> u64 {
    static STARTED: OnceLock<u64> = OnceLock::new();
    *STARTED.get_or_init(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_secs())
    })
}

/// `$HOME/.config/dviz/cmd_token`, if `HOME` is set.
fn default_token_path() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(|home| PathBuf::from(home).join(".config/dviz/cmd_token"))
}

pub struct DvizSession {
    shared: Arc<Shared>,
    control: Mutex<Control>,
}

/// Lock order: `state` before `cmd` when both are held (only
/// `send_command` holds both). Command reply callbacks take `cmd` alone, and
/// nothing calls into Zenoh while holding `cmd`: Zenoh may drop a callback,
/// which takes `cmd`, inside a call.
struct Shared {
    state: Mutex<SessionState>,
    sink: Mutex<Box<dyn Sink>>,
    link: Mutex<Link>,
    receivers_alive: AtomicU32,
    viewer: Mutex<ViewerState>,
    /// In its own `Arc` so that reply callbacks hold a weak reference to
    /// this alone, not to the sink and the rest.
    cmd: Arc<Mutex<CmdLink>>,
}

/// What sending commands needs; see `DvizSession::send_command`.
#[derive(Default)]
struct CmdLink {
    /// The running receiver's open Zenoh session and its topic prefix.
    session: Option<(zenoh::Session, String)>,
    /// The operator's token, read at every connect. Never shown or logged.
    token: Option<String>,
    /// Where `connect` reads the token; `None` is `default_token_path`.
    token_path: Option<PathBuf>,
    /// The latest command.
    last: Option<CommandRecord>,
    /// Commands made so far; the next id's sequence number minus 1.
    seq: u64,
}

#[derive(Default)]
struct Link {
    connected: bool,
    /// Set while the open session waits for its first peer without connect
    /// endpoints; see `SessionSnapshot::listening_for`.
    listening_since: Option<Instant>,
    last_error: Option<String>,
    generation: u64,
}

#[derive(Default)]
struct Control {
    /// Stop flag of the current receiver, `None` when disconnected.
    stop: Option<Arc<AtomicBool>>,
    /// The latest receiver thread, possibly still finishing after a stop;
    /// the next receiver joins it before it starts.
    thread: Option<JoinHandle<()>>,
    /// The options of the latest connect, kept across a disconnect.
    options: Option<SessionOptions>,
}

/// Visual data goes nowhere until a real sink is set.
struct DiscardSink;

impl Sink for DiscardSink {
    fn log(&mut self, _: &str, _: &MvizMessage, _: Option<&[u8]>) -> Result<(), String> {
        Ok(())
    }
}

impl Default for DvizSession {
    fn default() -> Self {
        Self::new()
    }
}

impl DvizSession {
    /// The session shared by every panel in this process.
    pub fn global() -> &'static DvizSession {
        static GLOBAL: OnceLock<DvizSession> = OnceLock::new();
        GLOBAL.get_or_init(DvizSession::new)
    }

    /// An independent session; for tests.
    pub fn new() -> DvizSession {
        // Pins the command id suffix to when the first session was made.
        process_started_unix_s();
        DvizSession {
            shared: Arc::new(Shared {
                state: Mutex::new(SessionState::default()),
                sink: Mutex::new(Box::new(DiscardSink)),
                link: Mutex::new(Link::default()),
                receivers_alive: AtomicU32::new(0),
                viewer: Mutex::new(ViewerState::Closed),
                cmd: Arc::default(),
            }),
            control: Mutex::new(Control::default()),
        }
    }

    /// Starts receiving with `options`, replacing any current connection.
    /// Returns at once: Zenoh opens on the receiver thread, and its failures
    /// land in `last_error`. Only invalid options fail here. Options other
    /// than the last connect's (mode, endpoints or prefix) start from a clear
    /// state: no entities, logs, nodes or edges, and every counter at 0.
    /// Robots are forgotten either way, and the command token is read again.
    pub fn connect(&self, options: SessionOptions) -> Result<(), String> {
        if let Err(error) = validate(&options) {
            lock(&self.shared.link).last_error = Some(error.clone());
            return Err(error);
        }
        self.load_token();
        let mut control = lock(&self.control);
        self.stop_current(&mut control);
        // After the stop: a receiver checks its flag under the state lock
        // before it writes, so the stopped one cannot refill this.
        let elsewhere = matches!(&control.options, Some(last) if *last != options);
        {
            let mut state = lock(&self.shared.state);
            if elsewhere {
                *state = SessionState::default();
            } else {
                state.start_connection();
            }
        }
        control.options = Some(options.clone());
        // The new thread waits for the previous one, so at most one runs.
        let previous = control.thread.take();
        let stop = Arc::new(AtomicBool::new(false));
        let shared = self.shared.clone();
        let thread_stop = stop.clone();
        let thread = thread::Builder::new()
            .name("dviz-zenoh".into())
            .spawn(move || run(&shared, &options, &thread_stop, previous))
            .map_err(|error| {
                let error = format!("could not start the dviz receiver: {error}");
                lock(&self.shared.link).last_error = Some(error.clone());
                error
            })?;
        control.stop = Some(stop);
        control.thread = Some(thread);
        Ok(())
    }

    /// Stops receiving and returns at once. The receiver thread ends within
    /// about the connect timeout (1 s) plus the session close; a later
    /// `connect` waits for it, so two receivers never run together.
    pub fn disconnect(&self) {
        self.stop_current(&mut lock(&self.control));
    }

    /// Where visual messages go from now on; the robot body model, if a
    /// status named one, is sent to it again. Does not wait for the previous
    /// sink to go away: dropping a Rerun stream flushes it, which can take
    /// seconds while its viewer is still starting, so it is dropped on a
    /// short-lived thread, after the sink lock is released.
    pub fn set_sink(&self, sink: Box<dyn Sink>) {
        drop_elsewhere(replace_sink(&self.shared, sink));
    }

    /// Opens a viewer and returns at once: `launch` runs on a short-lived
    /// thread and returns the viewer's sink with its name and version, or
    /// why there is none. The outcome lands in the snapshot's `viewer`. On
    /// success the new sink replaces the current one (as `set_sink` does),
    /// and the old one is dropped on that thread; on failure the current
    /// sink stays. Refused, with nothing changed, while an earlier launch
    /// still runs.
    pub fn start_viewer<F>(&self, launch: F) -> Result<(), String>
    where
        F: FnOnce() -> Result<(Box<dyn Sink>, String), String> + Send + 'static,
    {
        {
            let mut viewer = lock(&self.shared.viewer);
            if *viewer == ViewerState::Starting {
                return Err("The 3D window is already starting".into());
            }
            *viewer = ViewerState::Starting;
        }
        let shared = self.shared.clone();
        let spawned = thread::Builder::new()
            .name("dviz-viewer".into())
            .spawn(move || {
                // Ends `Starting` even if `launch` panics.
                let mut report = ViewerReport {
                    shared: &shared,
                    state: ViewerState::Failed("The 3D window launch stopped unexpectedly".into()),
                };
                let previous = match launch() {
                    Ok((sink, name)) => {
                        report.state = ViewerState::Open(name);
                        Some(replace_sink(&shared, sink))
                    }
                    Err(error) => {
                        report.state = ViewerState::Failed(error);
                        None
                    }
                };
                drop(report);
                // A Rerun stream flushes when dropped, which may take a while.
                drop(previous);
            });
        if let Err(error) = spawned {
            let error = format!("Could not start the 3D window: {error}");
            *lock(&self.shared.viewer) = ViewerState::Failed(error.clone());
            return Err(error);
        }
        Ok(())
    }

    /// Turns sink delivery for `entity` on or off. Switching it off only
    /// queues a clear: the receiver thread hands it to the sink before its
    /// next message, or within `POLL` when idle, so this never waits for the
    /// sink. While no receiver runs (disconnected), it waits for the next one.
    pub fn set_entity_enabled(&self, entity: &str, enabled: bool) {
        lock(&self.shared.state).set_entity_enabled(entity, enabled);
    }

    pub fn snapshot(&self) -> SessionSnapshot {
        let (connected, listening_for, last_error, generation) = {
            let link = lock(&self.shared.link);
            (
                link.connected,
                link.listening_since.map(|since| since.elapsed()),
                link.last_error.clone(),
                link.generation,
            )
        };
        let receivers_alive = self.shared.receivers_alive.load(Ordering::Relaxed);
        let viewer = lock(&self.shared.viewer).clone();
        let (command, has_token) = {
            let cmd = lock(&self.shared.cmd);
            (cmd.last.clone(), cmd.token.is_some())
        };
        let state = lock(&self.shared.state);
        SessionSnapshot {
            connected,
            listening_for,
            last_error,
            generation,
            receivers_alive,
            logs: state.logs.iter().cloned().collect(),
            nodes: state.nodes.clone(),
            edges: state.edges.clone(),
            entities: state.entities.clone(),
            dropped: state.dropped,
            malformed: state.malformed,
            sink_errors: state.sink_errors,
            received: state.received,
            nav: state.nav.clone(),
            viewer,
            command,
            has_token,
            robots: state.robots.iter().cloned().collect(),
        }
    }

    /// Sends `command` and returns its request id at once; what becomes of
    /// it lands in the snapshot's `command`, which it replaces. Never waits
    /// for the network: each robot's query is issued on a short-lived
    /// thread, since with blocking congestion control Zenoh may wait up to
    /// 5 s (`wait_before_close`) for room on a congested link.
    ///
    /// - Without a Zenoh session the record says `NotConnected("not
    ///   connected")`; before any robot status, `NotConnected("no robot seen
    ///   yet")`.
    /// - E-STOP and Disarm go to every robot seen, without the token, with
    ///   `Priority::RealTime` and `CongestionControl::Block`.
    /// - A goal or pose estimate carries the token and needs exactly one
    ///   robot; with more, nothing is sent and the record says
    ///   `NotConnected("more than one robot on this connection")`.
    /// - A robot's reply becomes `Ack`; a query that ends without a valid
    ///   one (3 s timeout) becomes `Timeout`.
    pub fn send_command(&self, command: NavCommand) -> String {
        let shared = &self.shared;
        // Lock order: state, then cmd. Both held, the robots and the
        // session belong to the same connection.
        let state = lock(&shared.state);
        let mut cmd = lock(&shared.cmd);
        let robots: Vec<String> = state.robots.iter().cloned().collect();
        drop(state);
        cmd.seq += 1;
        let id = command_id(cmd.seq, process_started_unix_s());
        let mut record = CommandRecord {
            id: id.clone(),
            cmd: command.name(),
            sent: Instant::now(),
            replies: Vec::new(),
        };
        let target = match &cmd.session {
            None => Err(NOT_CONNECTED),
            Some(_) if robots.is_empty() => Err(NO_ROBOT_SEEN),
            Some(_) if robots.len() > 1 && !command.stops() => Err(MORE_THAN_ONE_ROBOT),
            Some((session, prefix)) => Ok((session.clone(), prefix.clone())),
        };
        let (session, prefix) = match target {
            Ok(target) => target,
            Err(reason) => {
                record.replies = vec![(String::new(), CommandOutcome::NotConnected(reason))];
                cmd.last = Some(record);
                return id;
            }
        };
        let token = if command.stops() {
            None
        } else {
            cmd.token.clone()
        };
        let payload = serde_json::to_vec(&command.request(&id, token)).unwrap_or_default();
        // In place before any query, so every reply finds it.
        record.replies = robots
            .iter()
            .map(|robot| (robot.clone(), CommandOutcome::Pending))
            .collect();
        cmd.last = Some(record);
        drop(cmd);

        let queries: Vec<CommandQuery> = robots
            .into_iter()
            .map(|robot| CommandQuery {
                session: session.clone(),
                key: cmd_key(&prefix, &robot),
                payload: payload.clone(),
                stops: command.stops(),
                target: ReplyTarget {
                    cmd: Arc::downgrade(&shared.cmd),
                    id: id.clone(),
                    robot,
                },
            })
            .collect();
        // The queries hold handles too, and the receiver closes the session
        // itself, so dropping this one never closes it here.
        drop(session);
        for query in queries {
            run_elsewhere("dviz-cmd", move || query.send());
        }
        id
    }

    /// Where `connect` reads the command token, instead of
    /// `$HOME/.config/dviz/cmd_token`.
    #[cfg(test)]
    pub(crate) fn set_token_path(&self, path: PathBuf) {
        lock(&self.shared.cmd).token_path = Some(path);
    }

    /// Reads the command token again (also when it is gone now).
    fn load_token(&self) {
        let path = lock(&self.shared.cmd)
            .token_path
            .clone()
            .or_else(default_token_path);
        // Read without the lock.
        let token = path.as_deref().and_then(load_cmd_token);
        lock(&self.shared.cmd).token = token;
    }

    fn stop_current(&self, control: &mut Control) {
        if let Some(stop) = control.stop.take() {
            stop.store(true, Ordering::Relaxed);
        }
        // After the flag: a receiver checks it under this lock before it
        // offers its session, so no command goes out on a stopped one. The
        // receiver closes its session itself, so dropping this handle never
        // closes it here.
        let session = lock(&self.shared.cmd).session.take();
        drop(session);
        // After the flag: a receiver checks it under this lock before it
        // reports, so a stopped receiver can no longer overwrite this.
        let mut link = lock(&self.shared.link);
        link.connected = false;
        link.listening_since = None;
        link.last_error = None;
        drop(link);
        // A receiver checks the flag under the state lock before it routes,
        // so a stopped one cannot add robots again.
        lock(&self.shared.state).robots.clear();
    }
}

impl Drop for DvizSession {
    fn drop(&mut self) {
        let control = self
            .control
            .get_mut()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(stop) = control.stop.take() {
            stop.store(true, Ordering::Relaxed);
        }
    }
}

fn validate(options: &SessionOptions) -> Result<(), String> {
    match options.mode.as_str() {
        "peer" => {}
        "client" if !options.connect_endpoints.is_empty() => {}
        "client" => return Err("Zenoh client mode needs a connect endpoint".into()),
        other => return Err(format!("Zenoh mode must be peer or client, not {other:?}")),
    }
    KeyExpr::try_from(format!("{}/**", options.prefix))
        .map(drop)
        .map_err(|error| format!("invalid topic prefix {:?}: {error}", options.prefix))
}

/// Body of the receiver thread: (re)opens Zenoh until stopped.
fn run(
    shared: &Shared,
    options: &SessionOptions,
    stop: &AtomicBool,
    previous: Option<JoinHandle<()>>,
) {
    if let Some(previous) = previous {
        let _ = previous.join();
    }
    shared.receivers_alive.fetch_add(1, Ordering::Relaxed);
    let _alive = AliveGuard(&shared.receivers_alive);
    while !stop.load(Ordering::Relaxed) {
        if let Err(error) = receive(shared, options, stop) {
            let error = describe(options, error);
            report(shared, stop, |link| {
                link.connected = false;
                link.listening_since = None;
                link.last_error = Some(error);
            });
        }
        let retry_at = Instant::now() + RETRY_DELAY;
        while Instant::now() < retry_at && !stop.load(Ordering::Relaxed) {
            // Switch-offs still clear while Zenoh is down.
            let actions = take_actions(shared, stop);
            apply(shared, stop, &actions, None);
            thread::sleep(POLL);
        }
    }
}

/// Sets the viewer state when dropped, also when a launch panics.
struct ViewerReport<'a> {
    shared: &'a Shared,
    state: ViewerState,
}

impl Drop for ViewerReport<'_> {
    fn drop(&mut self) {
        *lock(&self.shared.viewer) = mem::take(&mut self.state);
    }
}

struct AliveGuard<'a>(&'a AtomicU32);

impl Drop for AliveGuard<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Makes `sink` the current one and returns the previous one. The new sink
/// has not drawn the robot body model, so that is queued for it again. One
/// lock at a time: the sink's, then the state's.
fn replace_sink(shared: &Shared, sink: Box<dyn Sink>) -> Box<dyn Sink> {
    let previous = mem::replace(&mut *lock(&shared.sink), sink);
    lock(&shared.state).queue_model();
    previous
}

/// Drops `sink` on a short-lived thread: dropping a Rerun stream flushes it,
/// which can take seconds. If no thread can be started, it is dropped here
/// after all.
fn drop_elsewhere(sink: Box<dyn Sink>) {
    let _ = thread::Builder::new()
        .name("dviz-sink-drop".into())
        .spawn(move || drop(sink));
}

/// Updates the link status unless this receiver has been stopped.
fn report(shared: &Shared, stop: &AtomicBool, update: impl FnOnce(&mut Link)) {
    let mut link = lock(&shared.link);
    if !stop.load(Ordering::Relaxed) {
        update(&mut link);
    }
}

/// One Zenoh session: open, subscribe, route until stopped.
fn receive(shared: &Shared, options: &SessionOptions, stop: &AtomicBool) -> zenoh::Result<()> {
    let session = zenoh::open(zenoh_config(options)?).wait()?;
    let inbox = Arc::new(Inbox::default());
    let callback_inbox = inbox.clone();
    let subscriber = session
        .declare_subscriber(format!("{}/**", options.prefix))
        .callback(move |sample| callback_inbox.push(sample))
        .wait()?;
    let offered = CommandSession::offer(shared, stop, &session, &options.prefix);

    let mut watch = LinkWatch::new(options);
    while !stop.load(Ordering::Relaxed) {
        watch.check(shared, stop, &session);
        let (next, dropped, oversized) = inbox.next(POLL);
        let item = next
            .as_ref()
            .map(|(topic, payload)| (topic, payload.to_bytes()));
        let (actions, visual) = {
            let mut state = lock(&shared.state);
            // Checked under the lock: once `connect` has stopped this
            // receiver, it may have cleared the state for other options.
            if stop.load(Ordering::Relaxed) {
                break;
            }
            state.dropped += dropped;
            state.malformed += oversized;
            let visual = match &item {
                Some((topic, bytes)) => route::accept(&mut state, &options.prefix, topic, bytes),
                None => None,
            };
            // After `accept`, which may queue the body model a status names.
            let actions = state.take_sink_actions();
            (actions, visual)
        };
        // The sink may be slow; the state lock is not held across it.
        apply(shared, stop, &actions, visual.as_ref());
    }
    // No new command on a closing session (also needed when the
    // `DvizSession` is dropped, which only sets the flag).
    drop(offered);
    // Closed in any case, so a command thread still holding a handle never
    // has to close it.
    let undeclared = subscriber.undeclare().wait();
    session.close().wait()?;
    undeclared
}

/// Offers the receiver's session for commands while it lives, and takes it
/// back when dropped, also when the receiver panics.
struct CommandSession<'a>(&'a Mutex<CmdLink>);

impl<'a> CommandSession<'a> {
    /// Checked under the lock: once `connect` or `disconnect` has stopped
    /// this receiver, it has also taken the session back, so a stopped
    /// receiver never offers it.
    fn offer(
        shared: &'a Shared,
        stop: &AtomicBool,
        session: &zenoh::Session,
        prefix: &str,
    ) -> Self {
        let mut cmd = lock(&shared.cmd);
        if !stop.load(Ordering::Relaxed) {
            cmd.session = Some((session.clone(), prefix.to_string()));
        }
        CommandSession(&shared.cmd)
    }
}

impl Drop for CommandSession<'_> {
    fn drop(&mut self) {
        // Outside the lock. The receiver closes the session itself.
        let session = lock(self.0).session.take();
        drop(session);
    }
}

/// One robot's command query.
struct CommandQuery {
    session: zenoh::Session,
    key: String,
    /// The request JSON; it may hold the token, so it is never logged.
    payload: Vec<u8>,
    stops: bool,
    target: ReplyTarget,
}

impl CommandQuery {
    /// Issues the query; returns once Zenoh has taken it, not on its reply.
    fn send(self) {
        let CommandQuery {
            session,
            key,
            payload,
            stops,
            target,
        } = self;
        let guard = ReplyGuard(target.clone());
        let mut get = session.get(key).payload(payload).timeout(COMMAND_TIMEOUT);
        if stops {
            get = get
                .priority(Priority::RealTime)
                .congestion_control(CongestionControl::Block);
        }
        // Zenoh drops the callback, and the guard with it, once the query
        // has ended: after the final reply, at the timeout, or when the
        // session closes. On an error it has already dropped it.
        let issued = get.callback(move |reply| guard.0.reply(&reply)).wait();
        if issued.is_err() {
            // The session closed under it: the command never went out.
            target.update(|outcome| *outcome = CommandOutcome::NotConnected(NOT_CONNECTED));
        }
    }
}

/// Where a reply to one robot goes: that robot's entry of the latest
/// command, if it is still the command with `id`.
#[derive(Clone)]
struct ReplyTarget {
    cmd: Weak<Mutex<CmdLink>>,
    id: String,
    robot: String,
}

impl ReplyTarget {
    fn update(&self, change: impl FnOnce(&mut CommandOutcome)) {
        let Some(cmd) = self.cmd.upgrade() else {
            return;
        };
        let mut cmd = lock(&cmd);
        let Some(record) = cmd.last.as_mut().filter(|record| record.id == self.id) else {
            // A newer command has replaced it.
            return;
        };
        if let Some((_, outcome)) = record
            .replies
            .iter_mut()
            .find(|(robot, _)| *robot == self.robot)
        {
            change(outcome);
        }
    }

    /// A reply from Zenoh: a valid ack to this request becomes the outcome.
    /// Anything else (Zenoh's timeout error, an error reply, a malformed or
    /// foreign ack) leaves it to the guard.
    fn reply(&self, reply: &Reply) {
        let Ok(sample) = reply.result() else {
            return;
        };
        let Ok(ack) = parse_cmd_ack(&sample.payload().to_bytes()) else {
            return;
        };
        if ack.id == self.id {
            self.update(|outcome| *outcome = CommandOutcome::Ack(ack));
        }
    }
}

/// Owned by a query's callback. Dropped with it when the query has ended:
/// a robot still `Pending` then has had no valid reply, so `Timeout`.
struct ReplyGuard(ReplyTarget);

impl Drop for ReplyGuard {
    fn drop(&mut self) {
        self.0.update(|outcome| {
            if *outcome == CommandOutcome::Pending {
                *outcome = CommandOutcome::Timeout;
            }
        });
    }
}

/// Runs `job` on a short-lived thread named `name`, or right here if no
/// thread can be started: a command must go out even then.
fn run_elsewhere(name: &str, job: impl FnOnce() + Send + 'static) {
    let slot = Arc::new(Mutex::new(Some(job)));
    let thread_slot = slot.clone();
    let spawned = thread::Builder::new().name(name.into()).spawn(move || {
        let job = lock(&thread_slot).take();
        if let Some(job) = job {
            job();
        }
    });
    if spawned.is_err() {
        let job = lock(&slot).take();
        if let Some(job) = job {
            job();
        }
    }
}

/// The queued sink work, unless this receiver has been stopped (the state
/// may then belong to other options).
fn take_actions(shared: &Shared, stop: &AtomicBool) -> VecDeque<SinkAction> {
    let mut state = lock(&shared.state);
    if stop.load(Ordering::Relaxed) {
        return VecDeque::new();
    }
    state.take_sink_actions()
}

/// Hands `actions` and then `visual` to the sink, without the state lock,
/// and counts what failed in `sink_errors`. The actions go first: they were
/// queued before `visual` was accepted, or while accepting it, so a clear
/// only removes older data, also when its entity has been switched on again
/// since.
fn apply(
    shared: &Shared,
    stop: &AtomicBool,
    actions: &VecDeque<SinkAction>,
    visual: Option<&Visual<'_>>,
) {
    let mut failed = 0;
    for action in actions {
        if !with_sink(shared, |sink| action.apply(sink)) {
            failed += 1;
        }
    }
    if let Some(visual) = visual {
        if !with_sink(shared, |sink| {
            sink.log(&visual.entity, &visual.msg, visual.binary)
        }) {
            failed += 1;
        }
    }
    if failed > 0 {
        let mut state = lock(&shared.state);
        if !stop.load(Ordering::Relaxed) {
            state.sink_errors += failed;
        }
    }
}

/// Runs `call` on the sink; false if the sink rejected it or panicked. A
/// sink that panics is replaced by a discarding one and the 3D window is
/// reported as failed, so the receiver goes on.
fn with_sink(shared: &Shared, call: impl FnOnce(&mut dyn Sink) -> Result<(), String>) -> bool {
    let (crashed, message) = {
        let mut sink = lock(&shared.sink);
        let outcome = panic::catch_unwind(AssertUnwindSafe(|| call(&mut **sink)));
        match outcome {
            Ok(delivered) => return delivered.is_ok(),
            // Under the same lock, so this is the sink that panicked.
            Err(panic) => (
                mem::replace(&mut *sink, Box::new(DiscardSink)),
                panic_message(&*panic),
            ),
        }
    };
    drop_elsewhere(crashed);
    let mut viewer = lock(&shared.viewer);
    // A launch under way swaps in its own sink and reports its outcome;
    // `Starting` also keeps a second launch out until then.
    if *viewer != ViewerState::Starting {
        *viewer = ViewerState::Failed(format!("3D sink crashed: {message}"));
    }
    false
}

fn panic_message(panic: &(dyn Any + Send)) -> String {
    if let Some(message) = panic.downcast_ref::<&str>() {
        message.to_string()
    } else if let Some(message) = panic.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic".into()
    }
}

/// Reports the session as connected only while it is linked: a peer or
/// router transport exists. A peer session opens without one (and keeps
/// connecting in the background), so an open session alone proves nothing.
/// Without connect endpoints, waiting for the first peer is normal and is
/// reported as listening; with them, staying unlinked past the connect
/// timeout is an error.
struct LinkWatch<'a> {
    options: &'a SessionOptions,
    /// The state last reported; `None` before the first check.
    linked: Option<bool>,
    unlinked_since: Instant,
    error_shown: bool,
    next_check: Instant,
}

impl<'a> LinkWatch<'a> {
    fn new(options: &'a SessionOptions) -> Self {
        let now = Instant::now();
        Self {
            options,
            linked: None,
            unlinked_since: now,
            error_shown: false,
            next_check: now,
        }
    }

    fn check(&mut self, shared: &Shared, stop: &AtomicBool, session: &zenoh::Session) {
        let now = Instant::now();
        if now < self.next_check {
            return;
        }
        self.next_check = now + LINK_CHECK;
        let linked = has_transport(session);
        let listening = self.options.connect_endpoints.is_empty();
        if self.linked != Some(linked) {
            if linked {
                report(shared, stop, |link| {
                    link.connected = true;
                    link.listening_since = None;
                    link.last_error = None;
                    link.generation += 1;
                });
            } else {
                self.unlinked_since = now;
                self.error_shown = false;
                report(shared, stop, |link| {
                    link.connected = false;
                    link.listening_since = listening.then_some(now);
                });
            }
            self.linked = Some(linked);
        } else if !linked
            && !listening
            && !self.error_shown
            && now - self.unlinked_since >= CONNECT_TIMEOUT
        {
            self.error_shown = true;
            let error = describe(self.options, "no peer or router reachable; still trying");
            report(shared, stop, |link| link.last_error = Some(error));
        }
    }
}

fn has_transport(session: &zenoh::Session) -> bool {
    let info = session.info();
    info.routers_zid().wait().next().is_some() || info.peers_zid().wait().next().is_some()
}

fn zenoh_config(options: &SessionOptions) -> zenoh::Result<zenoh::Config> {
    let mut config = zenoh::Config::default();
    config.insert_json5("mode", &serde_json::to_string(&options.mode)?)?;
    config.insert_json5(
        "connect/endpoints",
        &serde_json::to_string(&options.connect_endpoints)?,
    )?;
    if !options.listen_endpoints.is_empty() {
        config.insert_json5(
            "listen/endpoints",
            &serde_json::to_string(&options.listen_endpoints)?,
        )?;
    }
    // Discover peers only when no endpoint at all was given.
    let multicast = options.mode == "peer"
        && options.connect_endpoints.is_empty()
        && options.listen_endpoints.is_empty();
    config.insert_json5(
        "scouting/multicast/enabled",
        if multicast { "true" } else { "false" },
    )?;
    config.insert_json5("transport/shared_memory/enabled", "false")?;
    config.insert_json5(
        "connect/timeout_ms",
        &CONNECT_TIMEOUT.as_millis().to_string(),
    )?;
    Ok(config)
}

/// A Zenoh error, prefixed with the endpoints it concerns.
fn describe(options: &SessionOptions, error: impl Display) -> String {
    let mut endpoints = Vec::new();
    if !options.connect_endpoints.is_empty() {
        endpoints.push(format!("connect {}", options.connect_endpoints.join(", ")));
    }
    if !options.listen_endpoints.is_empty() {
        endpoints.push(format!("listen {}", options.listen_endpoints.join(", ")));
    }
    if endpoints.is_empty() {
        endpoints.push("multicast discovery".into());
    }
    format!(
        "Zenoh {} session ({}) failed: {error}",
        options.mode,
        endpoints.join("; ")
    )
}

/// The bounded queue between the Zenoh callback and the worker.
struct Inbox {
    pending: Mutex<Pending>,
    ready: Condvar,
    /// Most payload bytes queued; `INBOX_BYTES_CAP` by default.
    bytes_cap: usize,
}

#[derive(Default)]
struct Pending {
    queue: VecDeque<(String, ZBytes)>,
    /// Payload bytes in `queue`.
    bytes: usize,
    dropped: u64,
    oversized: u64,
}

impl Default for Inbox {
    fn default() -> Self {
        Self::new(INBOX_BYTES_CAP)
    }
}

impl Inbox {
    fn new(bytes_cap: usize) -> Self {
        Self {
            pending: Mutex::default(),
            ready: Condvar::new(),
            bytes_cap,
        }
    }

    /// Runs in the Zenoh callback: queue only.
    fn push(&self, sample: Sample) {
        if sample.payload().len() > MAX_PAYLOAD {
            lock(&self.pending).oversized += 1;
            return;
        }
        self.queue(
            sample.key_expr().as_str().to_string(),
            sample.payload().clone(),
        );
    }

    /// Queues `payload`, then evicts the oldest payloads while there are
    /// more than `RING_CAP` or more than `bytes_cap` bytes.
    fn queue(&self, topic: String, payload: ZBytes) {
        let mut pending = lock(&self.pending);
        pending.bytes += payload.len();
        pending.queue.push_back((topic, payload));
        while pending.queue.len() > RING_CAP || pending.bytes > self.bytes_cap {
            let Some((_, oldest)) = pending.queue.pop_front() else {
                break;
            };
            pending.bytes -= oldest.len();
            pending.dropped += 1;
        }
        drop(pending);
        self.ready.notify_one();
    }

    /// The oldest queued payload (waiting up to `timeout` for one), plus the
    /// dropped and oversized counts since the last call.
    fn next(&self, timeout: Duration) -> (Option<(String, ZBytes)>, u64, u64) {
        let mut pending = lock(&self.pending);
        if pending.queue.is_empty() {
            pending = self
                .ready
                .wait_timeout(pending, timeout)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        let next = pending.queue.pop_front();
        if let Some((_, payload)) = &next {
            pending.bytes -= payload.len();
        }
        (
            next,
            mem::take(&mut pending.dropped),
            mem::take(&mut pending.oversized),
        )
    }
}

/// Locks, ignoring poisoning: every guarded value stays consistent between
/// statements, so a panic elsewhere must not take the UI down with it.
fn lock<T: ?Sized>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drops as slowly as a Rerun stream flushing to a viewer that is still
    /// starting, then raises `dropped`.
    struct SlowDropSink {
        dropped: Arc<AtomicBool>,
    }

    impl Sink for SlowDropSink {
        fn log(&mut self, _: &str, _: &MvizMessage, _: Option<&[u8]>) -> Result<(), String> {
            Ok(())
        }
    }

    impl Drop for SlowDropSink {
        fn drop(&mut self) {
            thread::sleep(Duration::from_secs(2));
            self.dropped.store(true, Ordering::Relaxed);
        }
    }

    #[test]
    fn set_sink_does_not_drop_the_previous_sink_on_the_caller() {
        let session = DvizSession::new();
        let dropped = Arc::new(AtomicBool::new(false));
        session.set_sink(Box::new(SlowDropSink {
            dropped: dropped.clone(),
        }));

        let started = Instant::now();
        session.set_sink(Box::new(DiscardSink));
        let took = started.elapsed();
        assert!(took < Duration::from_millis(100), "set_sink took {took:?}");

        // Still dropped, just elsewhere.
        let deadline = Instant::now() + Duration::from_secs(5);
        while !dropped.load(Ordering::Relaxed) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(20));
        }
        assert!(
            dropped.load(Ordering::Relaxed),
            "the old sink was never dropped"
        );
    }

    /// Raises its flag when dropped.
    struct FlagSink(Arc<AtomicBool>);

    impl Sink for FlagSink {
        fn log(&mut self, _: &str, _: &MvizMessage, _: Option<&[u8]>) -> Result<(), String> {
            Ok(())
        }
    }

    impl Drop for FlagSink {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Relaxed);
        }
    }

    fn wait_for(timeout: Duration, mut done: impl FnMut() -> bool) -> bool {
        let deadline = Instant::now() + timeout;
        while !done() {
            if Instant::now() >= deadline {
                return false;
            }
            thread::sleep(Duration::from_millis(10));
        }
        true
    }

    #[test]
    fn start_viewer_launches_off_the_caller_and_swaps_the_sink_after() {
        let session = DvizSession::new();
        let old_dropped = Arc::new(AtomicBool::new(false));
        session.set_sink(Box::new(FlagSink(old_dropped.clone())));

        let started = Instant::now();
        let launched = session.start_viewer(|| {
            thread::sleep(Duration::from_millis(500));
            Ok((
                Box::new(DiscardSink) as Box<dyn Sink>,
                "Test 1.0".to_string(),
            ))
        });
        let took = started.elapsed();
        assert_eq!(launched, Ok(()));
        assert!(
            took < Duration::from_millis(100),
            "start_viewer took {took:?}"
        );
        assert_eq!(session.snapshot().viewer, ViewerState::Starting);
        assert!(
            !old_dropped.load(Ordering::Relaxed),
            "sink swapped too early"
        );

        // Refused while the first launch runs; its launch never runs.
        let second = session.start_viewer(|| Err("the second launch ran".to_string()));
        assert!(second.is_err());

        assert!(
            wait_for(Duration::from_secs(5), || session.snapshot().viewer
                == ViewerState::Open("Test 1.0".into())),
            "{:?}",
            session.snapshot().viewer
        );
        assert!(
            wait_for(Duration::from_secs(5), || old_dropped
                .load(Ordering::Relaxed)),
            "the previous sink was never replaced"
        );
    }

    #[test]
    fn failed_launch_is_reported_and_keeps_the_sink() {
        let session = DvizSession::new();
        let dropped = Arc::new(AtomicBool::new(false));
        session.set_sink(Box::new(FlagSink(dropped.clone())));

        assert_eq!(
            session.start_viewer(|| Err("no viewer here".to_string())),
            Ok(())
        );
        assert!(
            wait_for(Duration::from_secs(5), || session.snapshot().viewer
                == ViewerState::Failed("no viewer here".into())),
            "{:?}",
            session.snapshot().viewer
        );
        thread::sleep(Duration::from_millis(100));
        assert!(!dropped.load(Ordering::Relaxed), "the sink was replaced");

        // A failure does not block the next attempt.
        assert_eq!(
            session.start_viewer(|| Ok((Box::new(DiscardSink) as Box<dyn Sink>, "Test 2".into()))),
            Ok(())
        );
        assert!(wait_for(Duration::from_secs(5), || session
            .snapshot()
            .viewer
            == ViewerState::Open("Test 2".into())));
    }

    #[test]
    fn inbox_is_byte_bounded() {
        assert_eq!(Inbox::default().bytes_cap, INBOX_BYTES_CAP);

        // A 1 MiB cap stands in for the 64 MiB one. Twice the cap goes in, in
        // too few payloads for `RING_CAP` to evict any.
        const CAP: usize = 1024 * 1024;
        const CHUNK: usize = 64 * 1024;
        const PUSHED: usize = 2 * CAP / CHUNK;
        const KEPT: usize = CAP / CHUNK;
        assert!(PUSHED < RING_CAP);
        let inbox = Inbox::new(CAP);
        for n in 0..PUSHED {
            inbox.queue(
                "dviz/world/lidar".into(),
                ZBytes::from(vec![n as u8; CHUNK]),
            );
            let queued = lock(&inbox.pending).bytes;
            assert!(queued <= CAP, "{queued} bytes queued after payload {n}");
        }

        let (oldest, dropped, oversized) = inbox.next(Duration::ZERO);
        assert!(dropped > 0);
        assert_eq!(dropped, (PUSHED - KEPT) as u64);
        assert_eq!(oversized, 0);
        let (_, oldest) = oldest.expect("the newest payloads stay queued");
        assert_eq!(oldest.to_bytes()[0], KEPT as u8, "the oldest went first");
        assert_eq!(lock(&inbox.pending).bytes, CAP - CHUNK);
    }

    // ---- navigation commands -------------------------------------------

    use dviz_protocol::{parse_cmd_request, serialize_message, CmdAck, CmdRequest, CmdResult};
    use std::{
        net::TcpListener,
        path::{Path, PathBuf},
        sync::atomic::AtomicU64,
    };

    /// Opening a session, linking and the first status take well under this.
    const SETTLE: Duration = Duration::from_secs(5);
    /// `send_command` returns at once.
    const SEND_MAX: Duration = Duration::from_millis(50);

    fn free_port() -> u16 {
        TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    /// A fresh directory standing in for `$HOME`, removed when dropped.
    struct TempHome(PathBuf);

    impl TempHome {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let n = NEXT.fetch_add(1, Ordering::Relaxed);
            let dir =
                std::env::temp_dir().join(format!("dviz-session-test-{}-{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        /// Writes `text` to the token file below this home; returns its path.
        fn token(&self, text: &str) -> PathBuf {
            let path = self.0.join(".config/dviz/cmd_token");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, text).unwrap();
            path
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempHome {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn status_payload() -> Vec<u8> {
        let header = MvizMessage {
            msg_type: "status".into(),
            timestamp: Some(0.5),
            data: serde_json::json!({"loc_ok": true, "loc_reason": "", "armed": true,
                                     "estop": false, "gate_reason": ""}),
            format: None,
            count: None,
        };
        serialize_message(&header, None)
    }

    /// What the fake bridge's queryables do with a request.
    #[derive(Clone, Copy, PartialEq)]
    enum Queryables {
        /// None declared.
        Absent,
        /// Reply `ok` at once.
        Answer,
        /// Keep the query and never reply.
        Silent,
    }

    /// A peer listening on a free loopback port, standing in for robot
    /// bridges: it publishes a minimal `dviz/<robot>/nav/status` for each
    /// robot at 5 Hz and may declare `dviz/<robot>/cmd` queryables.
    struct FakeRobots {
        endpoint: String,
        /// `(robot, request)` for every query a queryable got.
        requests: Arc<Mutex<Vec<(String, CmdRequest)>>>,
        stop: Arc<AtomicBool>,
        publisher: Option<JoinHandle<()>>,
        _queryables: Vec<zenoh::query::Queryable<()>>,
        _held: Arc<Mutex<Vec<zenoh::query::Query>>>,
    }

    impl FakeRobots {
        fn requests(&self) -> Vec<(String, CmdRequest)> {
            lock(&self.requests).clone()
        }
    }

    impl Drop for FakeRobots {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            if let Some(publisher) = self.publisher.take() {
                let _ = publisher.join();
            }
        }
    }

    fn fake_robots(robots: &[&str], queryables: Queryables) -> FakeRobots {
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
        let peer = zenoh::open(config).wait().unwrap();

        let requests: Arc<Mutex<Vec<(String, CmdRequest)>>> = Arc::default();
        let held: Arc<Mutex<Vec<zenoh::query::Query>>> = Arc::default();
        let mut declared = Vec::new();
        if queryables != Queryables::Absent {
            for robot in robots {
                let (requests, held, robot) = (requests.clone(), held.clone(), robot.to_string());
                let queryable = peer
                    .declare_queryable(format!("dviz/{robot}/cmd"))
                    .callback(move |query| {
                        let bytes = query.payload().map(|p| p.to_bytes().into_owned());
                        let request = parse_cmd_request(&bytes.unwrap_or_default())
                            .expect("a valid command request");
                        let ack = CmdAck {
                            id: request.id.clone(),
                            cmd: request.cmd.clone(),
                            result: CmdResult::Ok,
                            reason: "sent to nav_command".into(),
                        };
                        lock(&requests).push((robot.clone(), request));
                        if queryables == Queryables::Silent {
                            lock(&held).push(query);
                            return;
                        }
                        query
                            .reply(query.key_expr().clone(), serde_json::to_vec(&ack).unwrap())
                            .wait()
                            .unwrap();
                    })
                    .wait()
                    .unwrap();
                declared.push(queryable);
            }
        }

        let stop = Arc::new(AtomicBool::new(false));
        let topics: Vec<String> = robots
            .iter()
            .map(|robot| format!("dviz/{robot}/nav/status"))
            .collect();
        let publisher = {
            let stop = stop.clone();
            thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    for topic in &topics {
                        let _ = peer.put(topic, status_payload()).wait();
                    }
                    thread::sleep(Duration::from_millis(200));
                }
            })
        };
        FakeRobots {
            endpoint,
            requests,
            stop,
            publisher: Some(publisher),
            _queryables: declared,
            _held: held,
        }
    }

    fn client_options(endpoint: &str) -> SessionOptions {
        SessionOptions {
            mode: "client".into(),
            connect_endpoints: vec![endpoint.to_string()],
            listen_endpoints: Vec::new(),
            prefix: "dviz".into(),
        }
    }

    /// A session connected as a client to `robots`, with the token file
    /// `token_path`; returns once it has seen every robot.
    fn connected_session(robots: &FakeRobots, names: &[&str], token_path: PathBuf) -> DvizSession {
        let session = DvizSession::new();
        session.set_token_path(token_path);
        session.connect(client_options(&robots.endpoint)).unwrap();
        assert!(
            wait_for(SETTLE, || session.snapshot().robots == names),
            "robots never seen: {:?}",
            session.snapshot()
        );
        session
    }

    /// `send_command`, checked to return at once.
    fn send(session: &DvizSession, command: NavCommand) -> String {
        let started = Instant::now();
        let id = session.send_command(command);
        let took = started.elapsed();
        assert!(took < SEND_MAX, "send_command took {took:?}");
        id
    }

    /// The latest command record, if it is the one with `id`.
    fn record(session: &DvizSession, id: &str) -> Option<CommandRecord> {
        session.snapshot().command.filter(|record| record.id == id)
    }

    fn outcomes(record: &CommandRecord) -> Vec<(&str, &CommandOutcome)> {
        record
            .replies
            .iter()
            .map(|(robot, outcome)| (robot.as_str(), outcome))
            .collect()
    }

    fn acked_ok(record: &CommandRecord) -> bool {
        !record.replies.is_empty()
            && record
                .replies
                .iter()
                .all(|(_, outcome)| matches!(outcome, CommandOutcome::Ack(ack) if ack.result == CmdResult::Ok))
    }

    const GOAL: NavCommand = NavCommand::Goal {
        x: 1.0,
        y: -2.0,
        yaw: Some(0.5),
    };

    #[test]
    fn studio_token_trims_whitespace() {
        let home = TempHome::new();
        assert_eq!(load_cmd_token(&home.token(" t0k\r\n")), Some("t0k".into()));
        assert_eq!(
            load_cmd_token(&home.token("\tab c\n\n")),
            Some("ab c".into())
        );
        assert_eq!(load_cmd_token(&home.token(" \r\n\t")), None);
        assert_eq!(load_cmd_token(&home.token("")), None);
        assert_eq!(load_cmd_token(&home.path().join("missing")), None);
        // A directory is no token either.
        assert_eq!(load_cmd_token(home.path()), None);
    }

    #[test]
    fn command_ids_unique_and_short() {
        assert_eq!(command_id(3, 0), "s3-0000");
        assert_eq!(command_id(3, 945_998), "s3-k9xq");
        // The suffix is the start second modulo 36^4.
        assert_eq!(command_id(1, 36u64.pow(4) + 35), "s1-000z");
        assert_eq!(
            command_id(12, u64::MAX),
            command_id(12, u64::MAX % 36u64.pow(4))
        );
        let started = 1_795_000_000;
        let ids: BTreeMap<String, u64> = (1..=1000)
            .map(|seq| (command_id(seq, started), seq))
            .collect();
        assert_eq!(ids.len(), 1000, "ids repeat");
        for id in ids.keys().chain([&command_id(u64::MAX, u64::MAX)]) {
            assert!(!id.is_empty() && id.len() <= 64, "{id}");
            assert!(
                id.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'),
                "{id}"
            );
        }
    }

    #[test]
    fn estop_without_connection_is_not_connected() {
        let session = DvizSession::new();
        let id = send(&session, NavCommand::Estop);
        let record = record(&session, &id).expect("a record of the e-stop");
        assert_eq!(record.cmd, "estop");
        assert_eq!(
            outcomes(&record),
            [("", &CommandOutcome::NotConnected("not connected"))]
        );
        // Each command gets its own id.
        let next = send(&session, NavCommand::Disarm);
        assert_ne!(next, id);
        assert_eq!(record_cmd(&session, &next), Some("disarm"));
    }

    fn record_cmd(session: &DvizSession, id: &str) -> Option<&'static str> {
        record(session, id).map(|record| record.cmd)
    }

    #[test]
    fn snapshot_has_no_token() {
        const TOKEN: &str = "sekret-snapshot-t0ken";
        let home = TempHome::new();
        let session = DvizSession::new();
        assert!(!session.snapshot().has_token);
        session.set_token_path(home.token(&format!("{TOKEN}\n")));
        let listen = SessionOptions {
            mode: "peer".into(),
            connect_endpoints: Vec::new(),
            listen_endpoints: vec![format!("tcp/127.0.0.1:{}", free_port())],
            prefix: "dviz".into(),
        };
        session.connect(listen.clone()).unwrap();
        // Read at connect, kept in memory only.
        let snapshot = session.snapshot();
        assert!(snapshot.has_token);
        let _ = send(&session, GOAL);
        let shown = format!("{:?}", session.snapshot());
        assert!(!shown.contains(TOKEN), "{shown}");

        // Read again at every connect: gone from the disk, gone here.
        std::fs::remove_file(home.path().join(".config/dviz/cmd_token")).unwrap();
        session.connect(listen).unwrap();
        assert!(!session.snapshot().has_token);
    }

    #[test]
    fn estop_before_any_status_is_no_robot_seen_yet() {
        let robots = fake_robots(&[], Queryables::Answer);
        let home = TempHome::new();
        let session = DvizSession::new();
        session.set_token_path(home.token("t0k"));
        session.connect(client_options(&robots.endpoint)).unwrap();
        assert!(
            wait_for(SETTLE, || session.snapshot().connected),
            "{:?}",
            session.snapshot()
        );
        let id = send(&session, NavCommand::Estop);
        assert_eq!(
            outcomes(&record(&session, &id).unwrap()),
            [("", &CommandOutcome::NotConnected("no robot seen yet"))]
        );
    }

    #[test]
    fn command_round_trip_over_zenoh() {
        const TOKEN: &str = "t0k-round-trip";
        let robots = fake_robots(&["tracer"], Queryables::Answer);
        let home = TempHome::new();
        let session = connected_session(&robots, &["tracer"], home.token(&format!(" {TOKEN}\r\n")));
        assert!(session.snapshot().has_token);

        let id = send(&session, GOAL);
        assert!(
            wait_for(Duration::from_secs(2), || record(&session, &id)
                .is_some_and(|record| acked_ok(&record))),
            "{:?}",
            session.snapshot().command
        );
        let record = record(&session, &id).unwrap();
        assert_eq!(record.cmd, "goal");
        let ack = CmdAck {
            id: id.clone(),
            cmd: "goal".into(),
            result: CmdResult::Ok,
            reason: "sent to nav_command".into(),
        };
        assert_eq!(outcomes(&record), [("tracer", &CommandOutcome::Ack(ack))]);

        let requests = robots.requests();
        assert_eq!(requests.len(), 1, "{requests:?}");
        let (robot, request) = &requests[0];
        assert_eq!(robot, "tracer");
        let expected = CmdRequest {
            cmd: "goal".into(),
            id: id.clone(),
            token: Some(TOKEN.into()),
            x: Some(1.0),
            y: Some(-2.0),
            yaw: Some(0.5),
        };
        assert_eq!(request, &expected);
        let shown = format!("{:?}", session.snapshot());
        assert!(!shown.contains(TOKEN), "{shown}");

        // A pose estimate always carries its heading.
        let pose = send(
            &session,
            NavCommand::InitialPose {
                x: 0.5,
                y: 0.25,
                yaw: -1.0,
            },
        );
        assert!(wait_for(Duration::from_secs(2), || robots.requests().len() == 2));
        let (_, request) = &robots.requests()[1];
        assert_eq!(
            (
                request.cmd.as_str(),
                request.id.as_str(),
                request.x,
                request.y,
                request.yaw
            ),
            (
                "initialpose",
                pose.as_str(),
                Some(0.5),
                Some(0.25),
                Some(-1.0)
            )
        );
        assert_eq!(request.token.as_deref(), Some(TOKEN));

        // Disconnected: the robots are forgotten at once, also while the
        // receiver is still finishing, and nothing is sent any more.
        session.disconnect();
        assert!(session.snapshot().robots.is_empty());
        let estop = send(&session, NavCommand::Estop);
        assert_eq!(
            outcomes(&self::record(&session, &estop).unwrap()),
            [("", &CommandOutcome::NotConnected("not connected"))]
        );
        thread::sleep(Duration::from_millis(500));
        assert!(
            session.snapshot().robots.is_empty(),
            "the stopped receiver added robots"
        );
        assert_eq!(robots.requests().len(), 2, "sent after the disconnect");
    }

    #[test]
    fn no_queryable_times_out() {
        let robots = fake_robots(&["tracer"], Queryables::Absent);
        let home = TempHome::new();
        let session = connected_session(&robots, &["tracer"], home.token("t0k"));
        let id = send(&session, GOAL);
        assert!(
            wait_for(Duration::from_millis(3500), || record(&session, &id)
                .is_some_and(
                    |record| outcomes(&record) == [("tracer", &CommandOutcome::Timeout)]
                )),
            "{:?}",
            session.snapshot().command
        );
    }

    #[test]
    fn silent_bridge_times_out_after_3_s() {
        let robots = fake_robots(&["tracer"], Queryables::Silent);
        let home = TempHome::new();
        let session = connected_session(&robots, &["tracer"], home.token("t0k"));
        let sent = Instant::now();
        let id = send(&session, NavCommand::Disarm);
        assert!(wait_for(SETTLE, || robots.requests().len() == 1));
        // Still waiting well into the 3 s.
        thread::sleep(Duration::from_millis(2500).saturating_sub(sent.elapsed()));
        assert_eq!(
            outcomes(&record(&session, &id).unwrap()),
            [("tracer", &CommandOutcome::Pending)]
        );
        assert!(
            wait_for(
                Duration::from_millis(3500).saturating_sub(sent.elapsed()),
                || {
                    record(&session, &id).is_some_and(|record| {
                        outcomes(&record) == [("tracer", &CommandOutcome::Timeout)]
                    })
                }
            ),
            "{:?}",
            session.snapshot().command
        );
    }

    #[test]
    fn estop_goes_to_every_robot() {
        let robots = fake_robots(&["tracer", "tracer_sim"], Queryables::Answer);
        let home = TempHome::new();
        let session = connected_session(&robots, &["tracer", "tracer_sim"], home.token("t0k"));

        let id = send(&session, NavCommand::Estop);
        assert!(
            wait_for(Duration::from_secs(2), || record(&session, &id)
                .is_some_and(|record| acked_ok(&record))),
            "{:?}",
            session.snapshot().command
        );
        let record = record(&session, &id).unwrap();
        let robots_replied: Vec<&str> = outcomes(&record).iter().map(|(robot, _)| *robot).collect();
        assert_eq!(robots_replied, ["tracer", "tracer_sim"]);
        let mut requests = robots.requests();
        requests.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(requests.len(), 2, "{requests:?}");
        for ((robot, request), expected) in requests.iter().zip(["tracer", "tracer_sim"]) {
            assert_eq!(robot, expected);
            assert_eq!(
                (request.cmd.as_str(), request.id.as_str()),
                ("estop", id.as_str())
            );
            // E-STOP needs no token, so it carries none.
            assert_eq!(request.token, None);
        }

        // A goal is for one robot only: not sent at all.
        let goal = send(&session, GOAL);
        assert_eq!(
            outcomes(&self::record(&session, &goal).unwrap()),
            [(
                "",
                &CommandOutcome::NotConnected("more than one robot on this connection")
            )]
        );
        let pose = send(
            &session,
            NavCommand::InitialPose {
                x: 0.0,
                y: 0.0,
                yaw: 0.0,
            },
        );
        assert_eq!(
            outcomes(&self::record(&session, &pose).unwrap()),
            [(
                "",
                &CommandOutcome::NotConnected("more than one robot on this connection")
            )]
        );
        // Disarm goes to both, like the E-STOP.
        let disarm = send(&session, NavCommand::Disarm);
        assert!(wait_for(Duration::from_secs(2), || self::record(
            &session, &disarm
        )
        .is_some_and(
            |record| record.replies.len() == 2 && acked_ok(&record)
        )));
        thread::sleep(Duration::from_millis(300));
        let cmds: Vec<String> = robots
            .requests()
            .iter()
            .map(|(_, r)| r.cmd.clone())
            .collect();
        assert_eq!(
            cmds.iter()
                .filter(|cmd| *cmd == "goal" || *cmd == "initialpose")
                .count(),
            0
        );
        assert_eq!(cmds.len(), 4, "{cmds:?}");
    }

    #[test]
    fn an_older_query_ending_leaves_the_newer_record_alone() {
        let robots = fake_robots(&["tracer"], Queryables::Silent);
        let home = TempHome::new();
        let session = connected_session(&robots, &["tracer"], home.token("t0k"));
        let first_sent = Instant::now();
        let first = send(&session, NavCommand::Disarm);
        thread::sleep(Duration::from_millis(1500));
        let second = send(&session, NavCommand::Estop);
        assert!(wait_for(SETTLE, || robots.requests().len() == 2));
        // The first query has timed out (3 s after it was sent); the record
        // is the second's, which still waits.
        thread::sleep(Duration::from_millis(3600).saturating_sub(first_sent.elapsed()));
        assert!(record(&session, &first).is_none());
        assert_eq!(
            outcomes(&record(&session, &second).expect("the newer record stays")),
            [("tracer", &CommandOutcome::Pending)]
        );
        assert!(
            wait_for(Duration::from_secs(2), || record(&session, &second)
                .is_some_and(
                    |record| outcomes(&record) == [("tracer", &CommandOutcome::Timeout)]
                )),
            "{:?}",
            session.snapshot().command
        );
    }

    #[test]
    fn a_panicking_launch_is_reported_not_stuck_starting() {
        let session = DvizSession::new();
        session
            .start_viewer(|| panic!("launch bug"))
            .expect("the launch starts");
        assert!(
            wait_for(Duration::from_secs(5), || matches!(
                session.snapshot().viewer,
                ViewerState::Failed(_)
            )),
            "{:?}",
            session.snapshot().viewer
        );
    }
}
