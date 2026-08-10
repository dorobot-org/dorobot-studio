//! Episode player & curation.
//!
//! Mirrors `docs/ux/ux-04-player.png`. Adds the two things the shipping player
//! lacks: curation actions, and an honest timeline that shows video/parquet
//! drift instead of assuming `frame / fps`.

use makepad_widgets::*;

use crate::api::{Intent, PlaybackState, Tag};
use makepad_app_shell::grid::panel_grid::PanelGridWidgetExt;
use makepad_app_shell::grid::LayoutState;
use crate::ui::frame::{apply_light, apply_light_in, theme_chip, theme_panel_head, Themed};

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    use mod.widgets.ux.*

    // Episode row in the tree: index, duration, quality tag.
    let EpisodeRow = View{
        width: Fill height: 26
        flow: Right
        align: Align{y: 0.5}
        spacing: 8.0
        padding: Inset{left: 18. right: 10.}
        // A dev View only hit-tests when it declares a cursor; without this the
        // row renders but never emits a ViewAction.
        cursor: MouseCursor.Hand
        show_bg: true
        draw_bg +: {
            sel: instance(0.0)
            light: instance(0.0)
            pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                let base = mix(#x1A1F2900, #xFFFFFF00, self.light)
                let on = mix(#x24365A, #xE3ECFB, self.light)
                sdf.box(4.0, 1.0, self.rect_size.x - 8.0, self.rect_size.y - 2.0, 4.0)
                sdf.fill(mix(base, on, self.sel))
                return sdf.result
            }
        }
        ep_name := Label{
            width: 150 text: "ep 000"
            draw_text +: {
                sel: instance(0.0)
                light: instance(0.0)
                text_style: mod.widgets.ux.TEXT_META{}
                get_color: fn() {
                    let idle = mix(#xA8B1C4, #x4A5266, self.light)
                    let on = mix(#xE6EAF2, #x14306B, self.light)
                    return mix(idle, on, self.sel)
                }
            }
        }
        Filler{}
        ep_tag := mod.widgets.ux.Chip{
            draw_bg +: {tone: 1.0}
            label +: { text: "good" draw_text +: {tone: 1.0} }
        }
    }

    let GroupHead = Label{
        text: "group"
        margin: Inset{left: 12. top: 10. bottom: 3.}
        draw_text +: {
            light: instance(0.0)
            text_style: mod.widgets.ux.TEXT_CHIP{}
            get_color: fn() { return mix(#x77819A, #x8992A6, self.light) }
        }
    }

    let StatRow = View{
        width: Fill height: 24
        flow: Right
        align: Align{y: 0.5}
        k := Label{
            width: Fill text: "KEY"
            draw_text +: {
                light: instance(0.0)
                text_style: mod.widgets.ux.TEXT_CHIP{}
                get_color: fn() { return mix(#x77819A, #x8992A6, self.light) }
            }
        }
        v := Label{
            text: "-"
            draw_text +: {
                warn: instance(0.0)
                light: instance(0.0)
                text_style: mod.widgets.ux.TEXT_META{}
                get_color: fn() {
                    let normal = mix(#xD7DDEA, #x2A3244, self.light)
                    let alert = mix(#xD9A24E, #x8A6414, self.light)
                    return mix(normal, alert, self.warn)
                }
            }
        }
    }

    let ActionBtn = Button{
        width: Fill
        padding: Inset{left: 14. right: 14. top: 11. bottom: 11.}
        draw_bg +: {
            tone: instance(0.0)
            light: instance(0.0)
            pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                // 0 neutral, 1 good, 2 bad, 3 primary
                let d_ok = mix(#x161B25, #x14301F, step(0.5, self.tone))
                let d_bad = mix(d_ok, #x33191B, step(1.5, self.tone))
                let dark_f = mix(d_bad, #x3B7BEE, step(2.5, self.tone))
                let l_ok = mix(#xFFFFFF, #xE9F6EF, step(0.5, self.tone))
                let l_bad = mix(l_ok, #xFCEDEC, step(1.5, self.tone))
                let light_f = mix(l_bad, #x3B7BEE, step(2.5, self.tone))
                let fill = mix(dark_f, light_f, self.light)
                let d_e = mix(#x333B52, #x2C6B48, step(0.5, self.tone))
                let d_e2 = mix(d_e, #x7A2F32, step(1.5, self.tone))
                let dark_e = mix(d_e2, #x3B7BEE, step(2.5, self.tone))
                let l_e = mix(#xD7DCE7, #xB6DEC7, step(0.5, self.tone))
                let l_e2 = mix(l_e, #xF0BFBD, step(1.5, self.tone))
                let light_e = mix(l_e2, #x3B7BEE, step(2.5, self.tone))
                sdf.box(0.5, 0.5, self.rect_size.x - 1.0, self.rect_size.y - 1.0, 6.0)
                sdf.fill_keep(mix(fill, fill * 1.12, self.hover))
                sdf.stroke(mix(dark_e, light_e, self.light), 1.0)
                return sdf.result
            }
        }
        draw_text +: {
            tone: instance(0.0)
            light: instance(0.0)
            text_style: mod.widgets.ux.TEXT_BODY{}
            get_color: fn() {
                let d_ok = mix(#xC9D2E2, #x58BE8A, step(0.5, self.tone))
                let d_bad = mix(d_ok, #xE5484D, step(1.5, self.tone))
                let dark_c = mix(d_bad, #xFFFFFF, step(2.5, self.tone))
                let l_ok = mix(#x333B4D, #x1F7A4C, step(0.5, self.tone))
                let l_bad = mix(l_ok, #xB3312F, step(1.5, self.tone))
                let light_c = mix(l_bad, #xFFFFFF, step(2.5, self.tone))
                return mix(dark_c, light_c, self.light)
            }
        }
    }

    // Panel *contents* only — no title bar, no border: the app-shell Panel
    // wraps these and provides drag handle, maximise and close.
    let StagePane = View{
        width: Fill height: Fill
        show_bg: true
        draw_bg +: {
            light: instance(0.0)
            pixel: fn() {
                let d = length(self.pos - vec2(0.5, 0.45))
                return mix(mix(#x141A24, #x090C12, clamp(d, 0.0, 1.0)),
                           mix(#xE9EDF4, #xD9DFE9, clamp(d, 0.0, 1.0)), self.light)
            }
        }
    }

    let ViewportPane = View{
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
                let hline = (1.0 - step(0.05, fract(pow(max(ny, 0.0), 0.55) * 6.0))) * below
                let dx = (p.x - self.rect_size.x * 0.5) / max(p.y - horizon, 1.0)
                let vline = (1.0 - step(0.02, fract(dx * 2.5 + 0.5))) * below
                let g = clamp(hline + vline, 0.0, 1.0) * 0.45
                let base = mix(#x0C0F16, #xEDF0F6, self.light)
                return mix(base, mix(#x38425C, #xC6CDDA, self.light), g)
            }
        }
    }

    let PlotPane = View{
        width: Fill height: Fill
        show_bg: true
        draw_bg +: {
            light: instance(0.0)
            pixel: fn() {
                let x = self.pos.x
                let bg = mix(#x141922, #xFAFBFD, self.light)
                let gy = 1.0 - step(0.012, fract(self.pos.y * 4.0))
                let grid = mix(bg, mix(#x232B3A, #xE8ECF3, self.light), gy)
                let ys = 0.5 + 0.30 * sin(x * 7.4) + 0.09 * sin(x * 19.0)
                let ya = ys + 0.05 + 0.03 * sin(x * 11.0)
                let ls = 1.0 - smoothstep(0.0, 0.012, abs(self.pos.y - ys))
                let dash = step(0.5, fract(x * 60.0))
                let la = (1.0 - smoothstep(0.0, 0.012, abs(self.pos.y - ya))) * dash
                let c1 = mix(grid, #x5B9BF0, ls)
                return mix(c1, #xD9A24E, la)
            }
        }
    }

    mod.widgets.PlayScreenBase = #(PlayScreen::register_widget(vm))
    mod.widgets.PlayScreen = set_type_default() do mod.widgets.PlayScreenBase{
        width: Fill height: Fill
        flow: Down
        spacing: 12.0
        padding: Inset{left: 14. right: 14. top: 12. bottom: 12.}
        show_bg: true
        draw_bg +: {
            light: instance(0.0)
            pixel: fn() { return mix(#x12151C, #xF4F6FA, self.light) }
        }

        upper := View{
            width: Fill height: Fill
            flow: Right
            spacing: 12.0

            // ---- episode tree ----
            tree := mod.widgets.ux.Card{
                width: 268 height: Fill
                flow: Down
                tree_head := mod.widgets.ux.PanelHead{ title +: { text: "dataset" } }
                tree_body := View{
                    width: Fill height: Fill flow: Down
                    padding: Inset{bottom: 8.}
                    grp_0 := GroupHead{}
                    row_0 := EpisodeRow{}
                    row_1 := EpisodeRow{}
                    row_2 := EpisodeRow{}
                    row_3 := EpisodeRow{}
                    row_4 := EpisodeRow{}
                    row_5 := EpisodeRow{}
                    grp_1 := GroupHead{}
                    row_6 := EpisodeRow{}
                    row_7 := EpisodeRow{}
                    row_8 := EpisodeRow{}
                    row_9 := EpisodeRow{}
                    grp_2 := GroupHead{}
                    row_10 := EpisodeRow{}
                    row_11 := EpisodeRow{}
                    row_12 := EpisodeRow{}
                    Filler{}
                }
            }

            // ---- real drag-and-drop panel grid (makepad-app-shell) ----
            // Panels here are draggable, maximisable and closable for real;
            // the grid owns layout state and emits PanelAction::LayoutChanged.
            grid := PanelGrid{
                width: Fill height: Fill
                window_container +: {
                    row1 +: {
                        s1_1 +: {
                            title: "cam_high"
                            content +: { stage_a := StagePane{} }
                        }
                        s1_2 +: {
                            title: "3D View"
                            content +: { stage_3d := ViewportPane{} }
                        }
                        s1_3 +: { visible: false }
                        s1_4 +: { visible: false }
                        s1_5 +: { visible: false }
                        s1_6 +: { visible: false }
                        s1_7 +: { visible: false }
                        s1_8 +: { visible: false }
                        s1_9 +: { visible: false }
                    }
                    row2 +: {
                        s2_1 +: {
                            title: "cam_wrist"
                            content +: { stage_b := StagePane{} }
                        }
                        s2_2 +: {
                            title: "action vs state"
                            content +: { stage_plot := PlotPane{} }
                        }
                        s2_3 +: { visible: false }
                        s2_4 +: { visible: false }
                        s2_5 +: { visible: false }
                        s2_6 +: { visible: false }
                        s2_7 +: { visible: false }
                        s2_8 +: { visible: false }
                        s2_9 +: { visible: false }
                    }
                    row3 +: { visible: false height: 0 }
                }
            }

            // ---- inspector + curation ----
            side := mod.widgets.ux.Card{
                width: 288 height: Fill
                flow: Down
                side_head := mod.widgets.ux.PanelHead{ title +: { text: "Episode" } }
                side_body := View{
                    width: Fill height: Fill flow: Down
                    padding: Inset{left: 14. right: 14. top: 10.}
                    spacing: 2.0
                    st_frames := StatRow{ k +: { text: "FRAMES" } }
                    st_dur := StatRow{ k +: { text: "DURATION" } }
                    st_fps := StatRow{ k +: { text: "FPS" } }
                    st_drift := StatRow{ k +: { text: "DRIFT" } }
                    task_h := Label{
                        text: "TASK"
                        margin: Inset{top: 14. bottom: 4.}
                        draw_text +: {
                            light: instance(0.0)
                            text_style: mod.widgets.ux.TEXT_CHIP{}
                            get_color: fn() { return mix(#x77819A, #x8992A6, self.light) }
                        }
                    }
                    task_t := Label{
                        width: Fill
                        text: ""
                        draw_text +: {
                            light: instance(0.0)
                            text_style: mod.widgets.ux.TEXT_BODY{}
                            get_color: fn() { return mix(#xC7CEDD, #x39415A, self.light) }
                        }
                    }
                    cur_h := Label{
                        text: "CURATION"
                        margin: Inset{top: 16. bottom: 8.}
                        draw_text +: {
                            light: instance(0.0)
                            text_style: mod.widgets.ux.TEXT_CHIP{}
                            get_color: fn() { return mix(#x77819A, #x8992A6, self.light) }
                        }
                    }
                    btn_good := ActionBtn{ text: "Tag good"      draw_bg +: {tone: 1.0} draw_text +: {tone: 1.0} }
                    btn_bad := ActionBtn{ text: "Tag bad"        draw_bg +: {tone: 2.0} draw_text +: {tone: 2.0} margin: Inset{top: 8.} }
                    btn_del := ActionBtn{ text: "Delete episode" margin: Inset{top: 8.} }
                    btn_push := ActionBtn{ text: "Push to Hub"   draw_bg +: {tone: 3.0} draw_text +: {tone: 3.0} margin: Inset{top: 8.} }
                    Filler{}
                }
            }
        }

        // ---- timeline ----
        timeline := mod.widgets.ux.Card{
            width: Fill height: 128
            flow: Down
            tl_head := mod.widgets.ux.PanelHead{ title +: { text: "Timeline" } }
            tl_body := View{
                width: Fill height: Fill flow: Down
                padding: Inset{left: 14. right: 14. top: 10. bottom: 10.}
                spacing: 8.0
                ruler := View{
                    width: Fill height: 34
                    show_bg: true
                    draw_bg +: {
                        head: instance(0.4)
                        light: instance(0.0)
                        pixel: fn() {
                            let x = self.pos.x
                            let bg = mix(#x141922, #xF6F8FB, self.light)
                            // tick marks every 10th of the range
                            let major = (1.0 - step(0.004, fract(x * 10.0))) * (1.0 - step(0.55, self.pos.y))
                            let minor = (1.0 - step(0.002, fract(x * 50.0))) * (1.0 - step(0.30, self.pos.y))
                            let ticks = clamp(major + minor * 0.6, 0.0, 1.0)
                            let c = mix(bg, mix(#x4A5470, #xB4BCCB, self.light), ticks)
                            // playhead
                            let ph = 1.0 - smoothstep(0.0, 0.0016, abs(x - self.head))
                            return mix(c, #xE5484D, ph)
                        }
                    }
                }
                drift := View{
                    width: Fill height: 12
                    show_bg: true
                    draw_bg +: {
                        light: instance(0.0)
                        pixel: fn() {
                            // Drift grows toward the tail: where video and
                            // parquet stop agreeing.
                            let x = self.pos.x
                            let d = clamp((x - 0.8) * 5.0, 0.0, 1.0)
                            let base = mix(#x1B2130, #xEDF0F6, self.light)
                            return mix(base, #xD9A24E, d * 0.85)
                        }
                    }
                }
                controls := View{
                    width: Fill height: Fit
                    flow: Right
                    align: Align{y: 0.5}
                    spacing: 12.0
                    frame_lbl := Label{
                        text: "0 / 0"
                        draw_text +: {
                            light: instance(0.0)
                            text_style: mod.widgets.ux.TEXT_META{}
                            get_color: fn() { return mix(#xD7DDEA, #x2A3244, self.light) }
                        }
                    }
                    Filler{}
                    drift_lbl := Label{
                        text: "DRIFT"
                        draw_text +: {
                            light: instance(0.0)
                            text_style: mod.widgets.ux.TEXT_CHIP{}
                            get_color: fn() { return mix(#xD9A24E, #x8A6414, self.light) }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct PlayScreen {
    #[deref]
    view: View,
    /// The grid initialises its own layout, so `layout_state().is_none()` is
    /// never true — seed ours exactly once instead, and let user drags stand.
    #[rust(false)]
    layout_seeded: bool,
    /// Episode index displayed by each row, in row order.
    #[rust]
    row_episode: Vec<u64>,
}

impl Widget for PlayScreen {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
}

const ROW_IDS: [&[LiveId]; 13] = [
    ids!(upper.tree.tree_body.row_0), ids!(upper.tree.tree_body.row_1),
    ids!(upper.tree.tree_body.row_2), ids!(upper.tree.tree_body.row_3),
    ids!(upper.tree.tree_body.row_4), ids!(upper.tree.tree_body.row_5),
    ids!(upper.tree.tree_body.row_6), ids!(upper.tree.tree_body.row_7),
    ids!(upper.tree.tree_body.row_8), ids!(upper.tree.tree_body.row_9),
    ids!(upper.tree.tree_body.row_10), ids!(upper.tree.tree_body.row_11),
    ids!(upper.tree.tree_body.row_12),
];
const GRP_IDS: [&[LiveId]; 3] = [
    ids!(upper.tree.tree_body.grp_0),
    ids!(upper.tree.tree_body.grp_1),
    ids!(upper.tree.tree_body.grp_2),
];

impl PlayScreenRef {
    pub fn sync(&self, cx: &mut Cx, st: &PlaybackState) {
        let light = crate::ui::frame::light_mode();
        let Some(mut inner) = self.borrow_mut() else { return };
        let need_seed = !inner.layout_seeded;
        inner.layout_seeded = true;
        let root = &mut inner.view;

        script_apply_eval!(cx, root, { draw_bg +: { light: #(light) } });
        apply_light_in(cx, root, &[
            (ids!(upper.tree), Themed::Bg),
            (ids!(stage_a), Themed::Bg),
            (ids!(stage_b), Themed::Bg),
            (ids!(stage_3d), Themed::Bg),
            (ids!(stage_plot), Themed::Bg),
            (ids!(upper.side), Themed::Bg),
            (ids!(upper.side.side_body.task_h), Themed::Text),
            (ids!(upper.side.side_body.task_t), Themed::Text),
            (ids!(upper.side.side_body.cur_h), Themed::Text),
            (ids!(upper.side.side_body.btn_good), Themed::Both),
            (ids!(upper.side.side_body.btn_bad), Themed::Both),
            (ids!(upper.side.side_body.btn_del), Themed::Both),
            (ids!(upper.side.side_body.btn_push), Themed::Both),
            (ids!(timeline), Themed::Bg),
            (ids!(timeline.tl_body.ruler), Themed::Bg),
            (ids!(timeline.tl_body.drift), Themed::Bg),
            (ids!(timeline.tl_body.controls.frame_lbl), Themed::Text),
            (ids!(timeline.tl_body.controls.drift_lbl), Themed::Text),
        ], light);
        for p in [
            ids!(upper.tree.tree_head) as &[LiveId],
            ids!(upper.side.side_head),
            ids!(timeline.tl_head),
        ] {
            let head = root.widget(cx, p);
            theme_panel_head(cx, &head, light);
        }

        root.label(cx, ids!(upper.tree.tree_head.title))
            .set_text(cx, &st.dataset_name);

        // The app-shell panels theme through their own dark_mode instance;
        // drive it from the same value so both systems stay in step.
        let dark_mode = 1.0 - light;
        makepad_app_shell::theme::set_global_dark_mode(dark_mode);
        let grid = root.panel_grid(cx, ids!(upper.grid));
        if !grid.is_empty() {
            grid.apply_dark_mode(cx, dark_mode);
            // Seed once; after that the grid owns layout so a user's drag
            // survives the next repaint.
            if need_seed {
                let mut layout = LayoutState::with_panel_count(4);
                // with_panel_count packs three to a row; this screen wants 2x2.
                layout.row_assignments = vec![
                    vec!["panel_0".into(), "panel_1".into()],
                    vec!["panel_2".into(), "panel_3".into()],
                    vec![],
                ];
                layout.visible_panels = ["panel_0", "panel_1", "panel_2", "panel_3"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect();
                for (id, title) in [
                    ("panel_0", "cam_high"),
                    ("panel_1", "3D View"),
                    ("panel_2", "cam_wrist"),
                    ("panel_3", "action vs state"),
                ] {
                    layout.panel_titles.insert(id.into(), title.into());
                }
                grid.set_layout_state(cx, layout);
            }
        }

        // ---- episode tree: group headers interleaved with rows ------------
        let mut groups: Vec<&str> = Vec::new();
        for e in &st.episodes {
            if !groups.iter().any(|g| *g == e.task_group) {
                groups.push(&e.task_group);
            }
        }
        for (i, p) in GRP_IDS.iter().enumerate() {
            let g = root.widget(cx, p);
            if g.is_empty() {
                continue;
            }
            match groups.get(i) {
                Some(name) => {
                    g.set_visible(cx, true);
                    let mut gl = root.widget(cx, p);
                    script_apply_eval!(cx, gl, { draw_text +: { light: #(light) } });
                    root.label(cx, p).set_text(cx, name);
                }
                None => g.set_visible(cx, false),
            }
        }
        // Rows are laid out in group order so the flat list reads as a tree.
        let ordered: Vec<_> = groups
            .iter()
            .flat_map(|g| st.episodes.iter().filter(move |e| &e.task_group == g))
            .collect();
        let row_map: Vec<u64> = ordered.iter().map(|e| e.index).collect();
        for (i, p) in ROW_IDS.iter().enumerate() {
            let row = root.widget(cx, p);
            if row.is_empty() {
                continue;
            }
            match ordered.get(i) {
                Some(e) => {
                    row.set_visible(cx, true);
                    row.label(cx, ids!(ep_name))
                        .set_text(cx, &format!("ep {:03}  ·  {:.1}s", e.index, e.duration_s));
                    let sel = if st.selected == Some(e.index) { 1.0 } else { 0.0 };
                    let mut r = row.clone();
                    script_apply_eval!(cx, r, { draw_bg +: { sel: #(sel) light: #(light) } });
                    let mut n = row.widget(cx, ids!(ep_name));
                    script_apply_eval!(cx, n, { draw_text +: { sel: #(sel) light: #(light) } });

                    let chip = row.widget(cx, ids!(ep_tag));
                    match e.tag {
                        Some(t) => {
                            chip.set_visible(cx, true);
                            let tone = if t == Tag::Good { 1.0 } else { 2.0 };
                            row.label(cx, ids!(ep_tag.label)).set_text(cx, t.label());
                            let mut c = chip.clone();
                            script_apply_eval!(cx, c, { draw_bg +: { tone: #(tone) } });
                            let mut cl = row.widget(cx, ids!(ep_tag.label));
                            script_apply_eval!(cx, cl, { draw_text +: { tone: #(tone) } });
                            theme_chip(cx, &chip, light);
                        }
                        None => chip.set_visible(cx, false),
                    }
                }
                None => row.set_visible(cx, false),
            }
        }

        // ---- inspector ----------------------------------------------------
        let s = &st.stats;
        for (p, val, warn) in [
            (ids!(upper.side.side_body.st_frames) as &[LiveId], format!("{}", s.frames), 0.0),
            (ids!(upper.side.side_body.st_dur), format!("{:.1}s", s.duration_s), 0.0),
            (ids!(upper.side.side_body.st_fps), format!("{}", s.fps as u32), 0.0),
            (
                ids!(upper.side.side_body.st_drift),
                format!("{:+.1} f", s.drift_frames),
                if s.drift_frames.abs() > 0.2 { 1.0 } else { 0.0 },
            ),
        ] {
            let r = root.widget(cx, p);
            if r.is_empty() {
                continue;
            }
            r.label(cx, ids!(v)).set_text(cx, &val);
            apply_light(cx, &r, &[(ids!(k), Themed::Text)], light);
            let mut vw = r.widget(cx, ids!(v));
            script_apply_eval!(cx, vw, { draw_text +: { warn: #(warn) light: #(light) } });
        }
        root.label(cx, ids!(upper.side.side_body.task_t)).set_text(cx, &s.task);

        // ---- timeline ------------------------------------------------------
        let frames = s.frames.max(1);
        let head = 0.4_f64;
        root.label(cx, ids!(timeline.tl_body.controls.frame_lbl))
            .set_text(cx, &format!("{} / {}", (frames as f64 * head) as u64, frames));
        root.label(cx, ids!(timeline.tl_body.controls.drift_lbl))
            .set_text(cx, &format!("DRIFT {:+.1} f", s.drift_frames));
        let mut ruler = root.widget(cx, ids!(timeline.tl_body.ruler));
        script_apply_eval!(cx, ruler, { draw_bg +: { head: #(head) light: #(light) } });

        inner.row_episode = row_map;
    }

    /// Episode whose row was released under the pointer, if any.
    pub fn clicked_episode(&self, cx: &mut Cx, actions: &Actions) -> Option<u64> {
        let mut inner = self.borrow_mut()?;
        let map = inner.row_episode.clone();
        for (i, path) in ROW_IDS.iter().enumerate() {
            let Some(&index) = map.get(i) else { continue };
            let row = inner.view.widget(cx, path);
            if row.is_empty() {
                continue;
            }
            if crate::ui::frame::view_clicked(actions, row.widget_uid()) {
                return Some(index);
            }
        }
        None
    }

    /// Curation button presses, as intents the caller can dispatch.
    pub fn curation_intent(&self, cx: &mut Cx, actions: &Actions, episode: u64) -> Option<Intent> {
        let mut inner = self.borrow_mut()?;
        let v = &mut inner.view;
        let hit = |v: &mut View, cx: &mut Cx, p: &[LiveId]| v.button(cx, p).clicked(actions);
        if hit(v, cx, ids!(upper.side.side_body.btn_good)) {
            return Some(Intent::TagEpisode { episode, tag: Some(Tag::Good) });
        }
        if hit(v, cx, ids!(upper.side.side_body.btn_bad)) {
            return Some(Intent::TagEpisode { episode, tag: Some(Tag::Bad) });
        }
        if hit(v, cx, ids!(upper.side.side_body.btn_del)) {
            return Some(Intent::DeleteEpisode(episode));
        }
        if hit(v, cx, ids!(upper.side.side_body.btn_push)) {
            return Some(Intent::PushToHub);
        }
        None
    }
}
