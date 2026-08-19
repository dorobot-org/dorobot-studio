//! Recording console — live capture with review-before-save.
//!
//! Mirrors `docs/ux/ux-03-record.png`. Camera panes and joint traces are
//! procedural placeholders here: real pixels arrive when the live dora feed is
//! wired, and the trace shader is seeded per joint so renders stay comparable.

use makepad_widgets::*;

use crate::api::{RecordState, TakeVerdict};
use crate::ui::frame::{apply_light, apply_light_in, theme_chip, theme_panel_head, Themed};

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    use mod.widgets.ux.*

    // Live camera pane: dark stage + LIVE badge. Swapped for a texture-backed
    // Image once the capture pipeline exists.
    let CameraPane = mod.widgets.ux.Card{
        width: Fill height: Fill
        flow: Down
        head := mod.widgets.ux.PanelHead{ title +: { text: "cam" } }
        stage := View{
            width: Fill height: Fill
            flow: Down
            align: Align{x: 0.02 y: 0.93}
            show_bg: true
            draw_bg +: {
                light: instance(0.0)
                pixel: fn() {
                    let p = self.pos
                    // soft vignette so the pane reads as a camera stage
                    let d = length(p - vec2(0.5, 0.45))
                    let core = mix(#x141312, #x1B1917, p.y)
                    let lit = mix(core, #x0A0908, clamp(d * 0.9, 0.0, 1.0))
                    return mix(lit, mix(#x2A2725, #x141312, clamp(d, 0.0, 1.0)), self.light * 0.0)
                }
            }
            badge := RoundedView{
                width: Fit height: Fit
                padding: Inset{left: 8. right: 9. top: 3. bottom: 3.}
                margin: Inset{left: 10. bottom: 10.}
                align: Align{y: 0.5}
                flow: Right
                spacing: 6.0
                show_bg: true
                draw_bg +: {
                    color: #x14181FCC
                    border_radius: 0.0
                }
                dot := View{
                    width: 7 height: 7
                    show_bg: true
                    draw_bg +: {
                        pixel: fn() {
                            let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                            sdf.circle(3.5, 3.5, 3.0)
                            sdf.fill(#xC43B36)
                            return sdf.result
                        }
                    }
                }
                Label{
                    text: "LIVE"
                    draw_text +: { color: #xE8E5E1 text_style: mod.widgets.ux.TEXT_CHIP{} }
                }
            }
        }
    }

    // One joint trace row: name, value, waveform, limits.
    let TraceRow = View{
        width: Fill height: 26
        flow: Right
        align: Align{y: 0.5}
        spacing: 10.0
        padding: Inset{left: 14. right: 14.}
        jn := Label{
            width: 30 text: "J1"
            draw_text +: {
                light: instance(0.0)
                text_style: mod.widgets.ux.TEXT_META{}
                get_color: fn() { return mix(#xD8D4CF, #x7A7169, self.light) }
            }
        }
        jv := Label{
            width: 74 text: "+0.000"
            draw_text +: {
                light: instance(0.0)
                text_style: mod.widgets.ux.TEXT_META{}
                get_color: fn() { return mix(#xD8D4CF, #x2A2725, self.light) }
            }
        }
        wave := View{
            width: Fill height: 18
            show_bg: true
            draw_bg +: {
                seed: instance(0.0)
                light: instance(0.0)
                pixel: fn() {
                    // Procedural trace, seeded per joint so the render is
                    // deterministic and diffable.
                    let x = self.pos.x
                    let s = self.seed
                    let y = 0.5
                        + 0.26 * sin(x * 18.0 + s * 2.2)
                        + 0.10 * sin(x * 47.0 + s * 5.1)
                        + 0.05 * sin(x * 91.0 + s * 1.7)
                    let d = abs(self.pos.y - y)
                    let line = 1.0 - smoothstep(0.0, 0.09, d)
                    let hue0 = mix(#xD15010, #x3E7A4A, fract(s * 0.37))
                    let hue = mix(hue0, #xB07514, fract(s * 0.19))
                    let bg = mix(#x1B1917, #xF2F0ED, self.light)
                    return mix(bg, hue, line)
                }
            }
        }
        jl := Label{
            width: 112 text: ""
            draw_text +: {
                light: instance(0.0)
                text_style: mod.widgets.ux.TEXT_META{}
                get_color: fn() { return mix(#x7A7169, #xD8D4CF, self.light) }
            }
        }
    }

    let FilmCell = View{
        width: 118 height: 74
        margin: Inset{right: 10.}
        show_bg: true
        draw_bg +: {
            light: instance(0.0)
            pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                sdf.box(0.0, 0.0, self.rect_size.x, self.rect_size.y, 0.5)
                sdf.fill(mix(mix(#x2A2725, #x1B1917, self.pos.y),
                             mix(#xF2F0ED, #xE8E5E1, self.pos.y), self.light))
                // sprocket strips top and bottom
                sdf.box(0.0, 0.0, self.rect_size.x, 7.0, 0.0)
                sdf.fill(mix(#x141312, #xD8D4CF, self.light))
                sdf.box(0.0, self.rect_size.y - 7.0, self.rect_size.x, 7.0, 0.0)
                sdf.fill(mix(#x141312, #xD8D4CF, self.light))
                return sdf.result
            }
        }
    }

    mod.widgets.RecordScreenBase = #(RecordScreen::register_widget(vm))
    mod.widgets.RecordScreen = set_type_default() do mod.widgets.RecordScreenBase{
        width: Fill height: Fill
        flow: Down
        spacing: 12.0
        padding: Inset{left: 14. right: 14. top: 12. bottom: 12.}
        show_bg: true
        draw_bg +: {
            light: instance(0.0)
            pixel: fn() { return mix(#x141312, #xFBFAF8, self.light) }
        }

        // ---- session bar ----
        sbar := View{
            width: Fill height: 40
            flow: Right
            align: Align{y: 0.5}
            spacing: 16.0
            padding: Inset{left: 4. right: 6.}
            profile := mod.widgets.ux.Chip{
                draw_bg +: {tone: 0.0}
                label +: { text: "profile" }
            }
            task := Label{
                width: Fill
                text: ""
                draw_text +: {
                    light: instance(0.0)
                    text_style: mod.widgets.ux.TEXT_BODY{}
                    get_color: fn() { return mix(#xD8D4CF, #x2A2725, self.light) }
                }
            }
            counter := Label{
                text: "EP 0 / 0"
                draw_text +: {
                    light: instance(0.0)
                    text_style: mod.widgets.ux.FONT_MEDIUM{font_size: 17.0}
                    get_color: fn() { return mix(#xE8E5E1, #x1B1917, self.light) }
                }
            }
            elapsed := Label{
                text: "00:00.0"
                margin: Inset{left: 14.}
                draw_text +: {
                    light: instance(0.0)
                    text_style: mod.widgets.ux.FONT_MEDIUM{font_size: 17.0}
                    get_color: fn() { return mix(#xD8D4CF, #x7A7169, self.light) }
                }
            }
        }

        // ---- main split ----
        body := View{
            width: Fill height: Fill
            flow: Right
            spacing: 12.0

            left := View{
                width: Fill height: Fill
                flow: Down
                spacing: 12.0
                cams := View{
                    width: Fill height: Fill
                    flow: Right
                    spacing: 12.0
                    cam_0 := CameraPane{}
                    cam_1 := CameraPane{}
                }
                traces := mod.widgets.ux.Card{
                    width: Fill height: 206
                    flow: Down
                    tr_head := mod.widgets.ux.PanelHead{ title +: { text: "Joint state" } }
                    tr_body := View{
                        width: Fill height: Fill flow: Down
                        padding: Inset{top: 6. bottom: 6.}
                        trace_0 := TraceRow{ wave +: { draw_bg +: {seed: 0.0} } }
                        trace_1 := TraceRow{ wave +: { draw_bg +: {seed: 1.0} } }
                        trace_2 := TraceRow{ wave +: { draw_bg +: {seed: 2.0} } }
                        trace_3 := TraceRow{ wave +: { draw_bg +: {seed: 3.0} } }
                        trace_4 := TraceRow{ wave +: { draw_bg +: {seed: 4.0} } }
                        trace_5 := TraceRow{ wave +: { draw_bg +: {seed: 5.0} } }
                    }
                }
            }

            right := View{
                width: 330 height: Fill
                flow: Down
                spacing: 12.0
                view3d := mod.widgets.ux.Card{
                    width: Fill height: Fill
                    flow: Down
                    v3_head := mod.widgets.ux.PanelHead{ title +: { text: "3D View" } }
                    v3_body := View{
                        width: Fill height: Fill
                        align: Align{x: 0.5 y: 0.5}
                        show_bg: true
                        draw_bg +: {
                            light: instance(0.0)
                            pixel: fn() {
                                let p = self.pos * self.rect_size
                                let horizon = self.rect_size.y * 0.42
                                let below = step(horizon, p.y)
                                let ny = (p.y - horizon) / max(self.rect_size.y - horizon, 1.0)
                                let rows = fract(pow(max(ny, 0.0), 0.55) * 7.0)
                                let hline = (1.0 - step(0.05, rows)) * below
                                let dx = (p.x - self.rect_size.x * 0.5) / max(p.y - horizon, 1.0)
                                let vline = (1.0 - step(0.02, fract(dx * 2.5 + 0.5))) * below
                                let g = clamp(hline + vline, 0.0, 1.0) * 0.45
                                let base = mix(#x141312, #xF2F0ED, self.light)
                                return mix(base, mix(#x2A2725, #xD8D4CF, self.light), g)
                            }
                        }
                        Label{
                            text: "RobotView mounts here"
                            draw_text +: {
                                light: instance(0.0)
                                text_style: mod.widgets.ux.TEXT_META{}
                                get_color: fn() { return mix(#x6B625B, #xD8D4CF, self.light) }
                            }
                        }
                    }
                }
                session := mod.widgets.ux.Card{
                    width: Fill height: 168
                    flow: Down
                    ss_head := mod.widgets.ux.PanelHead{ title +: { text: "Session" } }
                    ss_body := View{
                        width: Fill height: Fill flow: Down
                        padding: Inset{left: 14. right: 14. top: 12.}
                        spacing: 10.0
                        prog_row := View{
                            width: Fill height: Fit flow: Right align: Align{y: 0.5}
                            prog_label := Label{
                                width: Fill text: "PROGRESS"
                                draw_text +: {
                                    light: instance(0.0)
                                    text_style: mod.widgets.ux.TEXT_CHIP{}
                                    get_color: fn() { return mix(#x7A7169, #xD8D4CF, self.light) }
                                }
                            }
                            prog_val := Label{
                                text: "0 / 0"
                                draw_text +: {
                                    light: instance(0.0)
                                    text_style: mod.widgets.ux.TEXT_META{}
                                    get_color: fn() { return mix(#xD8D4CF, #x2A2725, self.light) }
                                }
                            }
                        }
                        prog_bar := View{
                            width: Fill height: 8
                            show_bg: true
                            draw_bg +: {
                                frac: instance(0.0)
                                light: instance(0.0)
                                pixel: fn() {
                                    let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                                    sdf.box(0.0, 0.0, self.rect_size.x, 8.0, 4.0)
                                    sdf.fill(mix(#x2A2725, #xE8E5E1, self.light))
                                    sdf.box(0.0, 0.0, self.rect_size.x * self.frac, 8.0, 4.0)
                                    sdf.fill(#xD15010)
                                    return sdf.result
                                }
                            }
                        }
                        tally := Label{
                            text: "Saved 0 · Discarded 0"
                            draw_text +: {
                                light: instance(0.0)
                                text_style: mod.widgets.ux.TEXT_META{}
                                get_color: fn() { return mix(#xD8D4CF, #x7A7169, self.light) }
                            }
                        }
                        cues := Label{
                            text: "SOUND CUES  ON"
                            draw_text +: {
                                light: instance(0.0)
                                text_style: mod.widgets.ux.TEXT_CHIP{}
                                get_color: fn() { return mix(#x7A7169, #xD8D4CF, self.light) }
                            }
                        }
                    }
                }
            }
        }

        // ---- review strip ----
        review := mod.widgets.ux.Card{
            width: Fill height: 132
            flow: Down
            rv_head := mod.widgets.ux.PanelHead{ title +: { text: "Last episode replay" } }
            rv_body := View{
                width: Fill height: Fill
                flow: Right
                align: Align{y: 0.5}
                padding: Inset{left: 14. right: 14.}
                film_0 := FilmCell{}
                film_1 := FilmCell{}
                film_2 := FilmCell{}
                film_3 := FilmCell{}
                Filler{}
                verdict := mod.widgets.ux.Chip{
                    draw_bg +: {tone: 1.0}
                    label +: { text: "ready to save" draw_text +: {tone: 1.0} }
                }
            }
        }

        // ---- transport ----
        transport := View{
            width: Fill height: 78
            flow: Right
            align: Align{y: 0.5}
            spacing: 14.0
            padding: Inset{left: 8. right: 8.}

            btn_discard := Button{
                text: "Discard last"
                padding: Inset{left: 22. right: 22. top: 13. bottom: 13.}
                draw_bg +: {
                    light: instance(0.0)
                    pixel: fn() {
                        let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                        sdf.box(0.5, 0.5, self.rect_size.x - 1.0, self.rect_size.y - 1.0, 0.5)
                        let idle = mix(#x1B1917, #xFFFFFF, self.light)
                        sdf.fill_keep(mix(idle, mix(#x2A2725, #xF2F0ED, self.light), self.hover))
                        sdf.stroke(mix(#x2A2725, #xD8D4CF, self.light), 1.0)
                        return sdf.result
                    }
                }
                draw_text +: {
                    light: instance(0.0)
                    text_style: mod.widgets.ux.TEXT_BODY{}
                    get_color: fn() { return mix(#xD8D4CF, #x2A2725, self.light) }
                }
            }
            btn_rerecord := Button{
                text: "Re-record"
                padding: Inset{left: 22. right: 22. top: 13. bottom: 13.}
                draw_bg +: {
                    light: instance(0.0)
                    pixel: fn() {
                        let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                        sdf.box(0.5, 0.5, self.rect_size.x - 1.0, self.rect_size.y - 1.0, 0.5)
                        let idle = mix(#x1B1917, #xFFFFFF, self.light)
                        sdf.fill_keep(mix(idle, mix(#x2A2725, #xF2F0ED, self.light), self.hover))
                        sdf.stroke(mix(#x2A2725, #xD8D4CF, self.light), 1.0)
                        return sdf.result
                    }
                }
                draw_text +: {
                    light: instance(0.0)
                    text_style: mod.widgets.ux.TEXT_BODY{}
                    get_color: fn() { return mix(#xD8D4CF, #x2A2725, self.light) }
                }
            }
            Filler{}
            btn_record := Button{
                text: ""
                width: 66 height: 66
                draw_bg +: {
                    recording: instance(1.0)
                    pixel: fn() {
                        let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                        let c = self.rect_size.x * 0.5
                        sdf.circle(c, c, c - 2.0)
                        sdf.stroke(#xC43B36, 2.0)
                        sdf.circle(c, c, c - 8.0)
                        sdf.fill(mix(#xC43B36, #xC43B36, self.hover))
                        // stop glyph while recording, solid disc when idle
                        sdf.box(c - 9.0, c - 9.0, 18.0, 18.0, 3.0)
                        sdf.fill(mix(#xE5484D00, #xFFFFFF, self.recording))
                        return sdf.result
                    }
                }
            }
            Filler{}
            btn_save := Button{
                text: "Save episode"
                padding: Inset{left: 34. right: 34. top: 14. bottom: 14.}
                draw_bg +: {
                    pixel: fn() {
                        let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                        sdf.box(0.5, 0.5, self.rect_size.x - 1.0, self.rect_size.y - 1.0, 0.5)
                        sdf.fill(mix(#xD15010, #xD15010, self.hover))
                        return sdf.result
                    }
                }
                draw_text +: {
                    color: #xFFFFFF
                    color_hover: #xFFFFFF
                    color_down: #xFFFFFF
                    text_style: mod.widgets.ux.FONT_MEDIUM{font_size: 13.0}
                }
            }
        }
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct RecordScreen {
    #[deref]
    view: View,
}

impl Widget for RecordScreen {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
}

const TRACE_IDS: [&[LiveId]; 6] = [
    ids!(body.left.traces.tr_body.trace_0),
    ids!(body.left.traces.tr_body.trace_1),
    ids!(body.left.traces.tr_body.trace_2),
    ids!(body.left.traces.tr_body.trace_3),
    ids!(body.left.traces.tr_body.trace_4),
    ids!(body.left.traces.tr_body.trace_5),
];
const CAM_IDS: [&[LiveId]; 2] = [ids!(body.left.cams.cam_0), ids!(body.left.cams.cam_1)];
const FILM_IDS: [&[LiveId]; 4] = [
    ids!(review.rv_body.film_0),
    ids!(review.rv_body.film_1),
    ids!(review.rv_body.film_2),
    ids!(review.rv_body.film_3),
];

impl RecordScreenRef {
    pub fn sync(&self, cx: &mut Cx, st: &RecordState) {
        let light = crate::ui::frame::light_mode();
        let Some(mut inner) = self.borrow_mut() else { return };
        let root = &mut inner.view;

        script_apply_eval!(cx, root, { draw_bg +: { light: #(light) } });
        apply_light_in(cx, root, &[
            (ids!(sbar.task), Themed::Text),
            (ids!(sbar.counter), Themed::Text),
            (ids!(sbar.elapsed), Themed::Text),
            (ids!(body.left.traces), Themed::Bg),
            (ids!(body.right.view3d), Themed::Bg),
            (ids!(body.right.view3d.v3_body), Themed::Bg),
            (ids!(body.right.session), Themed::Bg),
            (ids!(body.right.session.ss_body.prog_row.prog_label), Themed::Text),
            (ids!(body.right.session.ss_body.prog_row.prog_val), Themed::Text),
            (ids!(body.right.session.ss_body.prog_bar), Themed::Bg),
            (ids!(body.right.session.ss_body.tally), Themed::Text),
            (ids!(body.right.session.ss_body.cues), Themed::Text),
            (ids!(review), Themed::Bg),
            (ids!(transport.btn_discard), Themed::Both),
            (ids!(transport.btn_rerecord), Themed::Both),
        ], light);
        for p in [
            ids!(body.left.traces.tr_head) as &[LiveId],
            ids!(body.right.view3d.v3_head),
            ids!(body.right.session.ss_head),
            ids!(review.rv_head),
        ] {
            let head = root.widget(cx, p);
            theme_panel_head(cx, &head, light);
        }
        for p in FILM_IDS {
            let mut f = root.widget(cx, p);
            if !f.is_empty() {
                script_apply_eval!(cx, f, { draw_bg +: { light: #(light) } });
            }
        }

        // ---- session bar --------------------------------------------------
        root.label(cx, ids!(sbar.profile.label)).set_text(cx, &st.profile_label);
        root.label(cx, ids!(sbar.task)).set_text(cx, &format!("“{}”", st.task));
        root.label(cx, ids!(sbar.counter)).set_text(cx, &st.counter_label());
        root.label(cx, ids!(sbar.elapsed)).set_text(cx, &st.elapsed_label());
        let profile_chip = root.widget(cx, ids!(sbar.profile));
        theme_chip(cx, &profile_chip, light);

        // ---- cameras ------------------------------------------------------
        for (i, p) in CAM_IDS.iter().enumerate() {
            let pane = root.widget(cx, p);
            if pane.is_empty() {
                continue;
            }
            match st.cameras.get(i) {
                Some(name) => {
                    pane.set_visible(cx, true);
                    pane.label(cx, ids!(head.title)).set_text(cx, name);
                    let mut c = pane.clone();
                    script_apply_eval!(cx, c, { draw_bg +: { light: #(light) } });
                    let head = pane.widget(cx, ids!(head));
                    theme_panel_head(cx, &head, light);
                }
                None => pane.set_visible(cx, false),
            }
        }

        // ---- joint traces --------------------------------------------------
        for (i, p) in TRACE_IDS.iter().enumerate() {
            let row = root.widget(cx, p);
            if row.is_empty() {
                continue;
            }
            match st.joints.get(i) {
                Some(j) => {
                    row.set_visible(cx, true);
                    row.label(cx, ids!(jn)).set_text(cx, &j.name);
                    row.label(cx, ids!(jv))
                        .set_text(cx, &format!("{:+.3} rad", j.value));
                    row.label(cx, ids!(jl))
                        .set_text(cx, &format!("{:.3}   {:.3}", j.min, j.max));
                    apply_light(cx, &row, &[
                        (ids!(jn), Themed::Text),
                        (ids!(jv), Themed::Text),
                        (ids!(jl), Themed::Text),
                        (ids!(wave), Themed::Bg),
                    ], light);
                }
                None => row.set_visible(cx, false),
            }
        }

        // ---- session panel --------------------------------------------------
        root.label(cx, ids!(body.right.session.ss_body.prog_row.prog_val))
            .set_text(cx, &format!("{} / {}", st.saved, st.episode_target));
        root.label(cx, ids!(body.right.session.ss_body.tally))
            .set_text(cx, &format!("Saved {} · Discarded {}", st.saved, st.discarded));
        root.label(cx, ids!(body.right.session.ss_body.cues)).set_text(
            cx,
            if st.sound_cues { "SOUND CUES  ON" } else { "SOUND CUES  OFF" },
        );
        let frac = st.progress();
        let mut bar = root.widget(cx, ids!(body.right.session.ss_body.prog_bar));
        script_apply_eval!(cx, bar, { draw_bg +: { frac: #(frac) light: #(light) } });

        // ---- review strip ---------------------------------------------------
        let has_take = st.last_take.is_some();
        root.widget(cx, ids!(review)).set_visible(cx, has_take);
        if let Some(take) = &st.last_take {
            let (txt, tone) = match take.verdict {
                TakeVerdict::ReadyToSave => ("ready to save", 1.0),
                TakeVerdict::Warning => ("check warnings", 4.0),
            };
            root.label(cx, ids!(review.rv_body.verdict.label)).set_text(cx, txt);
            let chip = root.widget(cx, ids!(review.rv_body.verdict));
            let mut c = chip.clone();
            script_apply_eval!(cx, c, { draw_bg +: { tone: #(tone) } });
            let mut cl = root.widget(cx, ids!(review.rv_body.verdict.label));
            script_apply_eval!(cx, cl, { draw_text +: { tone: #(tone) } });
            theme_chip(cx, &chip, light);
        }

        // ---- transport ------------------------------------------------------
        let rec = if st.recording { 1.0 } else { 0.0 };
        let mut btn = root.widget(cx, ids!(transport.btn_record));
        script_apply_eval!(cx, btn, { draw_bg +: { recording: #(rec) } });
        root.button(cx, ids!(transport.btn_save)).set_enabled(cx, has_take);
    }
}
