//! The `dviz_nav` panel: the navigation scene on an RViz-style 2D map, with
//! the nav status line above it and a legend whose check boxes switch each
//! layer.
//!
//! Like `dviz_view`, the panel only holds a handle to the process-wide
//! [`DvizSession`] and reads a fresh snapshot on every `sync` (the host syncs
//! the visible panel about ten times a second). Nothing here waits on the
//! network; connecting is done in the `dviz_view` panel.

use crate::{
    map_view::layer_color, view_panel::theme_due, DvizMap2DWidgetRefExt, DvizSession, NavLayers,
    NavScene,
};
use dorobot_studio_core::{
    api::Backend,
    ui::frame::{self, Themed},
};
use dorobot_studio_plugin::*;
use dviz_protocol::{nav_entities, NavStatus};
use std::time::Duration;

/// A status older than this is stale.
const STALE_AFTER: Duration = Duration::from_secs(2);

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    // The status line: the panel's ink, grey while the status is stale.
    mod.widgets.DvizNavStatus = mod.widgets.DvizLabel{
        draw_text +: {
            stale: instance(0.0)
            get_color: fn() {
                let ink = mix(#xF5F5F7, #x1C1C1E, self.light)   // INK_D, INK_L
                return mix(ink, #x8E8E93, self.stale)            // FAINT
            }
        }
    }
    // A layer's colour in the legend.
    mod.widgets.DvizSwatch = mod.widgets.SolidView{width: 14 height: 10}
}

/// Every part of the panel that draws text or a ground, by path from its
/// root, and which of its draws carry `light`. The Follow button is themed
/// with its selection (`sync_follow_button`), the map through its own
/// `set_light`.
const THEMED: &[(&[LiveId], Themed)] = &[
    (ids!(heading), Themed::Text),
    (ids!(fit), Themed::Both),
    (ids!(status), Themed::Text),
    (ids!(legend), Themed::Bg),
    (ids!(legend_label), Themed::Text),
    (ids!(layer_map), Themed::Text),
    (ids!(layer_footprint), Themed::Text),
    (ids!(layer_plan), Themed::Text),
    (ids!(layer_trail), Themed::Text),
    (ids!(layer_goal), Themed::Text),
    (ids!(layer_scan), Themed::Text),
    (ids!(disclaimer), Themed::Text),
];

/// Each layer's check box and the switch it sets.
type LayerSwitch = fn(&mut NavLayers) -> &mut bool;
const LAYER_CHECKS: &[(&[LiveId], LayerSwitch)] = &[
    (ids!(layer_map), |layers| &mut layers.map),
    (ids!(layer_footprint), |layers| &mut layers.footprint),
    (ids!(layer_plan), |layers| &mut layers.plan),
    (ids!(layer_trail), |layers| &mut layers.trail),
    (ids!(layer_goal), |layers| &mut layers.goal),
    (ids!(layer_scan), |layers| &mut layers.scan),
];

/// Each legend swatch and the entity whose colour it shows.
const SWATCHES: &[(&[LiveId], &str)] = &[
    (ids!(swatch_map), nav_entities::MAP_GRID),
    (ids!(swatch_footprint), nav_entities::ROBOT_FOOTPRINT),
    (ids!(swatch_plan), nav_entities::NAV_PLAN),
    (ids!(swatch_trail), nav_entities::NAV_TRAIL),
    (ids!(swatch_goal), nav_entities::NAV_GOAL),
    (ids!(swatch_scan), nav_entities::SENSORS_SCAN),
];

/// The status line. With a status:
/// "Localization OK · Gate disarmed (disarmed) · Chassis disarmed · Goal C1 ACTIVE · 0.20 m/s",
/// each reason in brackets and missing parts left out; "Stale 3.1 s · " in
/// front once it is older than 2 s. Without one: "Waiting for nav status".
pub fn nav_status_text(status: Option<&NavStatus>, age: Option<Duration>) -> String {
    let Some(status) = status else {
        return "Waiting for nav status".to_string();
    };
    let mut parts = vec![if status.loc_ok {
        "Localization OK".to_string()
    } else {
        with_reason("Localization not trusted".into(), Some(&status.loc_reason))
    }];
    let gate = if status.estop {
        "E-STOP"
    } else if status.armed {
        "armed"
    } else {
        "disarmed"
    };
    parts.push(with_reason(
        format!("Gate {gate}"),
        Some(&status.gate_reason),
    ));
    let chassis = match (&status.chassis_mode, status.chassis_armed) {
        (Some(mode), _) => Some(format!("Chassis {}", flat(mode))),
        (None, Some(true)) => Some("Chassis armed".to_string()),
        (None, Some(false)) => Some("Chassis disarmed".to_string()),
        (None, None) => None,
    };
    if let Some(chassis) = chassis {
        parts.push(with_reason(chassis, status.chassis_reason.as_deref()));
    }
    let goal = match (&status.goal_id, &status.goal_state) {
        (Some(id), Some(state)) => Some(format!("Goal {} {}", flat(id), flat(state))),
        (Some(id), None) => Some(format!("Goal {}", flat(id))),
        (None, Some(state)) => Some(format!("Goal {}", flat(state))),
        (None, None) => None,
    };
    if let Some(goal) = goal {
        parts.push(with_reason(goal, status.goal_reason.as_deref()));
    }
    if let Some(v) = status.v {
        parts.push(format!("{v:.2} m/s"));
    }
    let line = parts.join(" · ");
    match age {
        Some(age) if status_is_stale(Some(age)) => {
            format!("Stale {:.1} s · {line}", age.as_secs_f64())
        }
        _ => line,
    }
}

/// Whether a status of this age is stale: older than 2 s, or never received.
pub fn status_is_stale(age: Option<Duration>) -> bool {
    age.is_none_or(|age| age > STALE_AFTER)
}

/// `text`, then the reason in brackets when there is one.
fn with_reason(text: String, reason: Option<&str>) -> String {
    match reason.map(str::trim) {
        Some(reason) if !reason.is_empty() => format!("{text} ({})", flat(reason)),
        _ => text,
    }
}

/// `text` on one line: control characters (newlines, tabs) as spaces.
fn flat(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

pub struct DvizNavPanel {
    root: WidgetRef,
    session: &'static DvizSession,
    /// The layers switched on in the legend.
    layers: NavLayers,
    /// `(light, reload generation)` the panel was last themed for.
    themed: Option<(f64, u64)>,
    /// `(following, light)` the Follow button was last styled for.
    follow_button: Option<(bool, f64)>,
    /// Whether the status line was last greyed as stale.
    stale: Option<bool>,
    /// The swatch colours last applied, in `SWATCHES` order.
    swatches: Option<Vec<[u8; 4]>>,
}

impl DvizNavPanel {
    pub fn create(vm: &mut ScriptVm, session: &'static DvizSession) -> Self {
        let value = script_eval!(vm, {
            use mod.prelude.widgets.*
            use mod.widgets.*
            View{width: Fill height: Fill flow: Down padding: 16.0 spacing: 12.0
                heading := mod.widgets.DvizLabel{text: "Navigation"
                    draw_text +: {text_style: mod.widgets.ux.TEXT_TITLE{}}}
                View{width: Fill height: Fit flow: Right spacing: 8.0 align: Align{y: 0.5}
                    fit := mod.widgets.ux.ToolButton{text: "Fit map"}
                    follow := mod.widgets.ux.SegmentButton{text: "Follow robot"}
                }
                status := mod.widgets.DvizNavStatus{width: Fill max_lines: 3 text: "Waiting for nav status"}
                View{width: Fill height: Fill flow: Right spacing: 12.0
                    map := mod.widgets.DvizMap2D{}
                    legend := mod.widgets.ux.Card{width: 230 height: Fit flow: Down padding: 12.0 spacing: 8.0
                        legend_label := mod.widgets.DvizLabel{text: "Layers"}
                        View{width: Fill height: Fit flow: Right spacing: 8.0 align: Align{y: 0.5}
                            layer_map := mod.widgets.DvizCheckBox{text: "Map" active: true}
                            swatch_map := mod.widgets.DvizSwatch{draw_bg +: {color: #xCDCDCD}}
                        }
                        View{width: Fill height: Fit flow: Right spacing: 8.0 align: Align{y: 0.5}
                            layer_footprint := mod.widgets.DvizCheckBox{text: "Robot" active: true}
                            swatch_footprint := mod.widgets.DvizSwatch{draw_bg +: {color: #x00C800}}
                        }
                        View{width: Fill height: Fit flow: Right spacing: 8.0 align: Align{y: 0.5}
                            layer_plan := mod.widgets.DvizCheckBox{text: "Plan" active: true}
                            swatch_plan := mod.widgets.DvizSwatch{draw_bg +: {color: #x00FF00}}
                        }
                        View{width: Fill height: Fit flow: Right spacing: 8.0 align: Align{y: 0.5}
                            layer_trail := mod.widgets.DvizCheckBox{text: "Trail" active: true}
                            swatch_trail := mod.widgets.DvizSwatch{draw_bg +: {color: #xFF8C00}}
                        }
                        View{width: Fill height: Fit flow: Right spacing: 8.0 align: Align{y: 0.5}
                            layer_goal := mod.widgets.DvizCheckBox{text: "Goal" active: true}
                            swatch_goal := mod.widgets.DvizSwatch{draw_bg +: {color: #xFF00FF}}
                        }
                        View{width: Fill height: Fit flow: Right spacing: 8.0 align: Align{y: 0.5}
                            layer_scan := mod.widgets.DvizCheckBox{text: "Scan" active: true}
                            swatch_scan := mod.widgets.DvizSwatch{draw_bg +: {color: #xFF0000}}
                        }
                        disclaimer := mod.widgets.DvizLabel{width: Fill
                            text: "Wi-Fi e-stop is not a safety device; switch the remote to RC."}
                    }
                }
            }
        });
        Self {
            root: WidgetRef::script_from_value(vm, value),
            session,
            layers: NavLayers::default(),
            themed: None,
            follow_button: None,
            stale: None,
            swatches: None,
        }
    }

    /// The theme last put on the panel; the current one before the first.
    fn light(&self) -> f64 {
        self.themed
            .map_or_else(frame::light_mode, |(light, _)| light)
    }

    /// Puts the theme on every part. A style reload also put the check
    /// boxes, the Follow button, the status grey and the swatches back to
    /// their source, so those are set again too.
    fn apply_theme(&mut self, cx: &mut Cx, light: f64) {
        frame::apply_light(cx, &self.root, THEMED, light);
        self.root
            .dviz_map2_d(cx, ids!(map))
            .set_light(cx, light as f32);
        self.follow_button = None;
        self.stale = None;
        self.swatches = None;
        for (path, switch) in LAYER_CHECKS {
            let check = self.root.check_box(cx, path);
            let on = *switch(&mut self.layers);
            if check.active(cx) != on {
                check.set_active(cx, on, Animate::No);
            }
        }
    }

    /// Everything from a fresh snapshot.
    fn refresh(&mut self, cx: &mut Cx) {
        let nav = self.session.snapshot().nav;
        let map = self.root.dviz_map2_d(cx, ids!(map));
        map.set_scene(cx, &nav);
        let age = nav.status_at.map(|at| at.elapsed());
        let status = self.root.label(cx, ids!(status));
        status.set_text(cx, &nav_status_text(nav.status.as_ref(), age));
        let stale = status_is_stale(age);
        if self.stale != Some(stale) {
            let mut status = self.root.widget(cx, ids!(status));
            let grey = if stale { 1.0 } else { 0.0 };
            script_apply_eval!(cx, status, { draw_text +: { stale: #(grey) } });
            self.stale = Some(stale);
        }
        self.sync_follow_button(cx, map.follow());
        self.sync_swatches(cx, &nav);
    }

    /// Shows the Follow button selected while the map follows the robot.
    fn sync_follow_button(&mut self, cx: &mut Cx, following: bool) {
        let light = self.light();
        if self.follow_button != Some((following, light)) {
            frame::sync_nav_button(cx, &self.root.widget(cx, ids!(follow)), following, light);
            self.follow_button = Some((following, light));
        }
    }

    /// Paints each swatch in its layer's colour, when that changed.
    fn sync_swatches(&mut self, cx: &mut Cx, nav: &NavScene) {
        let colors: Vec<[u8; 4]> = SWATCHES
            .iter()
            .map(|(_, entity)| layer_color(nav, entity))
            .collect();
        if self.swatches.as_ref() == Some(&colors) {
            return;
        }
        for ((path, _), color) in SWATCHES.iter().zip(&colors) {
            let mut swatch = self.root.widget(cx, path);
            if swatch.is_empty() {
                continue;
            }
            let color = vec4(
                f32::from(color[0]) / 255.0,
                f32::from(color[1]) / 255.0,
                f32::from(color[2]) / 255.0,
                f32::from(color[3]) / 255.0,
            );
            script_apply_eval!(cx, swatch, { draw_bg +: { color: #(color) } });
        }
        self.swatches = Some(colors);
    }
}

impl PluginPanel for DvizNavPanel {
    fn root(&self) -> WidgetRef {
        self.root.clone()
    }

    fn sync(&mut self, cx: &mut Cx, _: &PanelContext<'_>) {
        if let Some(light) = theme_due(cx, &mut self.themed) {
            self.apply_theme(cx, light);
        }
        self.refresh(cx);
    }

    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions, _: &mut dyn Backend) -> bool {
        let map = self.root.dviz_map2_d(cx, ids!(map));
        if self.root.button(cx, ids!(fit)).clicked(actions) {
            map.fit(cx);
        } else if self.root.button(cx, ids!(follow)).clicked(actions) {
            map.set_follow(cx, !map.follow());
        } else if let Some((switch, on)) = LAYER_CHECKS.iter().find_map(|(path, switch)| {
            Some((switch, self.root.check_box(cx, path).changed(actions)?))
        }) {
            *switch(&mut self.layers) = on;
            map.set_layers(cx, self.layers);
        } else {
            return false;
        }
        self.refresh(cx);
        true
    }

    /// Studio calls this whenever the panel is hidden, so it must not
    /// disconnect: the session is shared with `dviz_view` and the 3D window.
    /// The panel's only refresh is `sync`, which stops while it is hidden.
    fn on_disable(&mut self, _: &mut Cx) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example() -> NavStatus {
        NavStatus {
            loc_ok: true,
            armed: false,
            estop: false,
            gate_reason: "disarmed".into(),
            chassis_mode: Some("disarmed".into()),
            goal_id: Some("C1".into()),
            goal_state: Some("ACTIVE".into()),
            v: Some(0.2),
            ..NavStatus::default()
        }
    }

    #[test]
    fn status_text_examples() {
        let status = example();
        let fresh = Some(Duration::from_millis(300));
        assert_eq!(
            nav_status_text(Some(&status), fresh),
            "Localization OK · Gate disarmed (disarmed) · Chassis disarmed · Goal C1 ACTIVE · 0.20 m/s"
        );
        assert_eq!(nav_status_text(None, None), "Waiting for nav status");
        assert_eq!(
            nav_status_text(Some(&status), Some(Duration::from_millis(3100))),
            "Stale 3.1 s · Localization OK · Gate disarmed (disarmed) · Chassis disarmed · Goal C1 ACTIVE · 0.20 m/s"
        );
    }

    #[test]
    fn status_text_shows_reasons_and_leaves_out_what_is_missing() {
        let status = NavStatus {
            loc_ok: false,
            loc_reason: "no_initialpose".into(),
            armed: true,
            estop: true,
            gate_reason: "wifi\nestop".into(),
            chassis_armed: Some(true),
            ..NavStatus::default()
        };
        assert_eq!(
            nav_status_text(Some(&status), Some(Duration::ZERO)),
            "Localization not trusted (no_initialpose) · Gate E-STOP (wifi estop) · Chassis armed"
        );
        let armed = NavStatus {
            loc_ok: true,
            armed: true,
            chassis_mode: Some("CAN".into()),
            chassis_reason: Some("ok".into()),
            goal_id: Some("g7".into()),
            goal_reason: Some("blocked".into()),
            v: Some(-0.054),
            ..NavStatus::default()
        };
        assert_eq!(
            nav_status_text(Some(&armed), Some(Duration::ZERO)),
            "Localization OK · Gate armed · Chassis CAN (ok) · Goal g7 (blocked) · -0.05 m/s"
        );
    }

    #[test]
    fn status_older_than_two_seconds_is_stale() {
        assert!(status_is_stale(Some(Duration::from_millis(2100))));
        assert!(!status_is_stale(Some(Duration::from_millis(1900))));
        assert!(status_is_stale(None));
        // Exactly 2 s is not yet older than 2 s.
        assert!(!status_is_stale(Some(Duration::from_secs(2))));
        let text = nav_status_text(Some(&example()), Some(Duration::from_secs(2)));
        assert!(!text.starts_with("Stale"), "{text}");
    }
}
