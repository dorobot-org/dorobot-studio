//! Navigation commands sent by Studio to a robot bridge as Zenoh queries.
//!
//! Query key: `{prefix}/{robot_id}/cmd`. The request is JSON (at most
//! [`CMD_MAX_BYTES`]); the reply is a [`CmdAck`].

use serde::{Deserialize, Serialize};
use std::fmt;

/// Last key segment of the command query: `{prefix}/{robot_id}/cmd`.
pub const CMD_ENTITY: &str = "cmd";
/// Largest accepted request or reply, in bytes.
pub const CMD_MAX_BYTES: usize = 1024;
/// Longest request id, in characters.
pub const CMD_ID_MAX: usize = 64;

/// Query key for commands to one robot.
pub fn cmd_key(prefix: &str, robot_id: &str) -> String {
    format!("{prefix}/{robot_id}/{CMD_ENTITY}")
}

/// Command request (query payload). `Debug` never shows the token.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct CmdRequest {
    pub cmd: String,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yaw: Option<f64>,
}

impl fmt::Debug for CmdRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CmdRequest")
            .field("cmd", &self.cmd)
            .field("id", &self.id)
            .field("token", &self.token.as_ref().map(|_| "<redacted>"))
            .field("x", &self.x)
            .field("y", &self.y)
            .field("yaw", &self.yaw)
            .finish()
    }
}

/// Kind of command; anything unrecognised is `Other`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CmdKind {
    Goal,
    InitialPose,
    Estop,
    Disarm,
    Other,
}

impl CmdRequest {
    pub fn kind(&self) -> CmdKind {
        match self.cmd.as_str() {
            "goal" => CmdKind::Goal,
            "initialpose" => CmdKind::InitialPose,
            "estop" => CmdKind::Estop,
            "disarm" => CmdKind::Disarm,
            _ => CmdKind::Other,
        }
    }
}

fn finite(v: Option<f64>) -> bool {
    v.is_some_and(f64::is_finite)
}

/// Check size, JSON, id rules and the fields the command kind needs
/// (goal: finite x, y, yaw optional but finite; initialpose: finite x, y, yaw).
/// `Other` commands get no coordinate check.
pub fn parse_cmd_request(bytes: &[u8]) -> Result<CmdRequest, String> {
    if bytes.len() > CMD_MAX_BYTES {
        return Err(format!("request too large ({} > {CMD_MAX_BYTES} bytes)", bytes.len()));
    }
    let req: CmdRequest = serde_json::from_slice(bytes).map_err(|e| format!("bad json: {e}"))?;
    if req.id.is_empty() || req.id.len() > CMD_ID_MAX {
        return Err(format!("id must be 1-{CMD_ID_MAX} characters"));
    }
    if !req.id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-') {
        return Err("id may only contain [A-Za-z0-9_-]".into());
    }
    match req.kind() {
        CmdKind::Goal => {
            if !finite(req.x) || !finite(req.y) {
                return Err("goal needs finite x and y".into());
            }
            if req.yaw.is_some_and(|v| !v.is_finite()) {
                return Err("yaw must be finite".into());
            }
        }
        CmdKind::InitialPose => {
            if !finite(req.x) || !finite(req.y) || !finite(req.yaw) {
                return Err("initialpose needs finite x, y and yaw".into());
            }
        }
        CmdKind::Estop | CmdKind::Disarm | CmdKind::Other => {}
    }
    Ok(req)
}

/// Result code on the wire (`snake_case`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CmdResult {
    Ok,
    NotAllowed,
    Disabled,
    BadToken,
    NotArmed,
    EstopActive,
    GateUnknown,
    RateLimited,
    BadRequest,
    NavRejected,
    NoReply,
}

/// Reply to a command query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CmdAck {
    pub id: String,
    pub cmd: String,
    pub result: CmdResult,
    pub reason: String,
}

/// Parse a reply (at most [`CMD_MAX_BYTES`]).
pub fn parse_cmd_ack(bytes: &[u8]) -> Result<CmdAck, String> {
    if bytes.len() > CMD_MAX_BYTES {
        return Err(format!("reply too large ({} > {CMD_MAX_BYTES} bytes)", bytes.len()));
    }
    serde_json::from_slice(bytes).map_err(|e| format!("bad json: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn goal() -> CmdRequest {
        CmdRequest { cmd: "goal".into(), id: "s1-abcd".into(), token: Some("sekret-token".into()), x: Some(1.5), y: Some(-2.0), yaw: Some(0.5) }
    }

    #[test]
    fn cmd_key_layout() {
        assert_eq!(cmd_key("dviz", "tracer"), "dviz/tracer/cmd");
    }

    #[test]
    fn request_round_trip() {
        let mut pose = goal();
        pose.cmd = "initialpose".into();
        let mut estop = CmdRequest { cmd: "estop".into(), id: "s3".into(), token: None, x: None, y: None, yaw: None };
        let mut disarm = estop.clone();
        disarm.cmd = "disarm".into();
        for req in [goal(), pose, estop.clone(), disarm] {
            let bytes = serde_json::to_vec(&req).unwrap();
            assert_eq!(parse_cmd_request(&bytes).unwrap(), req);
        }
        estop.id = "s4".into();
        let v: serde_json::Value = serde_json::to_value(&estop).unwrap();
        assert!(v.get("x").is_none() && v.get("token").is_none());
        assert_eq!(goal().kind(), CmdKind::Goal);
        assert_eq!(estop.kind(), CmdKind::Estop);
    }

    #[test]
    fn bad_requests_are_rejected() {
        let long = format!(r#"{{"cmd":"estop","id":"a","pad":"{}"}}"#, "x".repeat(1100));
        assert!(long.len() > 1024);
        let id65 = "a".repeat(65);
        let cases: Vec<Vec<u8>> = vec![
            long.into_bytes(),
            b"not json".to_vec(),
            br#"{"cmd":"estop","id":""}"#.to_vec(),
            format!(r#"{{"cmd":"estop","id":"{id65}"}}"#).into_bytes(),
            br#"{"cmd":"estop","id":"a/b"}"#.to_vec(),
            br#"{"cmd":"goal","id":"a","x":1.0}"#.to_vec(),
            br#"{"cmd":"goal","id":"a","x":1e400,"y":0.0}"#.to_vec(),
            br#"{"cmd":"initialpose","id":"a","x":1.0,"y":2.0}"#.to_vec(),
        ];
        for c in cases {
            assert!(parse_cmd_request(&c).is_err(), "{}", String::from_utf8_lossy(&c));
        }
        let exactly = format!(r#"{{"cmd":"estop","id":"{}"}}"#, "a".repeat(64));
        assert!(parse_cmd_request(exactly.as_bytes()).is_ok());
    }

    #[test]
    fn unknown_and_forbidden_commands_parse() {
        for (c, id) in [("arm", "a1"), ("release", "a2"), ("fly", "a3")] {
            let req = parse_cmd_request(format!(r#"{{"cmd":"{c}","id":"{id}"}}"#).as_bytes()).unwrap();
            assert_eq!(req.kind(), CmdKind::Other);
        }
    }

    #[test]
    fn ack_round_trip_and_codes() {
        assert_eq!(serde_json::to_string(&CmdResult::GateUnknown).unwrap(), "\"gate_unknown\"");
        use CmdResult::*;
        let all = [Ok, NotAllowed, Disabled, BadToken, NotArmed, EstopActive, GateUnknown, RateLimited, BadRequest, NavRejected, NoReply];
        assert_eq!(all.len(), 11);
        for r in all {
            let ack = CmdAck { id: "a".into(), cmd: "goal".into(), result: r, reason: "why".into() };
            let bytes = serde_json::to_vec(&ack).unwrap();
            assert_eq!(parse_cmd_ack(&bytes).unwrap(), ack);
        }
        assert!(parse_cmd_ack(br#"{"id":"a","cmd":"goal","result":"weird","reason":""}"#).is_err());
        assert!(parse_cmd_ack(&vec![b' '; 1025]).is_err());
    }

    #[test]
    fn debug_redacts_token() {
        let s = format!("{:?}", goal());
        assert!(!s.contains("sekret-token"));
        assert!(s.contains("<redacted>"));
    }
}
