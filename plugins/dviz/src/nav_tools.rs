//! Pure logic for the Navigation panel's 2D tools: the pose / goal tool
//! state machine, drag-to-pose conversion, tool availability and the
//! command / reason text. No drawing here.

// Task 7 (the panel) is the first caller; drop this then.
#![allow(dead_code)]

use std::time::{Duration, Instant};

use dviz_protocol::{nav_status_parts, NavStatus};

use crate::session::{CommandOutcome, CommandRecord, NavCommand};
use crate::session::{MORE_THAN_ONE_ROBOT, NOT_CONNECTED, NO_ROBOT_SEEN};

/// A drag shorter than this (screen px) sets no heading.
pub const TOOL_MIN_DRAG_PX: f64 = 10.0;
/// Pose Estimate preview colour (the Goal preview uses the goal layer's).
pub const POSE_PREVIEW_COLOR: [u8; 4] = [170, 90, 255, 255];

const STATUS_MAX_AGE: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    PoseEstimate,
    Goal,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ToolState {
    Idle,
    Armed(ToolKind),
    Dragging {
        kind: ToolKind,
        start_world: [f64; 2],
        start_screen: [f64; 2],
        now_screen: [f64; 2],
    },
}

#[derive(Debug, PartialEq)]
pub enum ToolOutcome {
    Nothing,
    Send(NavCommand),
    Hint(&'static str),
}

impl ToolState {
    /// Button click: the same tool again cancels, another one switches.
    pub fn toggle(&mut self, kind: ToolKind) {
        *self = if self.active() == Some(kind) {
            ToolState::Idle
        } else {
            ToolState::Armed(kind)
        };
    }

    /// Pointer down on the map; only an armed tool starts a drag.
    pub fn press(&mut self, screen: [f64; 2], world: [f64; 2]) {
        if let ToolState::Armed(kind) = *self {
            *self = ToolState::Dragging {
                kind,
                start_world: world,
                start_screen: screen,
                now_screen: screen,
            };
        }
    }

    pub fn moved(&mut self, screen: [f64; 2]) {
        if let ToolState::Dragging { now_screen, .. } = self {
            *now_screen = screen;
        }
    }

    /// Pointer up. A drag of at least `TOOL_MIN_DRAG_PX` sends with a yaw
    /// (wherever it ends, even off the map); a shorter one sends a Goal
    /// without yaw, or hints for Pose Estimate and stays armed.
    pub fn release(&mut self, screen: [f64; 2]) -> ToolOutcome {
        let ToolState::Dragging {
            kind,
            start_world,
            start_screen,
            ..
        } = *self
        else {
            return ToolOutcome::Nothing;
        };
        let [x, y] = start_world;
        let dist = (screen[0] - start_screen[0]).hypot(screen[1] - start_screen[1]);
        let far = dist >= TOOL_MIN_DRAG_PX;
        let yaw = drag_yaw(start_screen, screen);
        match (kind, far) {
            (ToolKind::Goal, _) => {
                *self = ToolState::Idle;
                ToolOutcome::Send(NavCommand::Goal {
                    x,
                    y,
                    yaw: far.then_some(yaw),
                })
            }
            (ToolKind::PoseEstimate, true) => {
                *self = ToolState::Idle;
                ToolOutcome::Send(NavCommand::InitialPose { x, y, yaw })
            }
            (ToolKind::PoseEstimate, false) => {
                *self = ToolState::Armed(kind);
                ToolOutcome::Hint("drag to set the heading")
            }
        }
    }

    /// Esc, or the tool became unavailable: back to idle, nothing sent.
    pub fn cancel(&mut self) {
        *self = ToolState::Idle;
    }

    pub fn active(&self) -> Option<ToolKind> {
        match *self {
            ToolState::Idle => None,
            ToolState::Armed(kind) | ToolState::Dragging { kind, .. } => Some(kind),
        }
    }
}

/// Map-frame heading of a screen drag (screen y points down).
pub fn drag_yaw(start_screen: [f64; 2], end_screen: [f64; 2]) -> f64 {
    // `start - end` (not `-(end - start)`) keeps a leftward drag at +pi, not -pi.
    (start_screen[1] - end_screen[1]).atan2(end_screen[0] - start_screen[0])
}

pub struct ToolInputs<'a> {
    pub connected: bool,
    pub robots: usize,
    pub has_token: bool,
    pub status: Option<&'a NavStatus>,
    pub status_age: Option<Duration>,
}

#[derive(Debug, PartialEq)]
pub struct Availability {
    pub pose: Result<(), &'static str>,
    pub goal: Result<(), &'static str>,
}

/// First matching row of the design's table, per tool.
pub fn tool_availability(inputs: &ToolInputs) -> Availability {
    let common = common_reason(inputs);
    let Some(status) = inputs.status else {
        // `common_reason` already rejected a missing status.
        let r = common.map_or(Ok(()), Err);
        return Availability { pose: r, goal: r };
    };
    let goal = match common {
        Some(r) => Err(r),
        None if status.stale.iter().any(|p| p == nav_status_parts::GATE) => {
            Err("gate state unknown")
        }
        None if status.estop => Err("e-stop latched (release it on the robot)"),
        None if !status.armed => Err("robot not armed (arm it on the robot)"),
        None => Ok(()),
    };
    Availability {
        pose: common.map_or(Ok(()), Err),
        goal,
    }
}

/// Reasons that switch off both tools.
fn common_reason(i: &ToolInputs) -> Option<&'static str> {
    if !i.connected {
        return Some(NOT_CONNECTED);
    }
    if i.robots == 0 {
        return Some(NO_ROBOT_SEEN);
    }
    if i.robots > 1 {
        return Some(MORE_THAN_ONE_ROBOT);
    }
    if !i.has_token {
        return Some("no token (~/.config/dviz/cmd_token)");
    }
    match (i.status, i.status_age) {
        (Some(s), Some(age)) if age <= STATUS_MAX_AGE => {
            if s.cmd_goal_enabled {
                None
            } else {
                Some("goal / pose commands not enabled on the robot")
            }
        }
        _ => Some("nav status stale"),
    }
}

/// The reason line under the receipt; empty when both tools work.
pub fn availability_text(a: &Availability) -> String {
    match (a.pose, a.goal) {
        (Err(r), Err(_)) => format!("Pose / goal tools off: {r}"),
        (Err(r), Ok(())) => format!("2D Pose Estimate off: {r}"),
        (Ok(()), Err(r)) => format!("2D Goal Pose off: {r}"),
        (Ok(()), Ok(())) => String::new(),
    }
}

fn outcome_text(o: &CommandOutcome) -> String {
    match o {
        CommandOutcome::Pending => "sending…".to_string(),
        CommandOutcome::Ack(ack) => {
            let code = serde_json::to_value(ack.result)
                .ok()
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default();
            format!("{code}: {}", ack.reason)
        }
        CommandOutcome::NotConnected(r) => format!("not sent: {r}"),
        CommandOutcome::Timeout => "no reply (robot not reachable)".to_string(),
    }
}

/// The receipt line, e.g. `Last command: goal s3-k9xq → ok: accepted  (2 s ago)`.
pub fn command_text(record: &CommandRecord, now: Instant) -> String {
    let result = match record.replies.as_slice() {
        [(_, only)] => outcome_text(only),
        many => many
            .iter()
            .map(|(robot, o)| format!("{robot}: {}", outcome_text(o)))
            .collect::<Vec<_>>()
            .join("; "),
    };
    let age = now.saturating_duration_since(record.sent).as_secs();
    format!(
        "Last command: {} {} → {result}  ({age} s ago)",
        record.cmd, record.id
    )
}

/// Any reply that is neither pending nor an `ok` ack.
pub fn command_is_error(record: &CommandRecord) -> bool {
    record.replies.iter().any(|(_, o)| match o {
        CommandOutcome::Pending => false,
        CommandOutcome::Ack(ack) => ack.result != dviz_protocol::CmdResult::Ok,
        _ => true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use dviz_protocol::{CmdAck, CmdResult};
    use std::f64::consts::FRAC_PI_2;

    fn goal_cmd(x: f64, y: f64, yaw: Option<f64>) -> ToolOutcome {
        ToolOutcome::Send(NavCommand::Goal { x, y, yaw })
    }

    #[test]
    fn goal_drag_sends_pose_with_yaw() {
        let mut t = ToolState::Idle;
        t.toggle(ToolKind::Goal);
        t.press([100.0, 100.0], [1.0, 2.0]);
        t.moved([100.0, 70.0]);
        match t.release([100.0, 50.0]) {
            ToolOutcome::Send(NavCommand::Goal { x, y, yaw: Some(yaw) }) => {
                assert_eq!((x, y), (1.0, 2.0));
                assert!((yaw - FRAC_PI_2).abs() < 1e-12);
            }
            o => panic!("{o:?}"),
        }
        assert_eq!(t.active(), None);
    }

    #[test]
    fn short_goal_click_has_no_yaw() {
        let mut t = ToolState::Idle;
        t.toggle(ToolKind::Goal);
        t.press([100.0, 100.0], [1.0, 2.0]);
        assert_eq!(t.release([103.0, 104.0]), goal_cmd(1.0, 2.0, None));
        assert_eq!(t, ToolState::Idle);
    }

    #[test]
    fn short_pose_click_hints_and_stays() {
        let mut t = ToolState::Idle;
        t.toggle(ToolKind::PoseEstimate);
        t.press([100.0, 100.0], [1.0, 2.0]);
        assert_eq!(
            t.release([105.0, 100.0]),
            ToolOutcome::Hint("drag to set the heading")
        );
        assert_eq!(t.active(), Some(ToolKind::PoseEstimate));
    }

    #[test]
    fn pose_drag_sends_initialpose() {
        let mut t = ToolState::Idle;
        t.toggle(ToolKind::PoseEstimate);
        t.press([0.0, 0.0], [1.0, 2.0]);
        match t.release([-20.0, 0.0]) {
            ToolOutcome::Send(NavCommand::InitialPose { x, y, yaw }) => {
                assert_eq!((x, y), (1.0, 2.0));
                assert!((yaw - std::f64::consts::PI).abs() < 1e-12);
            }
            o => panic!("{o:?}"),
        }
    }

    #[test]
    fn escape_during_drag_cancels() {
        let mut t = ToolState::Idle;
        t.toggle(ToolKind::Goal);
        t.press([100.0, 100.0], [1.0, 2.0]);
        t.moved([150.0, 100.0]);
        t.cancel();
        assert_eq!(t, ToolState::Idle);
        assert_eq!(t.release([200.0, 100.0]), ToolOutcome::Nothing);
    }

    #[test]
    fn release_outside_map_still_sends() {
        let mut t = ToolState::Idle;
        t.toggle(ToolKind::Goal);
        t.press([10.0, 10.0], [0.5, 0.5]);
        match t.release([-40.0, 10.0]) {
            ToolOutcome::Send(NavCommand::Goal { yaw: Some(yaw), .. }) => {
                assert!((yaw - std::f64::consts::PI).abs() < 1e-12);
            }
            o => panic!("{o:?}"),
        }
    }

    #[test]
    fn unavailable_during_drag_cancels() {
        let mut t = ToolState::Idle;
        t.toggle(ToolKind::Goal);
        t.press([10.0, 10.0], [0.5, 0.5]);
        t.cancel();
        assert_eq!(t.release([100.0, 10.0]), ToolOutcome::Nothing);
        assert_eq!(t.active(), None);
    }

    #[test]
    fn idle_and_armed_release_do_nothing() {
        let mut t = ToolState::Idle;
        t.moved([1.0, 1.0]);
        assert_eq!(t.release([50.0, 50.0]), ToolOutcome::Nothing);
        assert_eq!(t, ToolState::Idle);
        t.toggle(ToolKind::Goal);
        assert_eq!(t.release([50.0, 50.0]), ToolOutcome::Nothing);
        assert_eq!(t, ToolState::Armed(ToolKind::Goal));
        let mut idle = ToolState::Idle;
        idle.press([0.0, 0.0], [0.0, 0.0]);
        assert_eq!(idle, ToolState::Idle);
    }

    #[test]
    fn toggle_same_tool_cancels_other_switches() {
        let mut t = ToolState::Idle;
        t.toggle(ToolKind::Goal);
        assert_eq!(t, ToolState::Armed(ToolKind::Goal));
        t.toggle(ToolKind::Goal);
        assert_eq!(t, ToolState::Idle);
        t.toggle(ToolKind::Goal);
        t.toggle(ToolKind::PoseEstimate);
        assert_eq!(t, ToolState::Armed(ToolKind::PoseEstimate));
    }

    fn status(f: impl FnOnce(&mut NavStatus)) -> NavStatus {
        let mut s: NavStatus = serde_json::from_str(
            r#"{"loc_ok":true,"loc_reason":"","armed":true,"estop":false,"gate_reason":""}"#,
        )
        .expect("status");
        s.cmd_goal_enabled = true;
        f(&mut s);
        s
    }

    const GOOD: Result<(), &str> = Ok(());

    #[test]
    fn availability_table() {
        let ok = status(|_| {});
        let secs = Duration::from_secs;
        let base = |status: Option<&'static NavStatus>| ToolInputs {
            connected: true,
            robots: 1,
            has_token: true,
            status,
            status_age: Some(secs(1)),
        };
        let ok: &'static NavStatus = Box::leak(Box::new(ok));
        let leak = |s: NavStatus| -> &'static NavStatus { Box::leak(Box::new(s)) };

        let both = |r| Availability { pose: Err(r), goal: Err(r) };
        let cases: Vec<(ToolInputs, Availability)> = vec![
            (
                ToolInputs { connected: false, robots: 0, ..base(Some(ok)) },
                both("not connected"),
            ),
            (
                ToolInputs { robots: 0, ..base(Some(ok)) },
                both("no robot seen yet"),
            ),
            (
                ToolInputs { robots: 2, ..base(Some(ok)) },
                both("more than one robot on this connection"),
            ),
            (
                ToolInputs { has_token: false, ..base(Some(ok)) },
                both("no token (~/.config/dviz/cmd_token)"),
            ),
            (
                ToolInputs { status_age: Some(secs(3)), ..base(Some(ok)) },
                both("nav status stale"),
            ),
            (
                ToolInputs { status_age: None, ..base(None) },
                both("nav status stale"),
            ),
            (
                base(Some(leak(status(|s| s.cmd_goal_enabled = false)))),
                both("goal / pose commands not enabled on the robot"),
            ),
            (
                base(Some(leak(status(|s| s.stale = vec!["gate".into()])))),
                Availability { pose: GOOD, goal: Err("gate state unknown") },
            ),
            (
                base(Some(leak(status(|s| s.estop = true)))),
                Availability {
                    pose: GOOD,
                    goal: Err("e-stop latched (release it on the robot)"),
                },
            ),
            (
                base(Some(leak(status(|s| s.armed = false)))),
                Availability {
                    pose: GOOD,
                    goal: Err("robot not armed (arm it on the robot)"),
                },
            ),
            (base(Some(ok)), Availability { pose: GOOD, goal: GOOD }),
        ];
        for (i, (inputs, want)) in cases.iter().enumerate() {
            assert_eq!(&tool_availability(inputs), want, "row {i}");
        }
    }

    #[test]
    fn availability_text_formats() {
        let both = Availability {
            pose: Err("robot not armed (arm it on the robot)"),
            goal: Err("robot not armed (arm it on the robot)"),
        };
        assert_eq!(
            availability_text(&both),
            "Pose / goal tools off: robot not armed (arm it on the robot)"
        );
        let goal = Availability {
            pose: Ok(()),
            goal: Err("robot not armed (arm it on the robot)"),
        };
        assert_eq!(
            availability_text(&goal),
            "2D Goal Pose off: robot not armed (arm it on the robot)"
        );
        assert_eq!(
            availability_text(&Availability { pose: Ok(()), goal: Ok(()) }),
            ""
        );
    }

    fn ack(result: CmdResult, reason: &str) -> CommandOutcome {
        CommandOutcome::Ack(CmdAck {
            id: "s1-aaaa".into(),
            cmd: "goal".into(),
            result,
            reason: reason.into(),
        })
    }

    fn record(replies: Vec<(&str, CommandOutcome)>, sent: Instant) -> CommandRecord {
        CommandRecord {
            id: "s3-k9xq".into(),
            cmd: "goal",
            sent,
            replies: replies.into_iter().map(|(r, o)| (r.to_string(), o)).collect(),
        }
    }

    #[test]
    fn command_text_cases() {
        let t0 = Instant::now();
        let now = t0 + Duration::from_millis(2600);
        let one = |o| command_text(&record(vec![("tracer", o)], t0), now);
        assert_eq!(
            one(CommandOutcome::Pending),
            "Last command: goal s3-k9xq → sending…  (2 s ago)"
        );
        assert_eq!(
            one(ack(CmdResult::Ok, "accepted")),
            "Last command: goal s3-k9xq → ok: accepted  (2 s ago)"
        );
        assert_eq!(
            one(ack(CmdResult::NotArmed, "gate disarmed")),
            "Last command: goal s3-k9xq → not_armed: gate disarmed  (2 s ago)"
        );
        assert_eq!(
            one(CommandOutcome::NotConnected("not connected")),
            "Last command: goal s3-k9xq → not sent: not connected  (2 s ago)"
        );
        assert_eq!(
            one(CommandOutcome::Timeout),
            "Last command: goal s3-k9xq → no reply (robot not reachable)  (2 s ago)"
        );
        let two = record(
            vec![
                ("a", ack(CmdResult::Ok, "done")),
                ("b", CommandOutcome::Timeout),
            ],
            t0,
        );
        assert_eq!(
            command_text(&two, now),
            "Last command: goal s3-k9xq → a: ok: done; b: no reply (robot not reachable)  (2 s ago)"
        );
        // `now` before `sent` must not panic.
        assert!(command_text(&two, t0 - Duration::from_secs(1)).ends_with("(0 s ago)"));
    }

    #[test]
    fn command_error_flag() {
        let t0 = Instant::now();
        let one = |o| command_is_error(&record(vec![("r", o)], t0));
        assert!(!one(CommandOutcome::Pending));
        assert!(!one(ack(CmdResult::Ok, "")));
        assert!(one(ack(CmdResult::NotArmed, "")));
        assert!(one(CommandOutcome::NotConnected("not connected")));
        assert!(one(CommandOutcome::Timeout));
        let mixed = record(
            vec![("a", ack(CmdResult::Ok, "")), ("b", CommandOutcome::Timeout)],
            t0,
        );
        assert!(command_is_error(&mixed));
    }
}
