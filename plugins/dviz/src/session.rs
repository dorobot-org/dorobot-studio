//! The process-wide dviz Zenoh session.
//!
//! One background thread per connection opens Zenoh (never the caller), then
//! drains a bounded queue that the subscriber callback fills. The callback
//! only queues `(topic, payload)`, so it never blocks Zenoh; when the queue is
//! full the oldest payload is evicted and counted as dropped.

use crate::route::{self, EntityInfo, SessionState, Sink};
use dviz_protocol::{GraphEdge, GraphNode, LogEntry, MvizMessage};
use std::{
    collections::{BTreeMap, VecDeque},
    fmt::Display,
    mem,
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
    pub received: u64,
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
            }),
            control: Mutex::new(Control::default()),
        }
    }

    /// Starts receiving with `options`, replacing any current connection.
    /// Returns at once: Zenoh opens on the receiver thread, and its failures
    /// land in `last_error`. Only invalid options fail here.
    pub fn connect(&self, options: SessionOptions) -> Result<(), String> {
        if let Err(error) = validate(&options) {
            lock(&self.shared.link).last_error = Some(error.clone());
            return Err(error);
        }
        let mut control = lock(&self.control);
        self.stop_current(&mut control);
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

    /// Where visual messages go from now on. Does not wait for the previous
    /// sink to go away: dropping a Rerun stream flushes it, which can take
    /// seconds while its viewer is still starting, so it is dropped on a
    /// short-lived thread, after the sink lock is released.
    pub fn set_sink(&self, sink: Box<dyn Sink>) {
        let previous = {
            let mut current = lock(&self.shared.sink);
            mem::replace(&mut *current, sink)
        };
        // If no thread can be started, the sink is dropped here after all.
        let _ = thread::Builder::new()
            .name("dviz-sink-drop".into())
            .spawn(move || drop(previous));
    }

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
            received: state.received,
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
            thread::sleep(POLL);
        }
    }
}

struct AliveGuard<'a>(&'a AtomicU32);

impl Drop for AliveGuard<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
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
        let visual = {
            let mut state = lock(&shared.state);
            state.dropped += dropped;
            state.malformed += oversized;
            match &item {
                Some((topic, bytes)) => route::accept(&mut state, &options.prefix, topic, bytes),
                None => None,
            }
        };
        // The sink may be slow; the state lock is not held across it.
        if let Some(visual) = visual {
            let delivered = lock(&shared.sink).log(&visual.entity, &visual.msg, visual.binary);
            if delivered.is_err() {
                lock(&shared.state).malformed += 1;
            }
        }
    }
    subscriber.undeclare().wait()?;
    session.close().wait()
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
#[derive(Default)]
struct Inbox {
    pending: Mutex<Pending>,
    ready: Condvar,
}

#[derive(Default)]
struct Pending {
    queue: VecDeque<(String, ZBytes)>,
    dropped: u64,
    oversized: u64,
}

impl Inbox {
    /// Runs in the Zenoh callback: queue only, evicting the oldest when full.
    fn push(&self, sample: Sample) {
        if sample.payload().len() > MAX_PAYLOAD {
            lock(&self.pending).oversized += 1;
            return;
        }
        let topic = sample.key_expr().as_str().to_string();
        let payload = sample.payload().clone();
        let mut pending = lock(&self.pending);
        if pending.queue.len() >= RING_CAP {
            pending.queue.pop_front();
            pending.dropped += 1;
        }
        pending.queue.push_back((topic, payload));
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
        (
            pending.queue.pop_front(),
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
}
