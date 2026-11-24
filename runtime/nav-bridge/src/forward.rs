//! Forwarding to nav_command over UDP, the latest gate state slot, and the two command lanes.
//! estop / disarm run on their own lane so a slow goal can never delay them. No Zenoh here:
//! the reply goes through [`Replier`]. Nothing here logs or returns the token.

use crate::command::{decide, gate_view, nav_command_payload, CmdPolicy, Decision, GateView};
use dviz_protocol::{CmdAck, CmdKind, CmdRequest, CmdResult};
use std::io::Write;
use std::net::{SocketAddr, UdpSocket};
use std::panic::AssertUnwindSafe;
use std::sync::mpsc::{channel, sync_channel, SyncSender, TrySendError};
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

/// Send one datagram from a fresh `127.0.0.1:0` socket; the socket is kept to await the reply.
fn send_step(target: SocketAddr, payload: &[u8]) -> Option<UdpSocket> {
    let sock = UdpSocket::bind("127.0.0.1:0").ok()?;
    sock.send_to(payload, target).ok()?;
    Some(sock)
}

/// Wait until `deadline` for `{"ok":bool}` from `target`. A reply that is not that JSON counts
/// as `Rejected`. A reply already waiting in the socket is read even if the deadline has passed.
fn await_step(sock: &UdpSocket, target: SocketAddr, deadline: Instant) -> Forwarded {
    let mut buf = [0u8; 4096];
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        if sock.set_read_timeout(Some(left.max(Duration::from_millis(1)))).is_err() {
            return Forwarded::NoReply;
        }
        match sock.recv_from(&mut buf) {
            Ok((n, from)) if from == target => {
                let ok = serde_json::from_slice::<serde_json::Value>(&buf[..n])
                    .ok()
                    .and_then(|v| v.get("ok").and_then(|o| o.as_bool()));
                return if ok == Some(true) { Forwarded::Ok } else { Forwarded::Rejected };
            }
            Ok(_) if !left.is_zero() => {} // not from nav_command: keep waiting
            _ => return Forwarded::NoReply,
        }
    }
}

/// Send one datagram from a fresh `127.0.0.1:0` socket and wait up to `timeout` for `{"ok":bool}`.
/// A reply that is not that JSON counts as `Rejected`.
pub fn forward_udp(target: SocketAddr, payload: &[u8], timeout: Duration) -> Forwarded {
    let deadline = Instant::now() + timeout;
    match send_step(target, payload) {
        Some(sock) => await_step(&sock, target, deadline),
        None => Forwarded::NoReply,
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
    // a panicking Replier must not take the lane thread down
    let replier = job.replier;
    let _ = std::panic::catch_unwind(AssertUnwindSafe(move || replier.reply(&ack)));
}

fn forward_and_answer(
    log: &SharedLog,
    job: Job,
    req: &CmdRequest,
    payload: &[u8],
    target: SocketAddr,
    timeout: Duration,
) {
    let outcome = forward_udp(target, payload, timeout);
    answer_forwarded(log, job, req, outcome);
}

fn answer_forwarded(log: &SharedLog, job: Job, req: &CmdRequest, outcome: Forwarded) {
    let (result, reason) = match outcome {
        Forwarded::Ok => (CmdResult::Ok, ""),
        Forwarded::Rejected => (CmdResult::NavRejected, "nav_command rejected the command"),
        Forwarded::NoReply => (CmdResult::NoReply, "no reply from nav_command"),
    };
    answer(log, job, Some(req), result, reason);
}

/// A stop command already sent, waiting for nav_command's answer on the reply thread.
struct Pending {
    sock: Option<UdpSocket>,
    job: Job,
    req: CmdRequest,
    deadline: Instant,
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
        // The stop lane thread only sends; a second thread awaits answers, logs and replies, so a
        // slow nav_command or a blocked Replier can never delay the next estop / disarm.
        let (pending_tx, pending_rx) = sync_channel::<Pending>(LANE_CAPACITY);
        let reply_log = log.clone();
        let reply_thread = std::thread::spawn(move || {
            for p in pending_rx {
                let outcome = match &p.sock {
                    Some(sock) => await_step(sock, target, p.deadline),
                    None => Forwarded::NoReply,
                };
                answer_forwarded(&reply_log, p.job, &p.req, outcome);
            }
        });
        // Jobs the stop thread cannot hand on are dropped on a third thread: dropping a Replier
        // may do I/O or panic, and nothing on the send thread may run Replier code.
        let (discard_tx, discard_rx) = channel::<(Job, Option<String>)>();
        let discard_log = log.clone();
        let discard_thread = std::thread::spawn(move || {
            for (job, line) in discard_rx {
                if let Some(line) = line {
                    let mut w = discard_log.lock().unwrap_or_else(|e| e.into_inner());
                    let _ = w.write_all(line.as_bytes());
                    let _ = w.flush();
                }
                let _ = std::panic::catch_unwind(AssertUnwindSafe(move || drop(job)));
            }
        });
        let stop_thread = std::thread::spawn(move || {
            for job in stop_rx {
                let req = match &job.request {
                    Ok(r) => r.clone(),
                    Err(_) => {
                        let _ = discard_tx.send((job, None));
                        continue;
                    }
                };
                let deadline = Instant::now() + timeout;
                let sock = send_step(target, &nav_command_payload(&req));
                let pending = Pending { sock, job, req, deadline };
                if let Err(TrySendError::Full(p) | TrySendError::Disconnected(p)) = pending_tx.try_send(pending) {
                    // Sent, but nobody can take the answer: the ack is dropped (the asker times out).
                    let Pending { sock, job, req, .. } = p;
                    drop(sock);
                    let line = format!(
                        "nav-bridge: cmd {} {} sent, ack lost (reply thread busy or gone)\n",
                        clean(&req.cmd),
                        clean(&req.id)
                    );
                    let _ = discard_tx.send((job, Some(line)));
                }
            }
        });
        CommandLanes { main, stop, threads: vec![main_thread, stop_thread, reply_thread, discard_thread] }
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
        let (g, grx) = job(&goal_json("s1-aaaa", TOKEN));
        assert!(l.submit(g));
        std::thread::sleep(Duration::from_millis(10));
        let (e, erx) = job(r#"{"cmd":"estop","id":"s2-bbbb"}"#);
        let submitted = Instant::now();
        assert!(l.submit(e));
        // arrivals: the goal first, then the estop
        let mut estop_at = None;
        let mut order = Vec::new();
        while let Ok((at, v)) = f.rx.recv_timeout(LONG) {
            order.push(v["cmd"].as_str().unwrap_or("").to_string());
            if v["cmd"] == "estop" {
                estop_at = Some(at);
                break;
            }
        }
        let took = estop_at.expect("estop never arrived").duration_since(submitted);
        assert!(took < Duration::from_millis(50), "estop took {took:?}");
        assert_eq!(order, ["goal", "estop"]);
        assert_eq!(erx.recv_timeout(LONG).unwrap().result, CmdResult::Ok);
        assert_eq!(grx.recv_timeout(LONG).unwrap().result, CmdResult::NoReply);
        l.stop();
    }

    #[test]
    fn second_estop_not_queued_behind_first() {
        // nav_command withholds its answer to the first estop
        let mut n = 0;
        let f = fake(move |_| {
            n += 1;
            if n == 1 { None } else { Some(true) }
        });
        let gate = Arc::new(GateSlot::new());
        let l = lanes(f.addr, Duration::from_millis(1000), &gate, &SharedBuf::default());
        let (e1, rx1) = job(r#"{"cmd":"estop","id":"s1-aaaa"}"#);
        assert!(l.submit(e1));
        std::thread::sleep(Duration::from_millis(10));
        let (e2, rx2) = job(r#"{"cmd":"disarm","id":"s2-bbbb"}"#);
        let submitted = Instant::now();
        assert!(l.submit(e2));
        f.rx.recv_timeout(LONG).unwrap(); // first estop
        let (at, v) = f.rx.recv_timeout(LONG).unwrap();
        assert_eq!(v["cmd"], "disarm");
        let took = at.duration_since(submitted);
        assert!(took < Duration::from_millis(50), "second stop took {took:?}");
        assert_eq!(rx2.recv_timeout(LONG).unwrap().result, CmdResult::Ok);
        assert_eq!(rx1.recv_timeout(LONG).unwrap().result, CmdResult::NoReply);
        l.stop();
    }

    struct PanicReplier;
    impl Replier for PanicReplier {
        fn reply(self: Box<Self>, _ack: &CmdAck) {
            panic!("replier blew up");
        }
    }

    #[test]
    fn panicking_replier_does_not_stop_estops() {
        let f = fake(|_| Some(true));
        let gate = Arc::new(GateSlot::new());
        armed(&gate);
        let l = lanes(f.addr, LONG, &gate, &SharedBuf::default());
        let bad = |json: &str| Job {
            request: dviz_protocol::parse_cmd_request(json.as_bytes()),
            replier: Box::new(PanicReplier),
        };
        assert!(l.submit(bad(r#"{"cmd":"estop","id":"s1-aaaa"}"#)));
        f.rx.recv_timeout(LONG).unwrap();
        let (e2, rx2) = job(r#"{"cmd":"estop","id":"s2-bbbb"}"#);
        assert!(l.submit(e2));
        assert_eq!(rx2.recv_timeout(LONG).unwrap().result, CmdResult::Ok);
        // the main lane survives a panicking replier too
        assert!(l.submit(bad(&goal_json("s3-cccc", TOKEN))));
        f.rx.recv_timeout(LONG).unwrap();
        let (g, grx) = job(&goal_json("s4-dddd", TOKEN));
        assert!(l.submit(g));
        assert_eq!(grx.recv_timeout(LONG).unwrap().result, CmdResult::Ok);
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
        // main lane: a bad request is answered (so the replier blocks) without any UDP
        let main_job = || blocked("not json");
        assert!(l.submit(main_job()));
        entered_rx.recv_timeout(LONG).unwrap(); // the main thread now holds job 1
        for i in 0..16 {
            assert!(l.submit(main_job()), "queued job {i}");
        }
        let start = Instant::now();
        assert!(!l.submit(main_job()), "18th submit must be refused");
        assert!(start.elapsed() < Duration::from_millis(100));
        // the stop lane is unaffected, even with its own repliers blocked: all of these are sent
        let estop = r#"{"cmd":"estop","id":"s1-aaaa"}"#;
        for i in 0..40 {
            assert!(l.submit(blocked(estop)), "estop {i}");
            std::thread::sleep(Duration::from_millis(3));
        }
        for i in 0..40 {
            let (_, v) = f.rx.recv_timeout(LONG).unwrap_or_else(|_| panic!("estop {i} not sent"));
            assert_eq!(v["cmd"], "estop");
        }
        drop(release_tx);
        l.stop();
    }

    /// Dropping it blocks until the test releases (`block`) or panics (`!block`).
    struct DropReplier {
        release: Arc<Mutex<mpsc::Receiver<()>>>,
        block: bool,
    }
    impl Replier for DropReplier {
        fn reply(self: Box<Self>, _ack: &CmdAck) {}
    }
    impl Drop for DropReplier {
        fn drop(&mut self) {
            if self.block {
                let _ = self.release.lock().unwrap_or_else(|e| e.into_inner()).recv();
            } else if !std::thread::panicking() {
                panic!("replier drop blew up");
            }
        }
    }

    #[test]
    fn overflow_never_runs_replier_on_send_thread() {
        let f = fake(|_| Some(true));
        let gate = Arc::new(GateSlot::new());
        let l = lanes(f.addr, LONG, &gate, &SharedBuf::default());
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let release = Arc::new(Mutex::new(release_rx));
        let (entered_tx, entered_rx) = mpsc::channel();
        let estop = r#"{"cmd":"estop","id":"s1-aaaa"}"#;
        let mk = |replier: Box<dyn Replier>| Job {
            request: dviz_protocol::parse_cmd_request(estop.as_bytes()),
            replier,
        };
        // hold the reply thread
        let held = BlockedReplier { entered: entered_tx, release: release.clone() };
        let mut submits = vec![Instant::now()];
        assert!(l.submit(mk(Box::new(held))));
        entered_rx.recv_timeout(LONG).unwrap();
        // overflow the 16-slot reply queue; dropping each of these blocks, then panics
        let mut sent = 1;
        for i in 0..60 {
            let replier = DropReplier { release: release.clone(), block: i < 30 };
            submits.push(Instant::now());
            assert!(l.submit(mk(Box::new(replier))), "estop {i}");
            sent += 1;
            std::thread::sleep(Duration::from_millis(3));
        }
        assert_eq!(submits.len(), sent);
        for (i, submitted) in submits.iter().enumerate() {
            let (at, v) = f.rx.recv_timeout(LONG).unwrap_or_else(|_| panic!("estop {i} not sent"));
            assert_eq!(v["cmd"], "estop");
            let took = at.saturating_duration_since(*submitted);
            assert!(took < Duration::from_millis(50), "estop {i} took {took:?}");
        }
        drop(release_tx);
        l.stop();
    }
}
