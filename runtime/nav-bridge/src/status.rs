//! Fold the latest value of each status input into a `NavStatus`.

use crate::formats::Odom;
use dviz_protocol::NavStatus;
use serde_json::Value;

#[derive(Default)]
pub struct StatusBuilder {
    cur: NavStatus,
    last_built: Option<NavStatus>,
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_owned)
}

fn b(v: &Value, k: &str) -> Option<bool> {
    v.get(k).and_then(Value::as_bool)
}

impl StatusBuilder {
    /// `status == 0` means NORMAL (trusted).
    pub fn on_trust(&mut self, v: &Value) {
        self.cur.loc_ok = v.get("status").and_then(Value::as_i64) == Some(0);
        self.cur.loc_reason = s(v, "reason").unwrap_or_default();
    }

    pub fn on_refine(&mut self, v: &Value) {
        self.cur.refine_state = s(v, "state");
    }

    pub fn on_gate(&mut self, v: &Value) {
        self.cur.armed = b(v, "armed").unwrap_or(false);
        self.cur.estop = b(v, "estop").unwrap_or(false);
        self.cur.gate_reason = s(v, "reason").unwrap_or_default();
    }

    pub fn on_chassis(&mut self, v: &Value) {
        self.cur.chassis_mode = s(v, "mode");
        self.cur.chassis_armed = b(v, "armed");
        self.cur.chassis_reason = s(v, "reason");
    }

    pub fn on_goal_status(&mut self, id: &str, state: &str, reason: &str) {
        self.cur.goal_id = Some(id.to_owned());
        self.cur.goal_state = Some(state.to_owned());
        self.cur.goal_reason = Some(reason.to_owned());
    }

    pub fn on_odom(&mut self, o: &Odom) {
        self.cur.v = Some(o.v as f32);
        self.cur.w = Some(o.w as f32);
    }

    /// Current status, and whether it differs from the previous `build`
    /// (a speed change alone does not count).
    pub fn build(&mut self) -> (NavStatus, bool) {
        let strip = |s: &NavStatus| NavStatus { v: None, w: None, ..s.clone() };
        let now = self.cur.clone();
        let changed = self.last_built.as_ref() != Some(&strip(&now));
        self.last_built = Some(strip(&now));
        (now, changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn feed(b: &mut StatusBuilder) {
        let j = |s: &str| serde_json::from_str::<serde_json::Value>(s).unwrap();
        b.on_trust(&j(include_str!("../tests/fixtures/trust.json")));
        b.on_refine(&j(include_str!("../tests/fixtures/refine_state.json")));
        b.on_gate(&j(include_str!("../tests/fixtures/gate_state.json")));
        b.on_chassis(&j(include_str!("../tests/fixtures/chassis_state.json")));
    }

    #[test]
    fn status_from_samples() {
        let mut b = StatusBuilder::default();
        feed(&mut b);
        b.on_goal_status("C1", "ACTIVE", "");
        let (s, changed) = b.build();
        assert!(changed);
        assert!(!s.loc_ok);
        assert_eq!(s.loc_reason, "no_initialpose");
        assert_eq!(s.refine_state.as_deref(), Some("WAIT_DATA"));
        assert!(!s.armed && !s.estop);
        assert_eq!(s.gate_reason, "disarmed");
        assert_eq!(s.chassis_mode.as_deref(), Some("disarmed"));
        assert_eq!(s.chassis_armed, Some(false));
        assert_eq!(s.chassis_reason.as_deref(), Some("boot"));
        assert_eq!(s.goal_id.as_deref(), Some("C1"));
        assert_eq!(s.goal_state.as_deref(), Some("ACTIVE"));
        assert!(!s.cmd_goal_enabled);
        let (s2, changed) = b.build();
        assert!(!changed);
        assert_eq!(s, s2);
    }

    #[test]
    fn velocity_does_not_count_as_change_but_is_reported() {
        let mut b = StatusBuilder::default();
        feed(&mut b);
        let _ = b.build();
        let o = crate::formats::Odom { stamp: 0.0, x: 0.0, y: 0.0, yaw: 0.0, v: 0.5, w: -0.25 };
        b.on_odom(&o);
        let (s, changed) = b.build();
        assert!(!changed);
        assert_eq!((s.v, s.w), (Some(0.5), Some(-0.25)));
        b.on_trust(&serde_json::json!({"status": 0, "reason": "ok"}));
        let (s, changed) = b.build();
        assert!(changed && s.loc_ok);
    }

    #[test]
    fn odd_json_is_tolerated() {
        let mut b = StatusBuilder::default();
        b.on_trust(&serde_json::json!("junk"));
        b.on_gate(&serde_json::json!({"armed": "yes"}));
        let (s, _) = b.build();
        assert!(!s.loc_ok && !s.armed);
    }
}
