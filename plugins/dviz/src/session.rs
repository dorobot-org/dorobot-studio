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

use crate::nav_scene::NavScene;
use crate::route::{self, EntityInfo, SessionState, Sink, SinkAction, Visual};
use dviz_protocol::{GraphEdge, GraphNode, LogEntry, MvizMessage};
use std::{
    any::Any,
    collections::{BTreeMap, VecDeque},
    fmt::Display,
    mem,
    panic::{self, AssertUnwindSafe},
    sync::{
        atomic::{AtomicBool, AtomicU32, Ordering},
        Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use zenoh::{bytes::ZBytes, key_expr::KeyExpr, sample::Sample, Wait};

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
}

pub struct DvizSession {
    shared: Arc<Shared>,
    control: Mutex<Control>,
}

struct Shared {
    state: Mutex<SessionState>,
    sink: Mutex<Box<dyn Sink>>,
    link: Mutex<Link>,
    receivers_alive: AtomicU32,
    viewer: Mutex<ViewerState>,
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
        DvizSession {
            shared: Arc::new(Shared {
                state: Mutex::new(SessionState::default()),
                sink: Mutex::new(Box::new(DiscardSink)),
                link: Mutex::new(Link::default()),
                receivers_alive: AtomicU32::new(0),
                viewer: Mutex::new(ViewerState::Closed),
            }),
            control: Mutex::new(Control::default()),
        }
    }

    /// Starts receiving with `options`, replacing any current connection.
    /// Returns at once: Zenoh opens on the receiver thread, and its failures
    /// land in `last_error`. Only invalid options fail here. Options other
    /// than the last connect's (mode, endpoints or prefix) start from a clear
    /// state: no entities, logs, nodes or edges, and every counter at 0.
    pub fn connect(&self, options: SessionOptions) -> Result<(), String> {
        if let Err(error) = validate(&options) {
            lock(&self.shared.link).last_error = Some(error.clone());
            return Err(error);
        }
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
        }
    }

    fn stop_current(&self, control: &mut Control) {
        if let Some(stop) = control.stop.take() {
            stop.store(true, Ordering::Relaxed);
        }
        // After the flag: a receiver checks it under this lock before it
        // reports, so a stopped receiver can no longer overwrite this.
        let mut link = lock(&self.shared.link);
        link.connected = false;
        link.listening_since = None;
        link.last_error = None;
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
    subscriber.undeclare().wait()?;
    session.close().wait()
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
