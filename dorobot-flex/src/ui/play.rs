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
use crate::widgets::time_series_plot::TimeSeriesPlotWidgetExt;
use crate::widgets::timeline::TimelineWidgetExt;
use crate::widgets::video_player::VideoPlayerWidgetExt;
use crate::playback_controls::PlaybackControlsWidgetExt;

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

    // The real plot, fed from the selected episode's parquet. It was a shader
    // drawing a fixed sine pair, which looked plausible and meant selecting an
    // episode changed nothing anyone could see.
    let PlotPane = TimeSeriesPlot{
        width: Fill height: Fill
        header +: { visible: false }
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
                            content +: { stage_a := VideoPlayer{} }
                        }
                        s1_2 +: {
                            title: "3D View"
                            content +: { stage_3d := RobotView{} }
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
                            content +: { stage_b := VideoPlayer{} }
                        }
                        s2_2 +: {
                            title: "tracking error"
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
            width: Fill height: 372
            flow: Down
            tl_head := mod.widgets.ux.PanelHead{ title +: { text: "Playback" } }
            tl_body := View{
                width: Fill height: Fill flow: Down
                padding: Inset{left: 14. right: 14. top: 8. bottom: 10.}
                spacing: 6.0

                controls := View{
                    width: Fill height: Fit
                    flow: Right
                    align: Align{y: 0.5}
                    spacing: 12.0
                    transport := PlaybackControls{
                        width: 160 height: Fit
                        show_bg: false
                        // Time and speed live on the scrubber and the frame
                        // counter already; two clocks would disagree.
                        time_row +: { visible: false }
                        speed_row +: { visible: false }
                    }
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

                tl := Timeline{ width: Fill height: 52 }

                // One lane per joint, each on its own scale, with the command
                // drawn over the measurement. Twelve traces sharing one axis
                // told you nothing: the joints overlap, and a joint that moves
                // a little is flattened by one that moves a lot.
                lanes := View{
                    width: Fill height: Fill flow: Down spacing: 1.0
                    lane_0 := View{
                        width: Fill height: 34 flow: Right align: Align{y: 0.5}
                        name_0 := Label{
                            width: 104
                            draw_text +: {
                                light: instance(0.0)
                                text_style: mod.widgets.ux.TEXT_CHIP{}
                                get_color: fn() { return mix(#x8B94AA, #x6A7286, self.light) }
                            }
                        }
                        plot_0 := TimeSeriesPlot{
                            width: Fill height: Fill
                            header +: { visible: false }
                        }
                    }
                    lane_1 := View{
                        width: Fill height: 34 flow: Right align: Align{y: 0.5}
                        name_1 := Label{
                            width: 104
                            draw_text +: {
                                light: instance(0.0)
                                text_style: mod.widgets.ux.TEXT_CHIP{}
                                get_color: fn() { return mix(#x8B94AA, #x6A7286, self.light) }
                            }
                        }
                        plot_1 := TimeSeriesPlot{
                            width: Fill height: Fill
                            header +: { visible: false }
                        }
                    }
                    lane_2 := View{
                        width: Fill height: 34 flow: Right align: Align{y: 0.5}
                        name_2 := Label{
                            width: 104
                            draw_text +: {
                                light: instance(0.0)
                                text_style: mod.widgets.ux.TEXT_CHIP{}
                                get_color: fn() { return mix(#x8B94AA, #x6A7286, self.light) }
                            }
                        }
                        plot_2 := TimeSeriesPlot{
                            width: Fill height: Fill
                            header +: { visible: false }
                        }
                    }
                    lane_3 := View{
                        width: Fill height: 34 flow: Right align: Align{y: 0.5}
                        name_3 := Label{
                            width: 104
                            draw_text +: {
                                light: instance(0.0)
                                text_style: mod.widgets.ux.TEXT_CHIP{}
                                get_color: fn() { return mix(#x8B94AA, #x6A7286, self.light) }
                            }
                        }
                        plot_3 := TimeSeriesPlot{
                            width: Fill height: Fill
                            header +: { visible: false }
                        }
                    }
                    lane_4 := View{
                        width: Fill height: 34 flow: Right align: Align{y: 0.5}
                        name_4 := Label{
                            width: 104
                            draw_text +: {
                                light: instance(0.0)
                                text_style: mod.widgets.ux.TEXT_CHIP{}
                                get_color: fn() { return mix(#x8B94AA, #x6A7286, self.light) }
                            }
                        }
                        plot_4 := TimeSeriesPlot{
                            width: Fill height: Fill
                            header +: { visible: false }
                        }
                    }
                    lane_5 := View{
                        width: Fill height: 34 flow: Right align: Align{y: 0.5}
                        name_5 := Label{
                            width: 104
                            draw_text +: {
                                light: instance(0.0)
                                text_style: mod.widgets.ux.TEXT_CHIP{}
                                get_color: fn() { return mix(#x8B94AA, #x6A7286, self.light) }
                            }
                        }
                        plot_5 := TimeSeriesPlot{
                            width: Fill height: Fill
                            header +: { visible: false }
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
    /// First episode shown, when the list is longer than the row window.
    #[rust(0usize)]
    scroll: usize,
    /// Episodes in the list, so scrolling knows where the end is.
    #[rust(0usize)]
    total_rows: usize,
    /// Sub-row wheel travel, kept so a trackpad does not quantise to nothing.
    #[rust(0f64)]
    scroll_accum: f64,
    /// Set when the wheel moved the window; the app re-syncs and clears it.
    #[rust(false)]
    scrolled: bool,
    /// Episode whose media is currently loaded, so video and URDF are opened
    /// once per selection rather than on every sync.
    #[rust]
    loaded_episode: Option<u64>,
}

impl Widget for PlayScreen {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        // The row window is driven here rather than by a ScrollY view: the rows
        // are fixed DSL widgets reused across a much longer list, so there is
        // nothing taller than the viewport for a scroll bar to move.
        if let Event::Scroll(e) = event {
            let max = self.total_rows.saturating_sub(ROW_IDS.len());
            let tree = self.view.widget(cx, ids!(upper.tree));
            if max > 0 && !tree.is_empty() && tree.area().rect(cx).contains(e.abs) {
                self.scroll_accum += e.scroll.y;
                let rows = (self.scroll_accum / ROW_H) as i64;
                if rows != 0 {
                    self.scroll_accum -= rows as f64 * ROW_H;
                    let next = (self.scroll as i64 + rows).clamp(0, max as i64) as usize;
                    if next != self.scroll {
                        self.scroll = next;
                        self.scrolled = true;
                        e.handled_y.set(true);
                    }
                }
            }
        }
        self.view.handle_event(cx, event, scope);
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
}

/// Seconds of trace kept either side of the playhead, so the plot scrolls with
/// playback instead of standing still.
const PLOT_WINDOW_S: f64 = 10.0;

/// Footer lanes, one per joint.
const LANE_IDS: [(&[LiveId], &[LiveId]); 6] = [
    (ids!(timeline.tl_body.lanes.lane_0.plot_0), ids!(timeline.tl_body.lanes.lane_0.name_0)),
    (ids!(timeline.tl_body.lanes.lane_1.plot_1), ids!(timeline.tl_body.lanes.lane_1.name_1)),
    (ids!(timeline.tl_body.lanes.lane_2.plot_2), ids!(timeline.tl_body.lanes.lane_2.name_2)),
    (ids!(timeline.tl_body.lanes.lane_3.plot_3), ids!(timeline.tl_body.lanes.lane_3.name_3)),
    (ids!(timeline.tl_body.lanes.lane_4.plot_4), ids!(timeline.tl_body.lanes.lane_4.name_4)),
    (ids!(timeline.tl_body.lanes.lane_5.plot_5), ids!(timeline.tl_body.lanes.lane_5.name_5)),
];

/// Must track `EpisodeRow`'s height, so a wheel notch advances whole rows.
const ROW_H: f64 = 26.0;

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

        // Episode order and the visible window are derived from state alone, so
        // they are settled before the view is borrowed.
        let mut groups: Vec<&str> = Vec::new();
        for e in &st.episodes {
            if !groups.iter().any(|g| *g == e.task_group) {
                groups.push(&e.task_group);
            }
        }
        // Rows are laid out in group order so the flat list reads as a tree.
        let ordered: Vec<_> = groups
            .iter()
            .flat_map(|g| st.episodes.iter().filter(move |e| &e.task_group == g))
            .collect();

        // Below the window size the tree renders as authored, with headers
        // interleaved; above it the headers cannot move to arbitrary row slots,
        // so the list flattens and one header reports the position instead.
        let total = ordered.len();
        let scrolling = total > ROW_IDS.len();
        inner.total_rows = total;
        inner.scroll = inner.scroll.min(total.saturating_sub(ROW_IDS.len()));
        let start = if scrolling { inner.scroll } else { 0 };

        // Media is opened once per selection, so the decision is taken before
        // the view is borrowed.
        let need_media = inner.loaded_episode != st.selected;
        if need_media {
            inner.loaded_episode = st.selected;
        }

        let root = &mut inner.view;

        script_apply_eval!(cx, root, { draw_bg +: { light: #(light) } });
        apply_light_in(cx, root, &[
            (ids!(upper.tree), Themed::Bg),
            (ids!(upper.side), Themed::Bg),
            (ids!(upper.side.side_body.task_h), Themed::Text),
            (ids!(upper.side.side_body.task_t), Themed::Text),
            (ids!(upper.side.side_body.cur_h), Themed::Text),
            (ids!(upper.side.side_body.btn_good), Themed::Both),
            (ids!(upper.side.side_body.btn_bad), Themed::Both),
            (ids!(upper.side.side_body.btn_del), Themed::Both),
            (ids!(upper.side.side_body.btn_push), Themed::Both),
            (ids!(timeline), Themed::Bg),
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
                    ("panel_3", "tracking error"),
                ] {
                    layout.panel_titles.insert(id.into(), title.into());
                }
                grid.set_layout_state(cx, layout);
            }
        }

        // ---- episode tree: group headers interleaved with rows ------------
        let shown = (start + ROW_IDS.len()).min(total);
        for (i, p) in GRP_IDS.iter().enumerate() {
            let g = root.widget(cx, p);
            if g.is_empty() {
                continue;
            }
            // Only the first header survives a scrolling list, and it stops
            // naming a group it can no longer be positioned above.
            let text = match (scrolling, i) {
                // Counted in episode index, not list position: the rows are
                // labelled "ep 029", so a header reading "30–42" would be a
                // second numbering for the same thing.
                (true, 0) => {
                    let first = ordered.get(start).map(|e| e.index).unwrap_or(0);
                    let last = ordered.get(shown - 1).map(|e| e.index).unwrap_or(0);
                    let range = format!("ep {:03}–{:03} of {}", first, last, total);
                    Some(if groups.len() == 1 { range } else { range + "  ·  all tasks" })
                }
                (true, _) => None,
                (false, _) => groups.get(i).map(|n| n.to_string()),
            };
            match text {
                Some(name) => {
                    g.set_visible(cx, true);
                    let mut gl = root.widget(cx, p);
                    script_apply_eval!(cx, gl, { draw_text +: { light: #(light) } });
                    root.label(cx, p).set_text(cx, &name);
                }
                None => g.set_visible(cx, false),
            }
        }
        let row_map: Vec<u64> = ordered[start..].iter().map(|e| e.index).collect();
        let ordered = &ordered[start..];
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

        // ---- plot: measured vs commanded for the selected episode ----------
        // The plot reads the app-shell theme global rather than our light
        // instance, so the two are kept in step here.
        makepad_app_shell::theme::set_global_dark_mode(1.0 - light);

        // One lane per joint: measurement and command together, each lane on
        // its own scale. Rebuilt only when the episode changes, since this
        // copies every sample and sync runs on each transport tick.
        let plot = root.time_series_plot(cx, ids!(stage_plot));
        if need_media {
            for (i, (plot_id, name_id)) in LANE_IDS.iter().enumerate() {
                let lane = root.time_series_plot(cx, plot_id);
                lane.clear();
                let label = st
                    .state_series
                    .get(i)
                    .map(|c| c.name.trim_start_matches("main_").to_string())
                    .unwrap_or_default();
                root.label(cx, name_id).set_text(cx, &label);
                if let Some(c) = st.state_series.get(i) {
                    lane.set_channel_data(0, &c.name, c.points.clone());
                }
                if let Some(c) = st.action_series.get(i) {
                    lane.set_channel_data(1, &format!("{} cmd", c.name), c.points.clone());
                }
                lane.set_time_range(0.0, st.stats.duration_s);
                lane.set_auto_scale_y(true);
                lane.set_window_size(PLOT_WINDOW_S);
                lane.recompute_scale(cx);
            }

            // The grid pane keeps a view the lanes cannot give: how far the
            // arm was from what it was told, all joints against one zero.
            plot.clear();
            for (i, st_c) in st.state_series.iter().enumerate() {
                let Some(act) = st.action_series.get(i) else { continue };
                let err: Vec<(f64, f64)> = st_c
                    .points
                    .iter()
                    .zip(act.points.iter())
                    .map(|((t, s), (_, a))| (*t, a - s))
                    .collect();
                let name = st_c.name.trim_start_matches("main_");
                plot.set_channel_data(i, &format!("{name} err"), err);
            }
            plot.set_time_range(0.0, st.stats.duration_s);
            plot.set_auto_scale_y(true);
            plot.set_window_size(PLOT_WINDOW_S);
            plot.recompute_scale(cx);
        }

        // ---- media: load once per episode, then follow the playhead --------
        if need_media {
            Self::load_media(cx, root, st);
        }
        Self::follow_playhead(cx, root, st);

        inner.row_episode = row_map;
    }

    /// Everything that depends on the playhead, and nothing that does not.
    ///
    /// The transport ticks at 60Hz; running a whole `sync` there costs more
    /// than the frame budget (every row re-evaluated, every theme re-applied)
    /// and playback visibly runs slow as a result.
    fn follow_playhead(cx: &mut Cx, root: &mut View, st: &PlaybackState) {
        let frames = st.stats.frames.max(1);
        let frame_idx = st.frame_index();

        for path in [ids!(stage_a) as &[LiveId], ids!(stage_b)] {
            let player = root.video_player(cx, path);
            if player.borrow().is_some() {
                player.show_frame_at_time(cx, st.current_time);
                player.set_frame_info(cx, frame_idx, frames);
            }
        }

        // The 3D mirror follows the same playhead, which is what the shipping
        // player never did — its robot view did not move during playback.
        let viewer = root.widget(cx, ids!(stage_3d));
        if let Some(joints) = st.joints_now() {
            if let Some(mut rv) = viewer.borrow_mut::<makepad_urdf_player::robot_view::RobotView>() {
                // Already in URDF convention: the backend owns that mapping,
                // because it is a property of the robot, not of the view.
                rv.set_joint_angles(cx, joints);
            }
        }

        root.time_series_plot(cx, ids!(stage_plot))
            .set_cursor_time(cx, st.current_time);
        for (plot_id, _) in LANE_IDS.iter() {
            root.time_series_plot(cx, plot_id)
                .set_cursor_time(cx, st.current_time);
        }

        let tl = root.timeline(cx, ids!(timeline.tl_body.tl));
        tl.set_duration(cx, st.stats.duration_s, st.stats.fps);
        tl.set_current_time(cx, st.current_time);
        tl.set_playing(cx, st.is_playing);
        tl.set_speed(cx, st.speed);
        root.playback_controls(cx, ids!(timeline.tl_body.controls.transport))
            .set_playing(cx, st.is_playing);
        root.label(cx, ids!(timeline.tl_body.controls.frame_lbl))
            .set_text(cx, &format!("{} / {}", frame_idx.min(frames), frames));
    }

    /// Transport tick: advance the views the playhead drives, nothing else.
    pub fn tick(&self, cx: &mut Cx, st: &PlaybackState) {
        let Some(mut inner) = self.borrow_mut() else { return };
        let root = &mut inner.view;
        Self::follow_playhead(cx, root, st);
    }

    /// Point the two camera panes and the robot at this episode's media.
    ///
    /// Camera keys are matched by preference and then by position, the way the
    /// shipping player does it, so a dataset naming its cameras `top`/`wrist`
    /// lands in the same panes as one naming them `cam_high`/`cam_left_wrist`.
    fn load_media(cx: &mut Cx, root: &mut View, st: &PlaybackState) {
        let keys: Vec<&String> = st.video_paths.keys().collect();
        let pick = |cands: &[&str], fallback: usize| -> Option<String> {
            cands
                .iter()
                .find_map(|c| keys.iter().find(|k| k.contains(c)).map(|k| (*k).clone()))
                .or_else(|| keys.get(fallback).map(|k| (*k).clone()))
        };
        let main = pick(&["cam_high", "top", "image"], 0);
        let wrist = pick(&["wrist", "cam_low", "side"], 1);

        for (path, key) in [
            (ids!(stage_a) as &[LiveId], main),
            (ids!(stage_b), wrist),
        ] {
            let player = root.video_player(cx, path);
            if player.borrow().is_none() {
                continue;
            }
            player.clear(cx);
            let Some(key) = key else { continue };
            // A second pane showing the same file is worse than an empty one.
            player.set_episode_info(st.video_frame_offset, st.stats.frames);
            player.set_fps_display(cx, st.stats.fps);
            if let Some(file) = st.video_paths.get(&key) {
                if let Err(e) = player.load_video(cx, &file.to_string_lossy()) {
                    ::log::error!("play: video {key} failed to load: {e}");
                }
            }
        }

        let viewer = root.widget(cx, ids!(stage_3d));
        if let Some((urdf, assets)) = &st.robot_urdf {
            if let Some(mut rv) = viewer.borrow_mut::<makepad_urdf_player::robot_view::RobotView>() {
                if let Err(e) = rv.load_robot(&urdf.to_string_lossy(), &assets.to_string_lossy()) {
                    ::log::error!("play: urdf {} failed to load: {e}", urdf.display());
                }
            }
        }
    }

    /// True once if the wheel moved the row window, so the app re-syncs it.
    pub fn take_scrolled(&self) -> bool {
        let Some(mut inner) = self.borrow_mut() else { return false };
        std::mem::take(&mut inner.scrolled)
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
