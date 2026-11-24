//! Sends one navigation command to a robot's bridge as a Zenoh query on
//! `dviz/<robot>/cmd` (what Studio's tools send) and prints the reply, a `CmdAck` as JSON,
//! on stdout.
//!
//! ```text
//! dviz_cmd --connect tcp/<host>:<port> --robot <id> --cmd <goal|initialpose|estop|disarm|arm|release>
//!          [--x X --y Y --yaw RAD] [--id ID] [--token-file PATH] [--wait-goal-state STATE --wait-s N]
//! ```
//!
//! - `--token-file`: the token is read from it (surrounding whitespace removed) and sent;
//!   without it no token is sent. A leading `~/` is `$HOME`.
//! - `--id`: the request id (default `cli-<unix seconds>-<pid>`); a goal carries it to
//!   nav_command as the goal id.
//! - `--wait-goal-state` (goal only): after an `ok` reply, follow `dviz/<robot>/nav/status`
//!   until `goal_id` is the request id and `goal_state` is STATE (e.g. `SUCCEEDED`), for at
//!   most `--wait-s` seconds (default 60). State changes are printed on stderr.
//!
//! estop and disarm go with `Priority::RealTime` and `CongestionControl::Block`, as Studio
//! sends them. The query times out after 3 s.
//!
//! Exit 0: a reply came (any result), or with `--wait-goal-state` the state was reached.
//! 1: no reply; or with `--wait-goal-state` a result other than `ok` or the state not
//! reached in time. 2: bad arguments, an unreadable token file or a Zenoh error.

use dorobot_nav_bridge::command::expand_home;
use dviz_protocol::{cmd_key, nav_entities, parse_cmd_ack, parse_message, CmdRequest, CmdResult, NavStatus};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use zenoh::qos::{CongestionControl, Priority};
use zenoh::Wait;

/// How long a command query waits for the bridge's reply (as in Studio).
const QUERY_TIMEOUT: Duration = Duration::from_secs(3);

const USAGE: &str = "usage: dviz_cmd --connect tcp/<host>:<port> --robot <id> \
--cmd <goal|initialpose|estop|disarm|arm|release> [--x X --y Y --yaw RAD] [--id ID] \
[--token-file PATH] [--wait-goal-state STATE --wait-s N]";

const COMMANDS: [&str; 6] = ["goal", "initialpose", "estop", "disarm", "arm", "release"];

#[derive(Debug, Clone, PartialEq)]
pub struct Args {
    pub connect: String,
    pub robot: String,
    pub cmd: String,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub yaw: Option<f64>,
    pub id: Option<String>,
    pub token_file: Option<String>,
    pub wait_goal_state: Option<String>,
    pub wait_s: f64,
}

fn number(flag: &str, v: &str) -> Result<f64, String> {
    v.parse::<f64>().map_err(|_| format!("{flag} {v:?}: not a number"))
}

/// Every flag takes a value; `--connect`, `--robot` and `--cmd` are required.
pub fn parse_args(args: &[String]) -> Result<Args, String> {
    let (mut connect, mut robot, mut cmd) = (None, None, None);
    let mut a = Args {
        connect: String::new(),
        robot: String::new(),
        cmd: String::new(),
        x: None,
        y: None,
        yaw: None,
        id: None,
        token_file: None,
        wait_goal_state: None,
        wait_s: 60.0,
    };
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let v = it.next().ok_or_else(|| format!("{flag} needs a value"))?.clone();
        match flag.as_str() {
            "--connect" => connect = Some(v),
            "--robot" => robot = Some(v),
            "--cmd" => cmd = Some(v),
            "--x" => a.x = Some(number(flag, &v)?),
            "--y" => a.y = Some(number(flag, &v)?),
            "--yaw" => a.yaw = Some(number(flag, &v)?),
            "--id" => a.id = Some(v),
            "--token-file" => a.token_file = Some(v),
            "--wait-goal-state" => a.wait_goal_state = Some(v),
            "--wait-s" => a.wait_s = Some(number(flag, &v)?).filter(|s| *s > 0.0).ok_or("--wait-s must be > 0")?,
            _ => return Err(format!("unknown flag {flag}")),
        }
    }
    a.connect = connect.ok_or("--connect is required")?;
    a.robot = robot.ok_or("--robot is required")?;
    a.cmd = cmd.ok_or("--cmd is required")?;
    if !COMMANDS.contains(&a.cmd.as_str()) {
        return Err(format!("--cmd {:?}: one of {}", a.cmd, COMMANDS.join(", ")));
    }
    if a.wait_goal_state.is_some() && a.cmd != "goal" {
        return Err("--wait-goal-state only goes with --cmd goal".into());
    }
    Ok(a)
}

/// The token in `path` (`~/` is `home`), surrounding whitespace removed; never empty.
pub fn read_token(path: &str, home: Option<&Path>) -> Result<String, String> {
    let path = expand_home(path, home);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("token file {}: {e}", path.display()))?;
    let token = text.trim();
    if token.is_empty() {
        return Err(format!("token file {}: empty", path.display()));
    }
    Ok(token.to_owned())
}

/// The query payload.
pub fn request(a: &Args, id: &str, token: Option<String>) -> Vec<u8> {
    let req = CmdRequest { cmd: a.cmd.clone(), id: id.to_owned(), token, x: a.x, y: a.y, yaw: a.yaw };
    serde_json::to_vec(&req).unwrap_or_default()
}

/// The `NavStatus` in a `nav/status` message.
pub fn status_of(payload: &[u8]) -> Option<NavStatus> {
    let (m, _) = parse_message(payload)?;
    serde_json::from_value(m.data).ok()
}

pub fn goal_reached(s: &NavStatus, id: &str, state: &str) -> bool {
    s.goal_id.as_deref() == Some(id) && s.goal_state.as_deref() == Some(state)
}

fn die(msg: &str) -> ! {
    eprintln!("dviz_cmd: {msg}");
    std::process::exit(2)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let a = parse_args(&args).unwrap_or_else(|e| {
        eprintln!("dviz_cmd: {e}\n{USAGE}");
        std::process::exit(2)
    });
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let token = a.token_file.as_deref().map(|p| read_token(p, home.as_deref()).unwrap_or_else(|e| die(&e)));
    let id = a.id.clone().unwrap_or_else(|| {
        let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
        format!("cli-{secs}-{}", std::process::id())
    });

    let mut config = zenoh::Config::default();
    for (key, value) in [
        ("mode", r#""client""#.to_owned()),
        ("connect/endpoints", serde_json::to_string(&[&a.connect]).unwrap_or_default()),
        ("listen/endpoints", "[]".to_owned()),
        ("scouting/multicast/enabled", "false".to_owned()),
    ] {
        config.insert_json5(key, &value).unwrap_or_else(|e| die(&format!("zenoh config {key}: {e}")));
    }
    let session = zenoh::open(config).wait().unwrap_or_else(|e| die(&format!("zenoh open ({}): {e}", a.connect)));

    // Follow nav/status before sending, so nothing after the goal is missed.
    let (status_tx, status_rx) = mpsc::channel::<NavStatus>();
    let _status_sub = a.wait_goal_state.as_ref().map(|_| {
        session
            .declare_subscriber(format!("dviz/{}/{}", a.robot, nav_entities::NAV_STATUS))
            .callback(move |sample| {
                if let Some(s) = status_of(&sample.payload().to_bytes()) {
                    let _ = status_tx.send(s);
                }
            })
            .wait()
            .unwrap_or_else(|e| die(&format!("zenoh subscribe: {e}")))
    });

    let mut get = session.get(cmd_key("dviz", &a.robot)).payload(request(&a, &id, token)).timeout(QUERY_TIMEOUT);
    if a.cmd == "estop" || a.cmd == "disarm" {
        get = get.priority(Priority::RealTime).congestion_control(CongestionControl::Block);
    }
    let replies = get.wait().unwrap_or_else(|e| die(&format!("zenoh query: {e}")));
    let reply = match replies.recv() {
        Ok(reply) => reply,
        Err(_) => {
            eprintln!("dviz_cmd: no reply from dviz/{}/cmd within {QUERY_TIMEOUT:?}", a.robot);
            std::process::exit(1)
        }
    };
    let payload = match reply.result() {
        Ok(sample) => sample.payload().to_bytes().into_owned(),
        Err(e) => {
            eprintln!("dviz_cmd: error reply: {}", String::from_utf8_lossy(&e.payload().to_bytes()));
            std::process::exit(1)
        }
    };
    println!("{}", String::from_utf8_lossy(&payload));

    let Some(want) = a.wait_goal_state else { return };
    match parse_cmd_ack(&payload) {
        Ok(ack) if ack.result == CmdResult::Ok => {}
        _ => {
            eprintln!("dviz_cmd: the goal was not accepted; not waiting for {want}");
            std::process::exit(1)
        }
    }
    let deadline = Instant::now() + Duration::from_secs_f64(a.wait_s);
    let mut last = None;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let Ok(s) = status_rx.recv_timeout(left) else {
            eprintln!("dviz_cmd: goal {id} not {want} within {} s (last: {last:?})", a.wait_s);
            std::process::exit(1)
        };
        let seen = (s.goal_id.clone(), s.goal_state.clone());
        if last.as_ref() != Some(&seen) {
            eprintln!("dviz_cmd: nav/status goal {:?} {:?}", seen.0, seen.1);
            last = Some(seen);
        }
        if goal_reached(&s, &id, &want) {
            eprintln!("dviz_cmd: goal {id} {want}");
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn parses_flags() {
        let a = parse_args(&args(
            "--connect tcp/127.0.0.1:7451 --robot tracer_sim --cmd goal --x 0 --y 1.5 --yaw -0.5 --id t1 \
             --token-file /tmp/tok --wait-goal-state SUCCEEDED --wait-s 30",
        ))
        .unwrap();
        assert_eq!((a.connect.as_str(), a.robot.as_str(), a.cmd.as_str()), ("tcp/127.0.0.1:7451", "tracer_sim", "goal"));
        assert_eq!((a.x, a.y, a.yaw), (Some(0.0), Some(1.5), Some(-0.5)));
        assert_eq!((a.id.as_deref(), a.token_file.as_deref()), (Some("t1"), Some("/tmp/tok")));
        assert_eq!((a.wait_goal_state.as_deref(), a.wait_s), (Some("SUCCEEDED"), 30.0));
        let e = parse_args(&args("--connect tcp/h:1 --robot r --cmd estop")).unwrap();
        assert_eq!((e.x, e.id, e.token_file, e.wait_goal_state, e.wait_s), (None, None, None, None, 60.0));
    }

    #[test]
    fn rejects_bad_flags() {
        for (line, why) in [
            ("--robot r --cmd estop", "--connect is required"),
            ("--connect c --cmd estop", "--robot is required"),
            ("--connect c --robot r", "--cmd is required"),
            ("--connect c --robot r --cmd reboot", "one of goal"),
            ("--connect c --robot r --cmd goal --x one", "not a number"),
            ("--connect c --robot r --cmd goal --wait-s 0", "> 0"),
            ("--connect c --robot r --cmd estop --wait-goal-state SUCCEEDED", "only goes with --cmd goal"),
            ("--connect c --robot r --cmd goal --x", "needs a value"),
            ("--connect c --robot r --cmd goal --speed 3", "unknown flag"),
        ] {
            let err = parse_args(&args(line)).unwrap_err();
            assert!(err.contains(why), "{line}: {err}");
        }
    }

    #[test]
    fn request_has_the_token_only_when_given() {
        let a = parse_args(&args("--connect c --robot r --cmd goal --x 1 --y 2")).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&request(&a, "t1", None)).unwrap();
        assert_eq!(v, serde_json::json!({"cmd": "goal", "id": "t1", "x": 1.0, "y": 2.0}));
        let v: serde_json::Value = serde_json::from_slice(&request(&a, "t1", Some("tok".into()))).unwrap();
        assert_eq!(v["token"], "tok");
        assert!(dviz_protocol::parse_cmd_request(&request(&a, "t1", None)).is_ok());
    }

    #[test]
    fn token_file_is_trimmed_and_never_empty() {
        let dir = std::env::temp_dir().join(format!("dviz-cmd-tok-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("tok"), "  abc123\r\n").unwrap();
        std::fs::write(dir.join("blank"), " \n").unwrap();
        assert_eq!(read_token("~/tok", Some(&dir)).unwrap(), "abc123");
        assert!(read_token("~/blank", Some(&dir)).unwrap_err().contains("empty"));
        assert!(read_token("~/missing", Some(&dir)).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn goal_state_match_needs_id_and_state() {
        let s = |id: &str, state: &str| NavStatus {
            goal_id: Some(id.into()),
            goal_state: Some(state.into()),
            ..NavStatus::default()
        };
        assert!(goal_reached(&s("t1", "SUCCEEDED"), "t1", "SUCCEEDED"));
        assert!(!goal_reached(&s("t1", "ACTIVE"), "t1", "SUCCEEDED"));
        assert!(!goal_reached(&s("t1-r1", "SUCCEEDED"), "t1", "SUCCEEDED"));
        assert!(!goal_reached(&NavStatus::default(), "t1", "SUCCEEDED"));
        // what the bridge publishes on nav/status
        let (_, bytes) = dorobot_nav_bridge::encode::enc_status(&s("t1", "ACTIVE"));
        assert_eq!(status_of(&bytes), Some(s("t1", "ACTIVE")));
        assert_eq!(status_of(b"junk"), None);
    }
}
