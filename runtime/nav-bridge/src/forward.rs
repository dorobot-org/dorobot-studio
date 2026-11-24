//! Forwarding to nav_command over UDP, the latest gate state slot, and the two command lanes.
//! estop / disarm run on their own lane so a slow goal can never delay them. No Zenoh here:
//! the reply goes through [`Replier`]. Nothing here logs or returns the token.

use crate::command::{decide, gate_view, nav_command_payload, CmdPolicy, Decision, GateView};
use dviz_protocol::{CmdAck, CmdKind, CmdRequest, CmdResult};
use std::io::Write;
use std::net::{SocketAddr, UdpSocket};
use std::sync::mpsc::{sync_channel, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// Jobs waiting per lane (one more may be in progress).
const LANE_CAPACITY: usize = 16;

/// The latest `cmd_gate/state` message and when it arrived.
pub struct GateSlot {
    latest: Mutex<Option<(Vec<u8>, Instant)>>,
}

impl GateSlot {
    pub fn new() -> Self {
        GateSlot { latest: Mutex::new(None) }
    }

    pub fn store(&self, bytes: &[u8], now: Instant) {
        let mut g = self.latest.lock().unwrap_or_else(|p| p.into_inner());
        *g = Some((bytes.to_vec(), now));
    }

    pub fn view(&self, now: Instant) -> GateView {
        let g = self.latest.lock().unwrap_or_else(|p| p.into_inner());
        gate_view(g.as_ref().map(|(b, at)| (b.as_slice(), *at)), now)
    }
}

impl Default for GateSlot {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, PartialEq)]
pub enum Forwarded {
    Ok,
    Rejected,
    NoReply,
}

/// Send one datagram from a fresh `127.0.0.1:0` socket and wait up to `timeout` for `{"ok":bool}`.
/// A reply that is not that JSON counts as `Rejected`.
pub fn forward_udp(target: SocketAddr, payload: &[u8], timeout: Duration) -> Forwarded {
    let Ok(sock) = UdpSocket::bind("127.0.0.1:0") else { return Forwarded::NoReply };
    if sock.send_to(payload, target).is_err() {
        return Forwarded::NoReply;
    }
    let deadline = Instant::now() + timeout;
    let mut buf = [0u8; 4096];
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() || sock.set_read_timeout(Some(left)).is_err() {
            return Forwarded::NoReply;
        }
        match sock.recv_from(&mut buf) {
            Ok((n, from)) if from == target => {
                let ok = serde_json::from_slice::<serde_json::Value>(&buf[..n])
                    .ok()
                    .and_then(|v| v.get("ok").and_then(|o| o.as_bool()));
                return if ok == Some(true) { Forwarded::Ok } else { Forwarded::Rejected };
            }
            Ok(_) => {} // not from nav_command: keep waiting
            Err(_) => return Forwarded::NoReply,
        }
    }
}

pub trait Replier: Send {
    fn reply(self: Box<Self>, ack: &CmdAck);
}

pub struct Job {
    pub request: Result<CmdRequest, String>,
    pub replier: Box<dyn Replier>,
}

pub struct CommandLanes {
    main: SyncSender<Job>,
    stop: SyncSender<Job>,
    threads: Vec<JoinHandle<()>>,
}

type SharedLog = Arc<Mutex<Box<dyn Write + Send>>>;

fn code(r: CmdResult) -> String {
    serde_json::to_value(r).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default()
}

/// Printable, short, one line: the command name comes from the network.
fn clean(s: &str) -> String {
    s.chars().take(32).map(|c| if c.is_control() { '?' } else { c }).collect()
}

fn log_line(log: &SharedLog, req: Option<&CmdRequest>, result: CmdResult) {
    let mut line = String::from("nav-bridge: cmd");
    match req {
        Some(r) => {
            line.push_str(&format!(" {} {}", clean(&r.cmd), clean(&r.id)));
            for (k, v) in [("x", r.x), ("y", r.y), ("yaw", r.yaw)] {
                if let Some(v) = v {
                    line.push_str(&format!(" {k}={v:.2}"));
                }
            }
        }
        None => line.push_str(" - -"),
    }
    line.push_str(&format!(" -> {}\n", code(result)));
    let mut w = log.lock().unwrap_or_else(|p| p.into_inner());
    let _ = w.write_all(line.as_bytes());
    let _ = w.flush();
}

fn answer(log: &SharedLog, job: Job, req: Option<&CmdRequest>, result: CmdResult, reason: &str) {
    log_line(log, req, result);
    let ack = CmdAck {
        id: req.map(|r| r.id.clone()).unwrap_or_default(),
        cmd: req.map(|r| r.cmd.clone()).unwrap_or_default(),
        result,
        reason: reason.to_string(),
    };
    job.replier.reply(&ack);
}

fn forward_and_answer(
    log: &SharedLog,
    job: Job,
    req: &CmdRequest,
    payload: &[u8],
    target: SocketAddr,
    timeout: Duration,
) {
    let (result, reason) = match forward_udp(target, payload, timeout) {
        Forwarded::Ok => (CmdResult::Ok, ""),
        Forwarded::Rejected => (CmdResult::NavRejected, "nav_command rejected the command"),
        Forwarded::NoReply => (CmdResult::NoReply, "no reply from nav_command"),
    };
    answer(log, job, Some(req), result, reason);
}

impl CommandLanes {
    /// estop / disarm go to the stop lane, everything else (bad requests too) to the main lane.
    /// Each job: decide, forward, log, reply. The stop lane never consults the policy or the
    /// gate; the main lane reads the gate right before deciding.
    pub fn spawn(
        mut policy: CmdPolicy,
        gate: Arc<GateSlot>,
        target: SocketAddr,
        timeout: Duration,
        log: Box<dyn Write + Send>,
    ) -> Self {
        let log: SharedLog = Arc::new(Mutex::new(log));
        let (main, main_rx) = sync_channel::<Job>(LANE_CAPACITY);
        let (stop, stop_rx) = sync_channel::<Job>(LANE_CAPACITY);
        let main_log = log.clone();
        let main_thread = std::thread::spawn(move || {
            for job in main_rx {
                let req = match &job.request {
                    Ok(r) => r.clone(),
                    Err(_) => {
                        answer(&main_log, job, None, CmdResult::BadRequest, "malformed command request");
                        continue;
                    }
                };
                let now = Instant::now();
                match decide(&req, &mut policy, gate.view(now), now) {
                    Decision::Forward(payload) => forward_and_answer(&main_log, job, &req, &payload, target, timeout),
                    Decision::Reply(result, reason) => answer(&main_log, job, Some(&req), result, &reason),
                }
            }
        });
        let stop_thread = std::thread::spawn(move || {
            for job in stop_rx {
                let req = match &job.request {
                    Ok(r) => r.clone(),
                    Err(_) => {
                        answer(&log, job, None, CmdResult::BadRequest, "malformed command request");
                        continue;
                    }
                };
                let payload = nav_command_payload(&req);
                forward_and_answer(&log, job, &req, &payload, target, timeout);
            }
        });
        CommandLanes { main, stop, threads: vec![main_thread, stop_thread] }
    }

    /// False when the lane is full (the asker will time out) or its thread is gone.
    pub fn submit(&self, job: Job) -> bool {
        let lane = match &job.request {
            Ok(r) if matches!(r.kind(), CmdKind::Estop | CmdKind::Disarm) => &self.stop,
            _ => &self.main,
        };
        match lane.try_send(job) {
            Ok(()) => true,
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => false,
        }
    }

    /// Close both lanes and wait for the threads (queued jobs are still answered).
    pub fn stop(self) {
        let CommandLanes { main, stop, threads } = self;
        drop(main);
        drop(stop);
        for t in threads {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::{load_token, TokenBucket};
    use std::os::unix::fs::PermissionsExt;
    use std::sync::mpsc;

    const TOKEN: &str = "s3cr3t-token";
    const LONG: Duration = Duration::from_secs(5);

    /// Fake nav_command: records (arrival, payload) and answers per `behave`
    /// (`Some(ok)` replies `{"ok":ok}`, `None` stays silent).
    struct Fake {
        addr: SocketAddr,
        rx: mpsc::Receiver<(Instant, serde_json::Value)>,
    }

    fn fake(mut behave: impl FnMut(&serde_json::Value) -> Option<bool> + Send + 'static) -> Fake {
        let sock = UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = sock.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = [0u8; 2048];
            while let Ok((n, from)) = sock.recv_from(&mut buf) {
                let at = Instant::now();
                let v: serde_json::Value = serde_json::from_slice(&buf[..n]).unwrap();
                let answer = behave(&v);
                if tx.send((at, v)).is_err() {
                    return;
                }
                if let Some(ok) = answer {
                    let _ = sock.send_to(format!("{{\"ok\":{ok},\"outputs\":[]}}").as_bytes(), from);
                }
            }
        });
        Fake { addr, rx }
    }

    #[test]
    fn forward_ok_rejected_and_silence() {
        let mut n = 0;
        let f = fake(move |_| {
            n += 1;
            match n {
                1 => Some(true),
                2 => Some(false),
                _ => None,
            }
        });
        let t = Duration::from_millis(200);
        assert_eq!(forward_udp(f.addr, b"{\"cmd\":\"a\"}", t), Forwarded::Ok);
        assert_eq!(forward_udp(f.addr, b"{\"cmd\":\"b\"}", t), Forwarded::Rejected);
        let start = Instant::now();
        assert_eq!(forward_udp(f.addr, b"{\"cmd\":\"c\"}", t), Forwarded::NoReply);
        let took = start.elapsed();
        assert!(took >= Duration::from_millis(150) && took < Duration::from_millis(400), "{took:?}");
    }

    struct ChanReplier(mpsc::Sender<CmdAck>);
    impl Replier for ChanReplier {
        fn reply(self: Box<Self>, ack: &CmdAck) {
            let _ = self.0.send(ack.clone());
        }
    }

    fn job(json: &str) -> (Job, mpsc::Receiver<CmdAck>) {
        let (tx, rx) = mpsc::channel();
        let request = dviz_protocol::parse_cmd_request(json.as_bytes());
        (Job { request, replier: Box::new(ChanReplier(tx)) }, rx)
    }

    fn goal_json(id: &str, token: &str) -> String {
        format!(r#"{{"cmd":"goal","id":"{id}","token":"{token}","x":1.5,"y":-2.25,"yaw":0.5}}"#)
    }

    fn policy() -> CmdPolicy {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("navbridge-fwd-{n}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("cmd_token");
        std::fs::write(&p, TOKEN).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).unwrap();
        CmdPolicy { goal_enabled: true, token: load_token(&p).ok(), bucket: TokenBucket::new(2.0, 2) }
    }

    #[derive(Clone, Default)]
    struct SharedBuf(Arc<Mutex<Vec<u8>>>);
    impl Write for SharedBuf {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn lanes(target: SocketAddr, timeout: Duration, gate: &Arc<GateSlot>, log: &SharedBuf) -> CommandLanes {
        CommandLanes::spawn(policy(), gate.clone(), target, timeout, Box::new(log.clone()))
    }

    fn armed(gate: &GateSlot) {
        gate.store(br#"{"armed":true,"estop":false}"#, Instant::now());
    }

    #[test]
    fn goal_rejected_right_after_disarm() {
        let f = fake(|_| Some(true));
        let gate = Arc::new(GateSlot::new());
        let now = Instant::now();
        gate.store(br#"{"armed":true,"estop":false}"#, now);
        gate.store(br#"{"armed":false,"estop":false}"#, now);
        let l = lanes(f.addr, LONG, &gate, &SharedBuf::default());
        let (j, rx) = job(&goal_json("s1-aaaa", TOKEN));
        assert!(l.submit(j));
        let ack = rx.recv_timeout(LONG).unwrap();
        assert_eq!(ack.result, CmdResult::NotArmed);
        assert_eq!((ack.id.as_str(), ack.cmd.as_str()), ("s1-aaaa", "goal"));
        assert!(f.rx.recv_timeout(Duration::from_millis(150)).is_err(), "goal reached nav_command");
        l.stop();
    }

    #[test]
    fn goal_forwarded_when_armed() {
        let f = fake(|_| Some(true));
        let gate = Arc::new(GateSlot::new());
        armed(&gate);
        let l = lanes(f.addr, LONG, &gate, &SharedBuf::default());
        let (j, rx) = job(&goal_json("s1-aaaa", TOKEN));
        assert!(l.submit(j));
        assert_eq!(rx.recv_timeout(LONG).unwrap().result, CmdResult::Ok);
        let (_, v) = f.rx.recv_timeout(LONG).unwrap();
        assert_eq!(v["cmd"], "goal");
        assert!(v.get("token").is_none());
        l.stop();
    }

    #[test]
    fn nav_rejection_and_silence_map_to_results() {
        let f = fake(|_| Some(false));
        let gate = Arc::new(GateSlot::new());
        armed(&gate);
        let l = lanes(f.addr, LONG, &gate, &SharedBuf::default());
        let (j, rx) = job(&goal_json("s1-aaaa", TOKEN));
        assert!(l.submit(j));
        assert_eq!(rx.recv_timeout(LONG).unwrap().result, CmdResult::NavRejected);
        l.stop();
        let f = fake(|_| None);
        let l = lanes(f.addr, Duration::from_millis(100), &gate, &SharedBuf::default());
        let (j, rx) = job(&goal_json("s2-aaaa", TOKEN));
        assert!(l.submit(j));
        assert_eq!(rx.recv_timeout(LONG).unwrap().result, CmdResult::NoReply);
        l.stop();
    }

    #[test]
    fn estop_not_queued_behind_goal() {
        // goals are never answered; estop is answered at once
        let f = fake(|v| if v["cmd"] == "estop" { Some(true) } else { None });
        let gate = Arc::new(GateSlot::new());
        armed(&gate);
        let l = lanes(f.addr, Duration::from_millis(1000), &gate, &SharedBuf::default());
        let (g, _grx) = job(&goal_json("s1-aaaa", TOKEN));
        assert!(l.submit(g));
        std::thread::sleep(Duration::from_millis(10));
        let (e, erx) = job(r#"{"cmd":"estop","id":"s2-bbbb"}"#);
        let submitted = Instant::now();
        assert!(l.submit(e));
        // arrivals: the goal first, then the estop
        let mut estop_at = None;
        while let Ok((at, v)) = f.rx.recv_timeout(LONG) {
            if v["cmd"] == "estop" {
                estop_at = Some(at);
                break;
            }
        }
        let took = estop_at.expect("estop never arrived").duration_since(submitted);
        assert!(took < Duration::from_millis(50), "estop took {took:?}");
        assert_eq!(erx.recv_timeout(LONG).unwrap().result, CmdResult::Ok);
        l.stop();
    }

    #[test]
    fn bad_request_gets_reply() {
        let f = fake(|_| Some(true));
        let gate = Arc::new(GateSlot::new());
        let l = lanes(f.addr, LONG, &gate, &SharedBuf::default());
        let (tx, rx) = mpsc::channel();
        assert!(l.submit(Job { request: Err("bad json".into()), replier: Box::new(ChanReplier(tx)) }));
        let ack = rx.recv_timeout(LONG).unwrap();
        assert_eq!(ack.result, CmdResult::BadRequest);
        assert_eq!((ack.id.as_str(), ack.cmd.as_str()), ("", ""));
        assert!(f.rx.recv_timeout(Duration::from_millis(100)).is_err());
        l.stop();
    }

    #[test]
    fn log_has_no_token() {
        let f = fake(|_| Some(true));
        let gate = Arc::new(GateSlot::new());
        armed(&gate);
        let log = SharedBuf::default();
        let l = lanes(f.addr, LONG, &gate, &log);
        let mut rxs = Vec::new();
        for json in [
            goal_json("s1-aaaa", TOKEN),
            format!(r#"{{"cmd":"estop","id":"s2-bbbb","token":"{TOKEN}"}}"#),
            goal_json("s3-cccc", "s3cr3t-wrong"),
        ] {
            let (j, rx) = job(&json);
            assert!(l.submit(j));
            rxs.push(rx);
        }
        let results: Vec<_> = rxs.iter().map(|r| r.recv_timeout(LONG).unwrap().result).collect();
        assert_eq!(results, [CmdResult::Ok, CmdResult::Ok, CmdResult::BadToken]);
        l.stop();
        let text = String::from_utf8(log.0.lock().unwrap().clone()).unwrap();
        assert!(!text.contains("s3cr3t"), "{text}");
        assert_eq!(text.lines().count(), 3, "{text}");
        assert!(text.contains("nav-bridge: cmd goal s1-aaaa x=1.50 y=-2.25 yaw=0.50 -> ok"), "{text}");
        assert!(text.contains("nav-bridge: cmd estop s2-bbbb -> ok"), "{text}");
        assert!(text.contains("nav-bridge: cmd goal s3-cccc x=1.50 y=-2.25 yaw=0.50 -> bad_token"), "{text}");
    }

    /// Blocks in `reply` until the test drops the release sender.
    struct BlockedReplier {
        entered: mpsc::Sender<()>,
        release: Arc<Mutex<mpsc::Receiver<()>>>,
    }
    impl Replier for BlockedReplier {
        fn reply(self: Box<Self>, _ack: &CmdAck) {
            let _ = self.entered.send(());
            let _ = self.release.lock().unwrap().recv();
        }
    }

    #[test]
    fn full_lane_refuses() {
        let f = fake(|_| Some(true));
        let gate = Arc::new(GateSlot::new());
        let l = lanes(f.addr, LONG, &gate, &SharedBuf::default());
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let release = Arc::new(Mutex::new(release_rx));
        let (entered_tx, entered_rx) = mpsc::channel();
        let blocked = |json: &str| Job {
            request: dviz_protocol::parse_cmd_request(json.as_bytes()),
            replier: Box::new(BlockedReplier { entered: entered_tx.clone(), release: release.clone() }),
        };
        let estop = r#"{"cmd":"estop","id":"s1-aaaa"}"#;
        assert!(l.submit(blocked(estop)));
        entered_rx.recv_timeout(LONG).unwrap(); // the lane thread now holds job 1
        for i in 0..16 {
            assert!(l.submit(blocked(estop)), "queued job {i}");
        }
        let start = Instant::now();
        assert!(!l.submit(blocked(estop)), "18th submit must be refused");
        assert!(start.elapsed() < Duration::from_millis(100));
        // the other lane is unaffected
        let (j, rx) = job(r#"{"cmd":"arm","id":"s9-zzzz"}"#);
        assert!(l.submit(j));
        assert_eq!(rx.recv_timeout(LONG).unwrap().result, CmdResult::NotAllowed);
        drop(release_tx);
        l.stop();
    }
}
