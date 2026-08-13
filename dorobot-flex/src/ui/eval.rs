//! Policy rollout — watch a policy drive the rig and see *where* it diverges.
//!
//! Mirrors `docs/ux/ux-05-eval.png`. The divergence stack is the point: one
//! small chart per joint, with the offending joint shaded rather than the run
//! merely being marked failed.

use makepad_widgets::*;

use crate::api::EvalState;
use crate::ui::frame::{apply_light, apply_light_in, theme_chip, theme_panel_head, Themed};

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    use mod.widgets.ux.*

    // One joint's measured-vs-commanded chart. `alarm` shades the row red and
    // reveals the delta readout.
    let DivergenceRow = View{
        width: Fill height: 52
        flow: Right
        align: Align{y: 0.5}
        spacing: 10.0
        padding: Inset{left: 14. right: 14.}
        jname := Label{
            width: 116 text: "joint"
            draw_text +: {
                alarm: instance(0.0)
                light: instance(0.0)
                text_style: mod.widgets.ux.TEXT_CHIP{}
                get_color: fn() {
                    let normal = mix(#xD8D4CF, #x7A7169, self.light)
                    let hot = mix(#xC43B36, #xE54048, self.light)
                    return mix(normal, hot, self.alarm)
                }
            }
        }
        chart := View{
            width: Fill height: 44
            show_bg: true
            draw_bg +: {
                seed: instance(0.0)
                alarm: instance(0.0)
                light: instance(0.0)
                pixel: fn() {
                    let x = self.pos.x
                    let s = self.seed
                    let bg0 = mix(#x1B1917, #xFBFAF8, self.light)
                    let hot = mix(#x1B1917, #xF2F0ED, self.light)
                    let bg = mix(bg0, hot, self.alarm)
                    // centre line
                    let mid = 1.0 - smoothstep(0.0, 0.01, abs(self.pos.y - 0.5))
                    let base = mix(bg, mix(#x2A2725, #xF2F0ED, self.light), mid * 0.8)
                    // measured (solid blue) and commanded (dashed amber)
                    let ym = 0.5 + 0.26 * sin(x * 6.0 + s) + 0.08 * sin(x * 17.0 + s * 2.0)
                    // the alarmed joint pulls away in the last third
                    let ya = ym + 0.03 + self.alarm * clamp((x - 0.55) * 0.62, 0.0, 1.0)
                    let lm = 1.0 - smoothstep(0.0, 0.02, abs(self.pos.y - ym))
                    let dash = step(0.5, fract(x * 44.0))
                    let la = (1.0 - smoothstep(0.0, 0.02, abs(self.pos.y - ya))) * dash
                    let c1 = mix(base, #xD15010, lm)
                    return mix(c1, #xB07514, la)
                }
            }
        }
        delta := Label{
            width: 74 text: ""
            draw_text +: {
                alarm: instance(0.0)
                light: instance(0.0)
                text_style: mod.widgets.ux.TEXT_META{}
                get_color: fn() {
                    let normal = mix(#x7A7169, #xD8D4CF, self.light)
                    let hot = mix(#xC43B36, #xE54048, self.light)
                    return mix(normal, hot, self.alarm)
                }
            }
        }
    }

    let RunRow = View{
        width: Fill height: 40
        flow: Right
        align: Align{y: 0.5}
        spacing: 10.0
        padding: Inset{left: 14. right: 14.}
        show_bg: true
        draw_bg +: {
            light: instance(0.0)
            pixel: fn() {
                let t = step(self.rect_size.y - 1.0, self.pos.y * self.rect_size.y)
                return mix(mix(#x1B1917, #xFFFFFF, self.light),
                           mix(#x2A2725, #xF2F0ED, self.light), t)
            }
        }
        run_id := Label{
            width: 74 text: "run 00"
            draw_text +: {
                light: instance(0.0)
                text_style: mod.widgets.ux.TEXT_META{}
                get_color: fn() { return mix(#xD8D4CF, #x2A2725, self.light) }
            }
        }
        run_state := mod.widgets.ux.Chip{
            draw_bg +: {tone: 1.0}
            label +: { text: "success" draw_text +: {tone: 1.0} }
        }
        Filler{}
        run_dur := Label{
            text: "0.0s"
            draw_text +: {
                light: instance(0.0)
                text_style: mod.widgets.ux.TEXT_META{}
                get_color: fn() { return mix(#xD8D4CF, #x7A7169, self.light) }
            }
        }
    }

    mod.widgets.EvalScreenBase = #(EvalScreen::register_widget(vm))
    mod.widgets.EvalScreen = set_type_default() do mod.widgets.EvalScreenBase{
        width: Fill height: Fill
        flow: Down
        spacing: 12.0
        padding: Inset{left: 14. right: 14. top: 12. bottom: 12.}
        show_bg: true
        draw_bg +: {
            light: instance(0.0)
            pixel: fn() { return mix(#x141312, #xFBFAF8, self.light) }
        }

        // ---- policy bar ----
        pbar := View{
            width: Fill height: 42
            flow: Right
            align: Align{y: 0.5}
            spacing: 14.0
            padding: Inset{left: 4. right: 4.}
            ckpt := mod.widgets.ux.Chip{
                draw_bg +: {tone: 0.0}
                label +: { text: "checkpoint" }
            }
            status := mod.widgets.ux.Chip{
                draw_bg +: {tone: 1.0}
                label +: { text: "policy driving" draw_text +: {tone: 1.0} }
            }
            latency := Label{
                text: "inference — ms"
                draw_text +: {
                    light: instance(0.0)
                    text_style: mod.widgets.ux.TEXT_META{}
                    get_color: fn() { return mix(#xD8D4CF, #x7A7169, self.light) }
                }
            }
            Filler{}
            btn_stop := Button{
                text: "STOP ROLLOUT"
                padding: Inset{left: 22. right: 22. top: 11. bottom: 11.}
                draw_bg +: {
                    light: instance(0.0)
                    pixel: fn() {
                        let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                        sdf.box(0.5, 0.5, self.rect_size.x - 1.0, self.rect_size.y - 1.0, 0.5)
                        let idle = mix(#x1B1917, #xF2F0ED, self.light)
                        sdf.fill_keep(mix(idle, mix(#x2A2725, #xE8E5E1, self.light), self.hover))
                        sdf.stroke(#xC43B36, 1.2)
                        return sdf.result
                    }
                }
                draw_text +: {
                    color: #xC43B36
                    color_hover: #xC43B36
                    color_down: #xC43B36
                    text_style: mod.widgets.ux.TEXT_CHIP{}
                }
            }
        }

        body := View{
            width: Fill height: Fill
            flow: Right
            spacing: 12.0

            left := View{
                width: Fill height: Fill
                flow: Down
                spacing: 12.0
                cams := View{
                    width: Fill height: 232
                    flow: Right
                    spacing: 12.0
                    cam_a := mod.widgets.ux.Card{
                        width: Fill height: Fill flow: Down
                        ha := mod.widgets.ux.PanelHead{ title +: { text: "cam_high" } }
                        sa := View{
                            width: Fill height: Fill show_bg: true
                            draw_bg +: {
                                light: instance(0.0)
                                pixel: fn() {
                                    let d = length(self.pos - vec2(0.5, 0.45))
                                    return mix(mix(#x1B1917, #x0A0908, clamp(d, 0.0, 1.0)),
                                               mix(#xF2F0ED, #xE8E5E1, clamp(d, 0.0, 1.0)), self.light)
                                }
                            }
                        }
                    }
                    cam_b := mod.widgets.ux.Card{
                        width: Fill height: Fill flow: Down
                        hb := mod.widgets.ux.PanelHead{ title +: { text: "cam_wrist" } }
                        sb := View{
                            width: Fill height: Fill show_bg: true
                            draw_bg +: {
                                light: instance(0.0)
                                pixel: fn() {
                                    let d = length(self.pos - vec2(0.5, 0.5))
                                    return mix(mix(#x1B1917, #x0A0908, clamp(d, 0.0, 1.0)),
                                               mix(#xF2F0ED, #xE8E5E1, clamp(d, 0.0, 1.0)), self.light)
                                }
                            }
                        }
                    }
                }
                stack := mod.widgets.ux.Card{
                    width: Fill height: Fill
                    flow: Down
                    st_head := mod.widgets.ux.PanelHead{ title +: { text: "action vs state — per joint" } }
                    st_body := View{
                        width: Fill height: Fill flow: Down
                        padding: Inset{top: 4. bottom: 4.}
                        div_0 := DivergenceRow{ chart +: { draw_bg +: {seed: 0.0} } }
                        div_1 := DivergenceRow{ chart +: { draw_bg +: {seed: 1.0} } }
                        div_2 := DivergenceRow{ chart +: { draw_bg +: {seed: 2.0} } }
                        div_3 := DivergenceRow{ chart +: { draw_bg +: {seed: 3.0} } }
                        div_4 := DivergenceRow{ chart +: { draw_bg +: {seed: 4.0} } }
                        div_5 := DivergenceRow{ chart +: { draw_bg +: {seed: 5.0} } }
                    }
                }
            }

            right := View{
                width: 336 height: Fill
                flow: Down
                spacing: 12.0
                view3d := mod.widgets.ux.Card{
                    width: Fill height: Fill
                    flow: Down
                    v_head := mod.widgets.ux.PanelHead{ title +: { text: "3D View" } }
                    v_body := View{
                        width: Fill height: Fill
                        show_bg: true
                        draw_bg +: {
                            light: instance(0.0)
                            pixel: fn() {
                                let p = self.pos * self.rect_size
                                let horizon = self.rect_size.y * 0.40
                                let below = step(horizon, p.y)
                                let ny = (p.y - horizon) / max(self.rect_size.y - horizon, 1.0)
                                let hline = (1.0 - step(0.05, fract(pow(max(ny, 0.0), 0.55) * 6.0))) * below
                                let dx = (p.x - self.rect_size.x * 0.5) / max(p.y - horizon, 1.0)
                                let vline = (1.0 - step(0.02, fract(dx * 2.5 + 0.5))) * below
                                let g = clamp(hline + vline, 0.0, 1.0) * 0.4
                                let base = mix(#x141312, #xF2F0ED, self.light)
                                let stage = mix(base, mix(#x2A2725, #xD8D4CF, self.light), g)
                                // predicted end-effector ribbon
                                let t = clamp((self.pos.x - 0.22) / 0.56, 0.0, 1.0)
                                let ty = 0.36 + 0.34 * t * t
                                let ribbon = 1.0 - smoothstep(0.0, 0.030, abs(self.pos.y - ty))
                                let inband = step(0.22, self.pos.x) * (1.0 - step(0.80, self.pos.x))
                                return mix(stage, #xD15010, ribbon * inband * 0.55)
                            }
                        }
                    }
                }
                runs := mod.widgets.ux.Card{
                    width: Fill height: 236
                    flow: Down
                    r_head := mod.widgets.ux.PanelHead{ title +: { text: "Rollouts" } }
                    r_body := View{
                        width: Fill height: Fill flow: Down
                        run_0 := RunRow{}
                        run_1 := RunRow{}
                        run_2 := RunRow{}
                        Filler{}
                        summary := Label{
                            width: Fill
                            text: "0/0 success · 0%"
                            margin: Inset{bottom: 12.}
                            draw_text +: {
                                light: instance(0.0)
                                text_style: mod.widgets.ux.FONT_MEDIUM{font_size: 13.0}
                                get_color: fn() { return mix(#x3E7A4A, #x6FAB78, self.light) }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct EvalScreen {
    #[deref]
    view: View,
}

impl Widget for EvalScreen {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
}

const DIV_IDS: [&[LiveId]; 6] = [
    ids!(body.left.stack.st_body.div_0), ids!(body.left.stack.st_body.div_1),
    ids!(body.left.stack.st_body.div_2), ids!(body.left.stack.st_body.div_3),
    ids!(body.left.stack.st_body.div_4), ids!(body.left.stack.st_body.div_5),
];
const RUN_IDS: [&[LiveId]; 3] = [
    ids!(body.right.runs.r_body.run_0),
    ids!(body.right.runs.r_body.run_1),
    ids!(body.right.runs.r_body.run_2),
];

impl EvalScreenRef {
    pub fn sync(&self, cx: &mut Cx, st: &EvalState) {
        let light = crate::ui::frame::light_mode();
        let Some(mut inner) = self.borrow_mut() else { return };
        let root = &mut inner.view;

        script_apply_eval!(cx, root, { draw_bg +: { light: #(light) } });
        apply_light_in(cx, root, &[
            (ids!(pbar.latency), Themed::Text),
            (ids!(pbar.btn_stop), Themed::Bg),
            (ids!(body.left.cams.cam_a), Themed::Bg),
            (ids!(body.left.cams.cam_a.sa), Themed::Bg),
            (ids!(body.left.cams.cam_b), Themed::Bg),
            (ids!(body.left.cams.cam_b.sb), Themed::Bg),
            (ids!(body.left.stack), Themed::Bg),
            (ids!(body.right.view3d), Themed::Bg),
            (ids!(body.right.view3d.v_body), Themed::Bg),
            (ids!(body.right.runs), Themed::Bg),
            (ids!(body.right.runs.r_body.summary), Themed::Text),
        ], light);
        for p in [
            ids!(body.left.cams.cam_a.ha) as &[LiveId],
            ids!(body.left.cams.cam_b.hb),
            ids!(body.left.stack.st_head),
            ids!(body.right.view3d.v_head),
            ids!(body.right.runs.r_head),
        ] {
            let head = root.widget(cx, p);
            theme_panel_head(cx, &head, light);
        }

        // ---- policy bar ----------------------------------------------------
        root.label(cx, ids!(pbar.ckpt.label)).set_text(cx, &st.checkpoint);
        root.label(cx, ids!(pbar.status.label))
            .set_text(cx, if st.driving { "policy driving" } else { "stopped" });
        root.label(cx, ids!(pbar.latency))
            .set_text(cx, &format!("inference {:.0} ms", st.inference_ms));
        let tone = if st.driving { 1.0 } else { 0.0 };
        let mut sw = root.widget(cx, ids!(pbar.status));
        script_apply_eval!(cx, sw, { draw_bg +: { tone: #(tone) } });
        let mut sl = root.widget(cx, ids!(pbar.status.label));
        script_apply_eval!(cx, sl, { draw_text +: { tone: #(tone) } });
        for p in [ids!(pbar.ckpt) as &[LiveId], ids!(pbar.status)] {
            let chip = root.widget(cx, p);
            theme_chip(cx, &chip, light);
        }

        // ---- divergence stack ----------------------------------------------
        for (i, p) in DIV_IDS.iter().enumerate() {
            let row = root.widget(cx, p);
            if row.is_empty() {
                continue;
            }
            match st.joints.get(i) {
                Some(j) => {
                    row.set_visible(cx, true);
                    row.label(cx, ids!(jname)).set_text(cx, &j.name.to_uppercase());
                    let alarm = if j.alarm { 1.0 } else { 0.0 };
                    // Only the offending joint shows its delta, so attention
                    // lands on the joint rather than the whole run.
                    let delta_txt = if j.alarm {
                        format!("Δ {:.1}°", j.delta_deg)
                    } else {
                        String::new()
                    };
                    row.label(cx, ids!(delta)).set_text(cx, &delta_txt);
                    let mut nm = row.widget(cx, ids!(jname));
                    script_apply_eval!(cx, nm, { draw_text +: { alarm: #(alarm) light: #(light) } });
                    let mut ch = row.widget(cx, ids!(chart));
                    script_apply_eval!(cx, ch, { draw_bg +: { alarm: #(alarm) light: #(light) } });
                    let mut dl = row.widget(cx, ids!(delta));
                    script_apply_eval!(cx, dl, { draw_text +: { alarm: #(alarm) light: #(light) } });
                }
                None => row.set_visible(cx, false),
            }
        }

        // ---- rollout ledger -------------------------------------------------
        for (i, p) in RUN_IDS.iter().enumerate() {
            let row = root.widget(cx, p);
            if row.is_empty() {
                continue;
            }
            match st.runs.get(i) {
                Some(r) => {
                    row.set_visible(cx, true);
                    row.label(cx, ids!(run_id)).set_text(cx, &format!("run {:02}", r.id));
                    row.label(cx, ids!(run_state.label))
                        .set_text(cx, if r.success { "success" } else { "fail" });
                    row.label(cx, ids!(run_dur)).set_text(
                        cx,
                        &match (&r.note, r.success) {
                            (Some(n), _) => n.clone(),
                            (None, _) => format!("{:.1}s", r.duration_s),
                        },
                    );
                    let tone = if r.success { 1.0 } else { 2.0 };
                    let mut rw = row.clone();
                    script_apply_eval!(cx, rw, { draw_bg +: { light: #(light) } });
                    let mut cw = row.widget(cx, ids!(run_state));
                    script_apply_eval!(cx, cw, { draw_bg +: { tone: #(tone) } });
                    let mut cl = row.widget(cx, ids!(run_state.label));
                    script_apply_eval!(cx, cl, { draw_text +: { tone: #(tone) } });
                    apply_light(cx, &row, &[
                        (ids!(run_id), Themed::Text),
                        (ids!(run_dur), Themed::Text),
                    ], light);
                    let chip = row.widget(cx, ids!(run_state));
                    theme_chip(cx, &chip, light);
                }
                None => row.set_visible(cx, false),
            }
        }
        root.label(cx, ids!(body.right.runs.r_body.summary))
            .set_text(cx, &st.success_label());
    }
}
