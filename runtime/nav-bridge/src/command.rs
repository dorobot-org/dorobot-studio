//! Command decision (pure apart from reading the token file): token, rate limit, gate rule.
//! Forwarding and Zenoh live elsewhere. Nothing here ever logs or returns the token.

use dviz_protocol::{CmdKind, CmdRequest, CmdResult};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// A gate state message older than this counts as unknown.
pub const GATE_FRESH: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateView {
    Unknown,
    Armed,
    Disarmed,
    Estop,
}

/// Read the latest `cmd_gate/state` JSON `{"armed","estop",...}` and when it arrived.
pub fn gate_view(latest: Option<(&[u8], Instant)>, now: Instant) -> GateView {
    let Some((bytes, at)) = latest else { return GateView::Unknown };
    if now.checked_duration_since(at).is_some_and(|age| age > GATE_FRESH) {
        return GateView::Unknown;
    }
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(bytes) else { return GateView::Unknown };
    let Some(armed) = v.get("armed").and_then(|a| a.as_bool()) else { return GateView::Unknown };
    let estop = v.get("estop").and_then(|e| e.as_bool()).unwrap_or(false);
    if estop {
        GateView::Estop
    } else if armed {
        GateView::Armed
    } else {
        GateView::Disarmed
    }
}

/// Token bucket: `burst` tokens, refilled at `per_s` tokens per second.
pub struct TokenBucket {
    per_s: f64,
    burst: f64,
    tokens: f64,
    last: Option<Instant>,
}

impl TokenBucket {
    pub fn new(per_s: f32, burst: u32) -> Self {
        let per_s = if per_s.is_finite() && per_s > 0.0 { f64::from(per_s) } else { 0.0 };
        TokenBucket { per_s, burst: f64::from(burst), tokens: f64::from(burst), last: None }
    }

    pub fn take(&mut self, now: Instant) -> bool {
        if let Some(last) = self.last {
            let dt = now.checked_duration_since(last).map_or(0.0, |d| d.as_secs_f64());
            self.tokens = (self.tokens + dt * self.per_s).min(self.burst);
        }
        if self.last.is_none_or(|l| now > l) {
            self.last = Some(now);
        }
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// The robot's command token. No `Debug` / `Display`, so it cannot reach a log by accident.
pub struct Token(Vec<u8>);

impl Token {
    /// Both sides trimmed, compared without an early exit.
    pub fn matches(&self, given: &str) -> bool {
        let a = &self.0;
        let b = given.trim().as_bytes();
        let mut diff = a.len() ^ b.len();
        for i in 0..a.len().max(b.len()) {
            diff |= usize::from(a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0));
        }
        diff == 0
    }
}

/// Read the token file: must exist, be non-blank, and not be readable by group or others.
/// Errors never contain the file's content.
pub fn load_token(path: &Path) -> Result<Token, String> {
    use std::os::unix::fs::PermissionsExt;
    let shown = path.display();
    let meta = std::fs::metadata(path).map_err(|e| format!("{shown}: {e}"))?;
    let mode = meta.permissions().mode();
    if mode & 0o077 != 0 {
        return Err(format!("{shown}: permissions {:03o} too open (need 600)", mode & 0o777));
    }
    let text = std::fs::read_to_string(path).map_err(|e| format!("{shown}: {}", e.kind()))?;
    let t = text.trim();
    if t.is_empty() {
        return Err(format!("{shown}: empty"));
    }
    Ok(Token(t.as_bytes().to_vec()))
}

/// Expand a leading `~/` only.
pub fn expand_home(path: &str, home: Option<&Path>) -> PathBuf {
    match (path.strip_prefix("~/"), home) {
        (Some(rest), Some(h)) => h.join(rest),
        _ => PathBuf::from(path),
    }
}

pub struct CmdPolicy {
    pub goal_enabled: bool,
    pub token: Option<Token>,
    pub bucket: TokenBucket,
}

impl CmdPolicy {
    /// Goals and initial poses can only be accepted with the switch on and a usable token.
    pub fn goal_channel_open(&self) -> bool {
        self.goal_enabled && self.token.is_some()
    }
}

#[derive(Debug, PartialEq)]
pub enum Decision {
    /// Send this payload to nav_command.
    Forward(Vec<u8>),
    /// Answer the query without forwarding.
    Reply(CmdResult, String),
}

fn reply(r: CmdResult, why: &str) -> Decision {
    Decision::Reply(r, why.to_string())
}

pub fn decide(req: &CmdRequest, policy: &mut CmdPolicy, gate: GateView, now: Instant) -> Decision {
    let kind = req.kind();
    match kind {
        CmdKind::Estop | CmdKind::Disarm => return Decision::Forward(nav_command_payload(req)),
        CmdKind::Other => return reply(CmdResult::NotAllowed, "command not allowed from here"),
        CmdKind::Goal | CmdKind::InitialPose => {}
    }
    let Some(token) = policy.token.as_ref().filter(|_| policy.goal_enabled) else {
        return reply(CmdResult::Disabled, "goal / initialpose not enabled on the robot");
    };
    if !req.token.as_deref().is_some_and(|t| token.matches(t)) {
        return reply(CmdResult::BadToken, "token missing or wrong");
    }
    if !policy.bucket.take(now) {
        return reply(CmdResult::RateLimited, "too many goal commands");
    }
    if kind == CmdKind::Goal {
        match gate {
            GateView::Unknown => return reply(CmdResult::GateUnknown, "gate state unknown"),
            GateView::Estop => return reply(CmdResult::EstopActive, "e-stop latched"),
            GateView::Disarmed => return reply(CmdResult::NotArmed, "robot not armed"),
            GateView::Armed => {}
        }
    }
    Decision::Forward(nav_command_payload(req))
}

/// `{"cmd","x","y","yaw"?,"id"?}` for nav_command: goal carries `id`; the token is never copied.
pub fn nav_command_payload(req: &CmdRequest) -> Vec<u8> {
    let mut m = serde_json::Map::new();
    m.insert("cmd".into(), req.cmd.clone().into());
    for (k, v) in [("x", req.x), ("y", req.y), ("yaw", req.yaw)] {
        if let Some(n) = v.and_then(serde_json::Number::from_f64) {
            m.insert(k.into(), n.into());
        }
    }
    if req.kind() == CmdKind::Goal {
        m.insert("id".into(), req.id.clone().into());
    }
    serde_json::to_vec(&m).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn req(cmd: &str, token: Option<&str>) -> CmdRequest {
        CmdRequest {
            cmd: cmd.into(),
            id: "s1-abcd".into(),
            token: token.map(String::from),
            x: Some(1.0),
            y: Some(2.0),
            yaw: Some(0.5),
        }
    }

    fn tmpdir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("navbridge-cmd-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn token_file(name: &str, content: &str, mode: u32) -> PathBuf {
        let p = tmpdir(name).join("cmd_token");
        std::fs::write(&p, content).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(mode)).unwrap();
        p
    }

    fn policy(enabled: bool, token: Option<&str>) -> CmdPolicy {
        let token = token.map(|t| {
            static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let p = token_file(&format!("policy{n}"), t, 0o600);
            load_token(&p).ok().unwrap()
        });
        CmdPolicy { goal_enabled: enabled, token, bucket: TokenBucket::new(2.0, 2) }
    }

    fn reply_code(d: Decision) -> Option<CmdResult> {
        match d {
            Decision::Reply(r, _) => Some(r),
            Decision::Forward(_) => None,
        }
    }

    #[test]
    fn estop_and_disarm_always_forward() {
        let now = Instant::now();
        for cmd in ["estop", "disarm"] {
            for gate in [GateView::Unknown, GateView::Estop, GateView::Disarmed] {
                let mut p = policy(false, None);
                while p.bucket.take(now) {}
                let d = decide(&req(cmd, None), &mut p, gate, now);
                assert!(matches!(d, Decision::Forward(_)), "{cmd} {gate:?}");
                // the empty bucket stays empty and a full one stays full
                assert!(!p.bucket.take(now));
                let mut full = policy(false, None);
                decide(&req(cmd, None), &mut full, gate, now);
                assert!(full.bucket.take(now) && full.bucket.take(now), "{cmd} touched the bucket");
            }
        }
    }

    #[test]
    fn arm_release_unknown_not_allowed() {
        let now = Instant::now();
        for cmd in ["arm", "release", "reboot", ""] {
            let mut p = policy(true, Some("tok"));
            let d = decide(&req(cmd, Some("tok")), &mut p, GateView::Armed, now);
            assert_eq!(reply_code(d), Some(CmdResult::NotAllowed), "{cmd}");
        }
    }

    #[test]
    fn goal_checks_in_order() {
        let t0 = Instant::now();
        let goal = |tok| req("goal", tok);
        // switch off
        let mut p = policy(false, Some("tok"));
        assert_eq!(reply_code(decide(&goal(Some("tok")), &mut p, GateView::Armed, t0)), Some(CmdResult::Disabled));
        // no token on the robot
        let mut p = policy(true, None);
        assert!(!p.goal_channel_open());
        assert_eq!(reply_code(decide(&goal(Some("tok")), &mut p, GateView::Armed, t0)), Some(CmdResult::Disabled));
        // wrong or missing token
        let mut p = policy(true, Some("tok"));
        assert!(p.goal_channel_open());
        assert_eq!(reply_code(decide(&goal(Some("nope")), &mut p, GateView::Armed, t0)), Some(CmdResult::BadToken));
        assert_eq!(reply_code(decide(&goal(None), &mut p, GateView::Armed, t0)), Some(CmdResult::BadToken));
        // bad tokens do not spend the bucket; the third good one inside a second is limited,
        // and rate limiting comes before the gate check
        let mut p = policy(true, Some("tok"));
        assert!(matches!(decide(&goal(Some("tok")), &mut p, GateView::Armed, t0), Decision::Forward(_)));
        assert!(matches!(decide(&goal(Some("tok")), &mut p, GateView::Armed, t0), Decision::Forward(_)));
        let t1 = t0 + Duration::from_millis(100);
        assert_eq!(reply_code(decide(&goal(Some("tok")), &mut p, GateView::Armed, t1)), Some(CmdResult::RateLimited));
        assert_eq!(reply_code(decide(&goal(Some("tok")), &mut p, GateView::Unknown, t1)), Some(CmdResult::RateLimited));
        // gate
        for (gate, want) in [
            (GateView::Unknown, Some(CmdResult::GateUnknown)),
            (GateView::Estop, Some(CmdResult::EstopActive)),
            (GateView::Disarmed, Some(CmdResult::NotArmed)),
            (GateView::Armed, None),
        ] {
            let mut p = policy(true, Some("tok"));
            assert_eq!(reply_code(decide(&goal(Some("tok")), &mut p, gate, t0)), want, "{gate:?}");
        }
    }

    #[test]
    fn initialpose_ignores_gate() {
        let now = Instant::now();
        for gate in [GateView::Disarmed, GateView::Estop, GateView::Unknown] {
            let mut p = policy(true, Some("tok"));
            let d = decide(&req("initialpose", Some("tok")), &mut p, gate, now);
            assert!(matches!(d, Decision::Forward(_)), "{gate:?}");
        }
        // but it still needs the switch and the token
        let mut p = policy(false, Some("tok"));
        let d = decide(&req("initialpose", Some("tok")), &mut p, GateView::Armed, now);
        assert_eq!(reply_code(d), Some(CmdResult::Disabled));
        let mut p = policy(true, Some("tok"));
        let d = decide(&req("initialpose", Some("bad")), &mut p, GateView::Armed, now);
        assert_eq!(reply_code(d), Some(CmdResult::BadToken));
    }

    #[test]
    fn gate_view_rules() {
        let now = Instant::now();
        let ago = |s: f64| now - Duration::from_secs_f64(s);
        let armed = br#"{"armed":true,"estop":false}"#;
        assert_eq!(gate_view(Some((armed, ago(1.0))), now), GateView::Armed);
        assert_eq!(gate_view(Some((armed, ago(2.5))), now), GateView::Unknown);
        assert_eq!(gate_view(Some((br#"{"armed":true,"estop":true}"#, ago(0.1))), now), GateView::Estop);
        assert_eq!(gate_view(Some((br#"{"armed":false}"#, ago(0.1))), now), GateView::Disarmed);
        assert_eq!(gate_view(Some((b"{", ago(0.1))), now), GateView::Unknown);
        assert_eq!(gate_view(Some((b"{}", ago(0.1))), now), GateView::Unknown);
        assert_eq!(gate_view(None, now), GateView::Unknown);
        let fixture = include_bytes!("../tests/fixtures/gate_state.json");
        assert_eq!(gate_view(Some((fixture, now)), now), GateView::Disarmed);
    }

    #[test]
    fn token_bucket_two_per_second() {
        let t0 = Instant::now();
        let mut b = TokenBucket::new(2.0, 2);
        assert_eq!([b.take(t0), b.take(t0), b.take(t0)], [true, true, false]);
        assert!(b.take(t0 + Duration::from_millis(500)));
        assert!(!b.take(t0 + Duration::from_millis(500)));
    }

    #[test]
    fn token_trims_whitespace_and_crlf() {
        let p = token_file("crlf", "abc123\r\n", 0o600);
        let t = load_token(&p).ok().unwrap();
        assert!(t.matches("  abc123 "));
        assert!(t.matches("abc123"));
        assert!(!t.matches("abc12"));
        assert!(!t.matches("abc1234"));
        assert!(!t.matches(""));
    }

    #[test]
    fn load_token_rejects() {
        let d = tmpdir("reject");
        assert!(load_token(&d.join("missing")).is_err());
        let blank = token_file("blank", " \r\n\t", 0o600);
        assert!(load_token(&blank).is_err());
        let open = token_file("open", "secret-value", 0o644);
        let err = load_token(&open).err().unwrap();
        assert!(!err.contains("secret-value"));
        let grp = token_file("grp", "secret-value", 0o640);
        assert!(load_token(&grp).is_err());
        let ok = token_file("ok", "secret-value", 0o600);
        assert!(load_token(&ok).is_ok());
        let ro = token_file("ro", "secret-value", 0o400);
        assert!(load_token(&ro).is_ok());
    }

    #[test]
    fn expand_home_only_leading() {
        let h = Path::new("/home/x");
        assert_eq!(expand_home("~/a/b", Some(h)), PathBuf::from("/home/x/a/b"));
        assert_eq!(expand_home("/a/~/b", Some(h)), PathBuf::from("/a/~/b"));
        assert_eq!(expand_home("~x/b", Some(h)), PathBuf::from("~x/b"));
        assert_eq!(expand_home("~/a", None), PathBuf::from("~/a"));
    }

    #[test]
    fn payload_never_has_token() {
        let v = |r: &CmdRequest| -> serde_json::Value { serde_json::from_slice(&nav_command_payload(r)).unwrap() };
        let g = v(&req("goal", Some("sekret")));
        assert!(g.get("token").is_none());
        assert!(!String::from_utf8(nav_command_payload(&req("goal", Some("sekret")))).unwrap().contains("sekret"));
        assert_eq!(g["id"], "s1-abcd");
        assert_eq!(g["cmd"], "goal");
        assert_eq!(g["x"], 1.0);
        assert_eq!(g["yaw"], 0.5);
        let mut no_yaw = req("goal", Some("t"));
        no_yaw.yaw = None;
        assert!(v(&no_yaw).get("yaw").is_none());
        let e = v(&req("estop", Some("t")));
        assert_eq!(e["cmd"], "estop");
        assert!(e.get("token").is_none());
    }
}
