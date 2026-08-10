//! Hardware setup & calibration wizard.
//!
//! Mirrors `docs/ux/ux-02-hardware.png`. The left panel is where
//! `makepad_urdf_player::RobotView` mounts once a rig profile supplies a URDF;
//! until then it renders an empty viewport so the screen is still verifiable.

use makepad_widgets::*;

use crate::api::{HardwareState, Intent, JointProgress, StepState};
use crate::ui::frame::{apply_light, apply_light_in, theme_panel_head, Themed};

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    use mod.widgets.ux.*

    // Stepper node: ring + number, or a filled check once the step is done.
    let StepDot = View{
        width: Fit height: Fit
        flow: Right
        align: Align{y: 0.5}
        spacing: 9.0

        dot := View{
            width: 26 height: 26
            flow: Overlay
            align: Align{x: 0.5 y: 0.5}
            show_bg: true
            draw_bg +: {
                // 0 pending, 1 active, 2 done
                state: instance(0.0)
                light: instance(0.0)
                pixel: fn() {
                    let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                    let active = step(0.5, self.state) * (1.0 - step(1.5, self.state))
                    let done = step(1.5, self.state)
                    let idle_ring = mix(#x39415A, #xCBD2DE, self.light)
                    let ring = mix(idle_ring, #x4C8BF5, active + done)
                    let base = mix(#x12151C, #xFFFFFF, self.light)
                    let fill = mix(base, #x4C8BF5, active)
                    sdf.circle(13.0, 13.0, 11.5)
                    sdf.fill_keep(fill)
                    sdf.stroke(ring, 1.6)
                    // checkmark only in the done state
                    sdf.move_to(7.5, 13.2)
                    sdf.line_to(11.4, 17.0)
                    sdf.line_to(18.5, 9.4)
                    sdf.stroke(mix(#x00000000, #x58BE8A, done), 2.0)
                    return sdf.result
                }
            }
            num := Label{
                text: "1"
                draw_text +: {
                    state: instance(0.0)
                    light: instance(0.0)
                    text_style: mod.widgets.ux.TEXT_CHIP{}
                    get_color: fn() {
                        let active = step(0.5, self.state) * (1.0 - step(1.5, self.state))
                        let idle = mix(#x8B95AD, #x79839A, self.light)
                        return mix(idle, #xFFFFFF, active)
                    }
                }
            }
        }
        caption := Label{
            text: "Step"
            draw_text +: {
                state: instance(0.0)
                light: instance(0.0)
                text_style: mod.widgets.ux.TEXT_BODY{}
                get_color: fn() {
                    let active = step(0.5, self.state) * (1.0 - step(1.5, self.state))
                    let done = step(1.5, self.state)
                    let pending = mix(#x77819A, #x8992A6, self.light)
                    let fin = mix(#xD7DDEA, #x2A3244, self.light)
                    let acc = mix(#x6BA1F8, #x2159C4, self.light)
                    return mix(mix(pending, fin, done), acc, active)
                }
            }
        }
    }

    let StepLine = View{
        width: Fill height: 1
        margin: Inset{left: 12. right: 12.}
        show_bg: true
        draw_bg +: {
            done: instance(0.0)
            light: instance(0.0)
            pixel: fn() {
                let idle = mix(#x2A3140, #xDCE1EA, self.light)
                return mix(idle, #x4C8BF5, self.done)
            }
        }
    }

    // One calibration row: status pip, index, name, swept-range bar, readout.
    let JointRow = View{
        width: Fill height: 52
        flow: Right
        align: Align{y: 0.5}
        spacing: 12.0
        padding: Inset{left: 14. right: 14.}
        show_bg: true
        draw_bg +: {
            light: instance(0.0)
            pixel: fn() {
                // hairline separator on the bottom edge
                let base = mix(#x161B24, #xFFFFFF, self.light)
                let sep = mix(#x242B3A, #xEAEEF5, self.light)
                let t = step(self.rect_size.y - 1.0, self.pos.y * self.rect_size.y)
                return mix(base, sep, t)
            }
        }

        pip := View{
            width: 22 height: 22
            show_bg: true
            draw_bg +: {
                done: instance(0.0)
                light: instance(0.0)
                pixel: fn() {
                    let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                    let empty = mix(#x12151C, #xFFFFFF, self.light)
                    let ring = mix(mix(#x434C63, #xC7CEDB, self.light), #x58BE8A, self.done)
                    sdf.circle(11.0, 11.0, 9.0)
                    sdf.fill_keep(mix(empty, #x1E6B45, self.done))
                    sdf.stroke(ring, 1.4)
                    sdf.move_to(6.5, 11.2)
                    sdf.line_to(9.7, 14.4)
                    sdf.line_to(15.4, 7.8)
                    sdf.stroke(mix(#x00000000, #xBFF0D6, self.done), 1.8)
                    return sdf.result
                }
            }
        }
        idx := Label{
            text: "1"
            draw_text +: {
                light: instance(0.0)
                text_style: mod.widgets.ux.TEXT_META{}
                get_color: fn() { return mix(#x6C7591, #x99A1B3, self.light) }
            }
        }
        jname := Label{
            width: 132
            text: "joint"
            draw_text +: {
                light: instance(0.0)
                text_style: mod.widgets.ux.TEXT_BODY{}
                get_color: fn() { return mix(#xD7DDEA, #x2A3244, self.light) }
            }
        }
        bar := View{
            width: Fill height: 12
            show_bg: true
            draw_bg +: {
                frac: instance(0.0)
                done: instance(0.0)
                light: instance(0.0)
                pixel: fn() {
                    let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                    sdf.box(0.0, 3.0, self.rect_size.x, 6.0, 3.0)
                    sdf.fill(mix(#x2B3243, #xE4E8F0, self.light))
                    // swept portion, centred like a range sweep
                    let w = self.rect_size.x * self.frac
                    let x0 = (self.rect_size.x - w) * 0.5
                    sdf.box(x0, 3.0, w, 6.0, 3.0)
                    sdf.fill(mix(#x4C8BF5, #x58BE8A, self.done))
                    return sdf.result
                }
            }
        }
        readout := Label{
            width: 152
            text: ""
            draw_text +: {
                warn: instance(0.0)
                light: instance(0.0)
                text_style: mod.widgets.ux.TEXT_META{}
                get_color: fn() {
                    let normal = mix(#xA8B1C4, #x616A7D, self.light)
                    let alert = mix(#xD9A24E, #x8A6414, self.light)
                    return mix(normal, alert, self.warn)
                }
            }
        }
    }

    mod.widgets.HardwareScreenBase = #(HardwareScreen::register_widget(vm))
    mod.widgets.HardwareScreen = set_type_default() do mod.widgets.HardwareScreenBase{
        width: Fill
        height: Fill
        flow: Down
        spacing: 14.0
        padding: Inset{left: 16. right: 16. top: 14. bottom: 16.}
        show_bg: true
        draw_bg +: {
            light: instance(0.0)
            pixel: fn() { return mix(#x12151C, #xF4F6FA, self.light) }
        }

        stepper := View{
            width: Fill height: 44
            flow: Right
            align: Align{y: 0.5}
            padding: Inset{left: 10. right: 10.}
            step_0 := StepDot{}
            line_0 := StepLine{}
            step_1 := StepDot{}
            line_1 := StepLine{}
            step_2 := StepDot{}
            line_2 := StepLine{}
            step_3 := StepDot{}
            line_3 := StepLine{}
            step_4 := StepDot{}
        }

        content := View{
            width: Fill height: Fill
            flow: Right
            spacing: 14.0

            mirror := mod.widgets.ux.Card{
                width: Fill height: Fill
                flow: Down
                mirror_head := mod.widgets.ux.PanelHead{ title +: { text: "live mirror" } }
                // The real renderer, which brings its own studio gradient with
                // it. This was a shader drawing a perspective grid with
                // "RobotView mounts here" written across it.
                viewport := RobotView{ width: Fill height: Fill }
            }

            calib := mod.widgets.ux.Card{
                width: 660 height: Fill
                flow: Down
                calib_head := mod.widgets.ux.PanelHead{ title +: { text: "Joint calibration" } }

                banner := RoundedView{
                    width: Fill height: 84
                    margin: Inset{left: 12. right: 12. top: 12.}
                    padding: Inset{left: 18. right: 18.}
                    flow: Right
                    align: Align{y: 0.5}
                    spacing: 16.0
                    show_bg: true
                    draw_bg +: {
                        light: instance(0.0)
                        border_size: 1.0
                        border_radius: 6.0
                        pixel: fn() {
                            let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                            sdf.box(0.5, 0.5, self.rect_size.x - 1.0, self.rect_size.y - 1.0, 6.0)
                            sdf.fill_keep(mix(#x141B2B, #xF2F6FD, self.light))
                            sdf.stroke(mix(#x27334C, #xD5E0F2, self.light), 1.0)
                            return sdf.result
                        }
                    }
                    ring := View{
                        width: 54 height: 54
                        flow: Overlay
                        align: Align{x: 0.5 y: 0.5}
                        show_bg: true
                        draw_bg +: {
                            frac: instance(0.0)
                            light: instance(0.0)
                            pixel: fn() {
                                let p = self.pos * self.rect_size - vec2(27.0, 27.0)
                                let d = length(p)
                                let band = (1.0 - step(24.0, d)) * step(19.0, d)
                                // angle from 12 o'clock, clockwise, 0..1
                                let a = fract(atan2(p.x, -p.y) / 6.2831853 + 1.0)
                                let on = 1.0 - step(self.frac, a)
                                let track = mix(mix(#x2A3140, #xDCE2EC, self.light), #x58BE8A, on)
                                return mix(#x00000000, track, band)
                            }
                        }
                        ring_text := Label{
                            text: "0/0"
                            draw_text +: {
                                light: instance(0.0)
                                text_style: mod.widgets.ux.TEXT_TITLE{}
                                get_color: fn() { return mix(#xE6EAF2, #x1B2231, self.light) }
                            }
                        }
                    }
                    btext := View{
                        width: Fill height: Fit flow: Down spacing: 5.0
                        instruction := Label{
                            width: Fill
                            text: "Move every joint through its full range"
                            draw_text +: {
                                light: instance(0.0)
                                text_style: mod.widgets.ux.TEXT_TITLE{}
                                get_color: fn() { return mix(#xE6EAF2, #x1B2231, self.light) }
                            }
                        }
                        subline := Label{
                            text: "0/0 joints complete"
                            draw_text +: { color: #x9AA5BC text_style: mod.widgets.ux.TEXT_META{} }
                        }
                    }
                }

                rows := View{
                    width: Fill height: Fit
                    flow: Down
                    margin: Inset{left: 12. right: 12. top: 12.}
                    joint_0 := JointRow{}
                    joint_1 := JointRow{}
                    joint_2 := JointRow{}
                    joint_3 := JointRow{}
                    joint_4 := JointRow{}
                    joint_5 := JointRow{}
                }

                Filler{}

                actions := View{
                    width: Fill height: Fit
                    flow: Right
                    align: Align{y: 0.5}
                    spacing: 12.0
                    padding: Inset{left: 12. right: 12. bottom: 14.}
                    Filler{}
                    btn_restart := Button{
                        text: "Restart step"
                        padding: Inset{left: 20. right: 20. top: 12. bottom: 12.}
                        draw_bg +: {
                            light: instance(0.0)
                            pixel: fn() {
                                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                                sdf.box(0.5, 0.5, self.rect_size.x - 1.0, self.rect_size.y - 1.0, 6.0)
                                let idle = mix(#x161B25, #xFFFFFF, self.light)
                                let hov = mix(#x1E2532, #xF1F4FA, self.light)
                                sdf.fill_keep(mix(idle, hov, self.hover))
                                sdf.stroke(mix(#x333B52, #xD7DCE7, self.light), 1.0)
                                return sdf.result
                            }
                        }
                        draw_text +: {
                            light: instance(0.0)
                            text_style: mod.widgets.ux.TEXT_BODY{}
                            get_color: fn() { return mix(#xC9D2E2, #x333B4D, self.light) }
                        }
                    }
                    btn_continue := Button{
                        text: "Continue"
                        padding: Inset{left: 30. right: 30. top: 12. bottom: 12.}
                        draw_bg +: {
                            enabled_f: instance(0.0)
                            light: instance(0.0)
                            pixel: fn() {
                                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                                let off = mix(#x2A3446, #xE4E8F0, self.light)
                                let live = mix(off, #x3B7BEE, self.enabled_f)
                                sdf.box(0.5, 0.5, self.rect_size.x - 1.0, self.rect_size.y - 1.0, 6.0)
                                sdf.fill(mix(live, #x5C93F7, self.hover * self.enabled_f))
                                return sdf.result
                            }
                        }
                        draw_text +: {
                            enabled_f: instance(0.0)
                            light: instance(0.0)
                            text_style: mod.widgets.ux.TEXT_BODY{}
                            get_color: fn() {
                                let off = mix(#x6C7591, #xA2AAB9, self.light)
                                return mix(off, #xFFFFFF, self.enabled_f)
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct HardwareScreen {
    #[deref]
    view: View,
    /// Model currently mounted in the mirror, so it is opened once.
    #[rust]
    loaded_urdf: Option<(std::path::PathBuf, std::path::PathBuf)>,
}

impl Widget for HardwareScreen {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
}

const STEP_IDS: [&[LiveId]; 5] = [
    ids!(stepper.step_0),
    ids!(stepper.step_1),
    ids!(stepper.step_2),
    ids!(stepper.step_3),
    ids!(stepper.step_4),
];
const LINE_IDS: [&[LiveId]; 4] = [
    ids!(stepper.line_0),
    ids!(stepper.line_1),
    ids!(stepper.line_2),
    ids!(stepper.line_3),
];
const JOINT_IDS: [&[LiveId]; 6] = [
    ids!(content.calib.rows.joint_0),
    ids!(content.calib.rows.joint_1),
    ids!(content.calib.rows.joint_2),
    ids!(content.calib.rows.joint_3),
    ids!(content.calib.rows.joint_4),
    ids!(content.calib.rows.joint_5),
];

impl HardwareScreenRef {
    pub fn sync(&self, cx: &mut Cx, state: &HardwareState) {
        let light = crate::ui::frame::light_mode();
        let Some(mut inner) = self.borrow_mut() else { return };
        let want = state.robot_urdf.clone();
        let need_load = inner.loaded_urdf != want;
        if need_load {
            inner.loaded_urdf = want.clone();
        }
        let root = &mut inner.view;

        // The mirror renders the robot itself, so the model is opened once and
        // then only posed. With no arm attached there are no live angles yet,
        // and it simply sits at its rest pose.
        let viewer = root.widget(cx, ids!(content.mirror.viewport));
        if need_load {
            if let (Some((urdf, assets)), Some(mut rv)) = (
                want.as_ref(),
                viewer.borrow_mut::<makepad_urdf_player::robot_view::RobotView>(),
            ) {
                if let Err(e) = rv.load_robot(&urdf.to_string_lossy(), &assets.to_string_lossy()) {
                    ::log::error!("hardware: urdf {} failed to load: {e}", urdf.display());
                }
            }
        }
        if !state.live_angles.is_empty() {
            if let Some(mut rv) =
                viewer.borrow_mut::<makepad_urdf_player::robot_view::RobotView>()
            {
                rv.set_joint_angles(cx, &state.live_angles);
            }
        }

        script_apply_eval!(cx, root, { draw_bg +: { light: #(light) } });
        apply_light_in(cx, root, &[
            (ids!(content.mirror), Themed::Bg),
            (ids!(content.calib), Themed::Bg),
            (ids!(content.calib.banner), Themed::Bg),
            (ids!(content.calib.banner.ring), Themed::Bg),
            (ids!(content.calib.banner.ring.ring_text), Themed::Text),
            (ids!(content.calib.banner.btext.instruction), Themed::Text),
            (ids!(content.calib.banner.btext.subline), Themed::Text),
            (ids!(content.calib.actions.btn_restart), Themed::Both),
            (ids!(content.calib.actions.btn_continue), Themed::Both),
        ], light);
        for p in [ids!(content.mirror.mirror_head) as &[LiveId], ids!(content.calib.calib_head)] {
            let head = root.widget(cx, p);
            theme_panel_head(cx, &head, light);
        }
        for p in LINE_IDS {
            let mut w = root.widget(cx, p);
            if !w.is_empty() {
                script_apply_eval!(cx, w, { draw_bg +: { light: #(light) } });
            }
        }

        root.label(cx, ids!(content.mirror.mirror_head.title))
            .set_text(cx, &format!("{} — live mirror", state.robot_label));

        // ---- stepper -------------------------------------------------------
        for (i, path) in STEP_IDS.iter().enumerate() {
            let item = root.widget(cx, path);
            if item.is_empty() {
                continue;
            }
            match state.steps.get(i) {
                Some(step) => {
                    item.set_visible(cx, true);
                    apply_light(cx, &item, &[
                        (ids!(dot), Themed::Bg),
                        (ids!(dot.num), Themed::Text),
                        (ids!(caption), Themed::Text),
                    ], light);
                    let s = match step.state {
                        StepState::Pending => 0.0,
                        StepState::Active => 1.0,
                        StepState::Done => 2.0,
                    };
                    item.label(cx, ids!(caption)).set_text(cx, &step.title);
                    item.label(cx, ids!(dot.num))
                        .set_text(cx, &format!("{}", i + 1));
                    // The number is replaced by the checkmark once complete.
                    item.widget(cx, ids!(dot.num))
                        .set_visible(cx, step.state != StepState::Done);

                    let mut dot = item.widget(cx, ids!(dot));
                    script_apply_eval!(cx, dot, { draw_bg +: { state: #(s) } });
                    let mut num = item.widget(cx, ids!(dot.num));
                    script_apply_eval!(cx, num, { draw_text +: { state: #(s) } });
                    let mut cap = item.widget(cx, ids!(caption));
                    script_apply_eval!(cx, cap, { draw_text +: { state: #(s) } });
                }
                None => item.set_visible(cx, false),
            }
        }
        for (i, path) in LINE_IDS.iter().enumerate() {
            let mut connector = root.widget(cx, path);
            if connector.is_empty() {
                continue;
            }
            // A connector lights up once the step to its left is complete.
            let done = state
                .steps
                .get(i)
                .map(|s| s.state == StepState::Done)
                .unwrap_or(false);
            // NB: `v` as an interpolation name collides with script_apply_eval!'s
            // internal Vec<ScriptValue> binding — keep these names distinctive.
            let lit = if done { 1.0 } else { 0.0 };
            script_apply_eval!(cx, connector, { draw_bg +: { done: #(lit) } });
        }

        // ---- progress banner ----------------------------------------------
        let done_n = state.joints_done();
        let total = state.joints.len();
        let frac = if total == 0 { 0.0 } else { done_n as f64 / total as f64 };
        root.label(cx, ids!(content.calib.banner.btext.instruction))
            .set_text(cx, &state.instruction);
        root.label(cx, ids!(content.calib.banner.btext.subline))
            .set_text(cx, &format!("{}/{} joints complete", done_n, total));
        root.label(cx, ids!(content.calib.banner.ring.ring_text))
            .set_text(cx, &format!("{}/{}", done_n, total));
        let mut ring = root.widget(cx, ids!(content.calib.banner.ring));
        script_apply_eval!(cx, ring, { draw_bg +: { frac: #(frac) } });

        // ---- joint rows ----------------------------------------------------
        for (i, path) in JOINT_IDS.iter().enumerate() {
            let row = root.widget(cx, path);
            if row.is_empty() {
                continue;
            }
            match state.joints.get(i) {
                Some(j) => {
                    row.set_visible(cx, true);
                    {
                        let mut r = row.clone();
                        script_apply_eval!(cx, r, { draw_bg +: { light: #(light) } });
                    }
                    apply_light(cx, &row, &[
                        (ids!(pip), Themed::Bg),
                        (ids!(idx), Themed::Text),
                        (ids!(jname), Themed::Text),
                        (ids!(bar), Themed::Bg),
                        (ids!(readout), Themed::Text),
                    ], light);
                    let done = j.progress == JointProgress::Done;
                    row.label(cx, ids!(idx)).set_text(cx, &format!("{}", i + 1));
                    row.label(cx, ids!(jname)).set_text(cx, &j.name);

                    // Completed joints show the learned range; the rest nag.
                    let readout = if done {
                        format!("{:.1}°   {:.1}°", j.min_deg, j.max_deg)
                    } else {
                        j.progress.hint().to_string()
                    };
                    row.label(cx, ids!(readout)).set_text(cx, &readout);

                    let d = if done { 1.0 } else { 0.0 };
                    let frac = j.swept.clamp(0.0, 1.0) as f64;
                    let mut pip = row.widget(cx, ids!(pip));
                    script_apply_eval!(cx, pip, { draw_bg +: { done: #(d) } });
                    let mut bar = row.widget(cx, ids!(bar));
                    script_apply_eval!(cx, bar, { draw_bg +: { frac: #(frac) done: #(d) } });
                    let mut ro = row.widget(cx, ids!(readout));
                    script_apply_eval!(cx, ro, { draw_text +: { warn: #(1.0 - d) } });
                }
                None => row.set_visible(cx, false),
            }
        }

        // ---- continue gate --------------------------------------------------
        let enabled = if state.can_continue() { 1.0 } else { 0.0 };
        let btn = root.button(cx, ids!(content.calib.actions.btn_continue));
        btn.set_enabled(cx, state.can_continue());
        let mut btn_w = root.widget(cx, ids!(content.calib.actions.btn_continue));
        script_apply_eval!(cx, btn_w, {
            draw_bg +: { enabled_f: #(enabled) }
            draw_text +: { enabled_f: #(enabled) }
        });
    }
}

impl HardwareScreenRef {
    /// Wizard buttons, as intents the caller can dispatch. They were drawn but
    /// never read, so pressing them did nothing.
    pub fn wizard_intent(&self, cx: &mut Cx, actions: &Actions) -> Option<Intent> {
        let mut inner = self.borrow_mut()?;
        let v = &mut inner.view;
        if v.button(cx, ids!(content.calib.actions.btn_restart)).clicked(actions) {
            return Some(Intent::WizardRestartStep);
        }
        if v.button(cx, ids!(content.calib.actions.btn_continue)).clicked(actions) {
            return Some(Intent::WizardAdvance);
        }
        None
    }
}
