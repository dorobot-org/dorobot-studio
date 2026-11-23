//! Fold the latest value of each status input into a `NavStatus`, and mark the parts
//! whose input has stopped.

use crate::formats::Odom;
use dviz_protocol::{nav_status_parts as part, NavStatus};
use serde_json::Value;
use std::time::{Duration, Instant};

/// A part whose input has not arrived for longer than this is stale. Every input
/// behind a part is periodic: loc_trust per odometry, loc_refine every 0.5 s,
/// cmd_gate every 1 s, the chassis every 0.2 s, map_base per odometry. The goal
/// status is sent only when it changes, so it is never stale.
pub const STALE_AFTER: Duration = Duration::from_secs(2);

#[derive(Default)]
pub struct StatusBuilder {
    cur: NavStatus,
    last_built: Option<NavStatus>,
    trust_at: Option<Instant>,
    refine_at: Option<Instant>,
    gate_at: Option<Instant>,
    chassis_at: Option<Instant>,
    odom_at: Option<Instant>,
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_owned)
}

fn b(v: &Value, k: &str) -> Option<bool> {
    v.get(k).and_then(Value::as_bool)
}

fn i(v: &Value, k: &str) -> Option<i32> {
    v.get(k).and_then(Value::as_i64).and_then(|n| i32::try_from(n).ok())
}

fn f(v: &Value, k: &str) -> Option<f32> {
    v.get(k).and_then(Value::as_f64).map(|x| x as f32).filter(|x| x.is_finite())
}

/// Received within [`STALE_AFTER`] of `now`.
fn fresh(at: Option<Instant>, now: Instant) -> bool {
    at.is_some_and(|at| now.saturating_duration_since(at) <= STALE_AFTER)
}

impl StatusBuilder {
    /// `status == 0` means NORMAL (trusted).
    pub fn on_trust(&mut self, v: &Value, now: Instant) {
        self.cur.loc_ok = v.get("status").and_then(Value::as_i64) == Some(0);
        self.cur.loc_reason = s(v, "reason").unwrap_or_default();
        self.trust_at = Some(now);
    }

    pub fn on_refine(&mut self, v: &Value, now: Instant) {
        self.cur.refine_state = s(v, "state");
        self.refine_at = Some(now);
    }

    pub fn on_gate(&mut self, v: &Value, now: Instant) {
        self.cur.armed = b(v, "armed").unwrap_or(false);
        self.cur.estop = b(v, "estop").unwrap_or(false);
        self.cur.gate_reason = s(v, "reason").unwrap_or_default();
        self.gate_at = Some(now);
    }

    /// `mode` is the chassis node's arm state; `control_mode` the base's (1 CAN,
    /// 3 RC, -1 no feedback).
    pub fn on_chassis(&mut self, v: &Value, now: Instant) {
        self.cur.chassis_mode = s(v, "mode");
        self.cur.chassis_armed = b(v, "armed");
        self.cur.chassis_reason = s(v, "reason");
        self.cur.chassis_control_mode = i(v, "control_mode");
        self.cur.chassis_error_code = i(v, "error_code");
        self.cur.battery_v = f(v, "battery_v");
        self.chassis_at = Some(now);
    }

    pub fn on_goal_status(&mut self, id: &str, state: &str, reason: &str) {
        self.cur.goal_id = Some(id.to_owned());
        self.cur.goal_state = Some(state.to_owned());
        self.cur.goal_reason = Some(reason.to_owned());
    }

    pub fn on_odom(&mut self, o: &Odom, now: Instant) {
        self.cur.v = Some(o.v as f32);
        self.cur.w = Some(o.w as f32);
        self.odom_at = Some(now);
    }

    /// The parts whose input stopped more than [`STALE_AFTER`] before `now`, or never
    /// came. The localization part is stale without a trust status, and also when
    /// loc_refine's state, once heard, stops.
    fn stale(&self, now: Instant) -> Vec<String> {
        let localization = !fresh(self.trust_at, now) || (self.refine_at.is_some() && !fresh(self.refine_at, now));
        [
            (part::LOCALIZATION, localization),
            (part::GATE, !fresh(self.gate_at, now)),
            (part::CHASSIS, !fresh(self.chassis_at, now)),
            (part::SPEED, !fresh(self.odom_at, now)),
        ]
        .into_iter()
        .filter(|(_, stale)| *stale)
        .map(|(name, _)| name.to_owned())
        .collect()
    }

    /// Current status at `now`, and whether it differs from the previous `build`
    /// (a speed change alone does not count; a part going stale or fresh does).
    pub fn build(&mut self, now: Instant) -> (NavStatus, bool) {
        self.cur.stale = self.stale(now);
        let strip = |s: &NavStatus| NavStatus { v: None, w: None, ..s.clone() };
        let status = self.cur.clone();
        let changed = self.last_built.as_ref() != Some(&strip(&status));
        self.last_built = Some(strip(&status));
        (status, changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dviz_protocol::nav_status_parts as part;
    use serde_json::json;
    use std::time::{Duration, Instant};

    fn j(s: &str) -> Value {
        serde_json::from_str(s).unwrap()
    }

    fn odom(v: f64, w: f64) -> Odom {
        Odom { stamp: 0.0, x: 0.0, y: 0.0, yaw: 0.0, v, w }
    }

    /// Every periodic input once, at `now`.
    fn feed(b: &mut StatusBuilder, now: Instant) {
        b.on_trust(&j(include_str!("../tests/fixtures/trust.json")), now);
        b.on_refine(&j(include_str!("../tests/fixtures/refine_state.json")), now);
        b.on_gate(&j(include_str!("../tests/fixtures/gate_state.json")), now);
        b.on_chassis(&j(include_str!("../tests/fixtures/chassis_state.json")), now);
        b.on_odom(&odom(0.0, 0.0), now);
    }

    #[test]
    fn status_from_samples() {
        let mut b = StatusBuilder::default();
        let now = Instant::now();
        feed(&mut b, now);
        b.on_goal_status("C1", "ACTIVE", "");
        let (s, changed) = b.build(now);
        assert!(changed);
        assert!(!s.loc_ok);
        assert_eq!(s.loc_reason, "no_initialpose");
        assert_eq!(s.refine_state.as_deref(), Some("WAIT_DATA"));
        assert!(!s.armed && !s.estop);
        assert_eq!(s.gate_reason, "disarmed");
        assert_eq!(s.chassis_mode.as_deref(), Some("disarmed"));
        assert_eq!(s.chassis_armed, Some(false));
        assert_eq!(s.chassis_reason.as_deref(), Some("boot"));
        // The fixture is from a base without feedback.
        assert_eq!(s.chassis_control_mode, Some(-1));
        assert_eq!(s.chassis_error_code, Some(0));
        assert_eq!(s.battery_v, Some(0.0));
        assert_eq!(s.goal_id.as_deref(), Some("C1"));
        assert_eq!(s.goal_state.as_deref(), Some("ACTIVE"));
        assert!(!s.cmd_goal_enabled);
        assert!(s.stale.is_empty(), "{:?}", s.stale);
        let (s2, changed) = b.build(now);
        assert!(!changed);
        assert_eq!(s, s2);
    }

    /// A real `tracer/state` with the base on the remote (control_mode 3) and an
    /// error bit set.
    #[test]
    fn chassis_control_mode_error_and_battery() {
        let mut b = StatusBuilder::default();
        let now = Instant::now();
        b.on_chassis(
            &json!({"armed": false, "battery_v": 25.6, "control_mode": 3, "error_code": 4, "estop": false,
                    "mode": "disarmed", "reason": "boot", "timestamp": 1795406866.1653068, "vehicle_state": 2}),
            now,
        );
        let (s, _) = b.build(now);
        assert_eq!(s.chassis_control_mode, Some(3));
        assert_eq!(s.chassis_error_code, Some(4));
        assert_eq!(s.battery_v, Some(25.6));
        assert_eq!(s.chassis_mode.as_deref(), Some("disarmed"));
        assert_eq!(s.chassis_reason.as_deref(), Some("boot"));
        // Fields missing from an older chassis node are none.
        b.on_chassis(&json!({"mode": "armed", "armed": true}), now);
        let (s, changed) = b.build(now);
        assert!(changed);
        assert_eq!((s.chassis_control_mode, s.chassis_error_code, s.battery_v), (None, None, None));
    }

    #[test]
    fn velocity_does_not_count_as_change_but_is_reported() {
        let mut b = StatusBuilder::default();
        let now = Instant::now();
        feed(&mut b, now);
        let _ = b.build(now);
        b.on_odom(&odom(0.5, -0.25), now);
        let (s, changed) = b.build(now);
        assert!(!changed);
        assert_eq!((s.v, s.w), (Some(0.5), Some(-0.25)));
        b.on_trust(&json!({"status": 0, "reason": "ok"}), now);
        let (s, changed) = b.build(now);
        assert!(changed && s.loc_ok);
    }

    /// A part whose input has not come for more than 2 s (or never) is listed in
    /// `stale`, its last values kept; that counts as a change.
    #[test]
    fn a_part_without_input_for_2_s_is_stale() {
        let t0 = Instant::now();
        let at = |s: f32| t0 + Duration::from_secs_f32(s);
        let mut b = StatusBuilder::default();
        let (s, _) = b.build(t0);
        assert_eq!(s.stale, part::ALL, "nothing received yet");
        feed(&mut b, t0);
        let (s, changed) = b.build(at(0.1));
        assert!(s.stale.is_empty() && changed, "{:?}", s.stale);
        let (s, changed) = b.build(at(2.0));
        assert!(s.stale.is_empty() && !changed, "exactly 2 s is not yet stale: {:?}", s.stale);
        // Only the chassis keeps sending.
        b.on_chassis(&j(include_str!("../tests/fixtures/chassis_state.json")), at(1.9));
        let (s, changed) = b.build(at(2.1));
        assert!(changed);
        assert_eq!(s.stale, [part::LOCALIZATION, part::GATE, part::SPEED]);
        assert_eq!(s.gate_reason, "disarmed", "the last values are kept");
        assert_eq!(s.loc_reason, "no_initialpose");
        // Fresh again.
        feed(&mut b, at(3.0));
        let (s, changed) = b.build(at(3.0));
        assert!(changed && s.stale.is_empty(), "{:?}", s.stale);
        // The trust status alone is not enough once loc_refine has been heard:
        // its state stopping makes the localization part stale.
        b.on_trust(&j(include_str!("../tests/fixtures/trust.json")), at(5.0));
        b.on_gate(&j(include_str!("../tests/fixtures/gate_state.json")), at(5.0));
        b.on_chassis(&j(include_str!("../tests/fixtures/chassis_state.json")), at(5.0));
        b.on_odom(&odom(0.0, 0.0), at(5.0));
        let (s, _) = b.build(at(5.1));
        assert_eq!(s.stale, [part::LOCALIZATION]);
    }

    #[test]
    fn odd_json_is_tolerated() {
        let mut b = StatusBuilder::default();
        let now = Instant::now();
        b.on_trust(&json!("junk"), now);
        b.on_gate(&json!({"armed": "yes"}), now);
        b.on_chassis(&json!({"control_mode": "CAN", "error_code": 1e30, "battery_v": "full"}), now);
        let (s, _) = b.build(now);
        assert!(!s.loc_ok && !s.armed);
        assert_eq!((s.chassis_control_mode, s.chassis_error_code, s.battery_v), (None, None, None));
    }
}
