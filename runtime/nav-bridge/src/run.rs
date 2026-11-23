//! The Dora side of the bridge, testable without Dora: `main` turns each Dora event
//! into a [`NodeEvent`] for [`Bridge::on_event`], which only queues inputs, and owns
//! the process exit (see `main.rs`).

use crate::config::BridgeConfig;
use crate::logs::LogSources;
use crate::publish::{zenoh_publisher, Publish};
use crate::worker::{spawn_worker_with_logs, Inbox};
use dora_node_api::DoraArray;
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Inputs queued for the worker; when full the oldest is dropped.
pub const INBOX_CAP: usize = 64;

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

/// The running bridge: inbox, worker thread and its stop flag.
pub struct Bridge {
    inbox: Arc<Inbox>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    worker_dead: bool,
    inputs: u64,
    not_bytes: u64,
}

impl Bridge {
    /// Opens the Zenoh session (peer, listening on `cfg.listen`) and starts the worker.
    pub fn start(cfg: BridgeConfig, sources: LogSources) -> Result<Bridge, String> {
        let publisher = zenoh_publisher(&cfg)?;
        Ok(Bridge::with_publisher(cfg, sources, publisher))
    }

    pub fn with_publisher(cfg: BridgeConfig, sources: LogSources, publisher: Box<dyn Publish>) -> Bridge {
        let inbox = Inbox::new(INBOX_CAP);
        let stop = Arc::new(AtomicBool::new(false));
        let worker = spawn_worker_with_logs(cfg, inbox.clone(), publisher, stop.clone(), sources);
        Bridge { inbox, stop, worker: Some(worker), worker_dead: false, inputs: 0, not_bytes: 0 }
    }

    /// One Dora event; never blocks. An input goes into the inbox. If the worker
    /// thread has died, one ERROR line goes to `err` (stderr, which Dora records) and
    /// inputs are dropped from then on; events are still consumed, so the dataflow
    /// is not disturbed.
    pub fn on_event(&mut self, event: NodeEvent, err: &mut dyn Write) -> Flow {
        match event {
            NodeEvent::Input { id, data } => {
                self.inputs += 1;
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
    /// publisher, which closes the Zenoh session.
    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }

    /// Stops the worker and waits until `deadline` for its thread to end, Zenoh session
    /// close included; true if it ended. A dead (panicked) worker counts as ended.
    pub fn wait(&mut self, deadline: Instant) -> bool {
        self.request_stop();
        let Some(worker) = self.worker.take() else { return true };
        while !worker.is_finished() {
            let now = Instant::now();
            if now >= deadline {
                self.worker = Some(worker);
                return false;
            }
            thread::sleep((deadline - now).min(Duration::from_millis(5)));
        }
        // A panic was already printed by the panic hook; nothing more to report.
        let _ = worker.join();
        true
    }

    pub fn summary(&self) -> String {
        format!(
            "nav-bridge: {} inputs, {} dropped (inbox full), {} not bytes",
            self.inputs,
            self.inbox.dropped(),
            self.not_bytes
        )
    }
}

impl Drop for Bridge {
    /// Stops the worker without waiting for it.
    fn drop(&mut self) {
        self.request_stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dora_node_api::IntoArrow;
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
}
