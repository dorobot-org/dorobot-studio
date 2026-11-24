//! Episode player & curation.
//!
//! Mirrors `docs/ux/ux-04-player.png`. Adds the two things the shipping player
//! lacks: curation actions, and an honest timeline that shows video/parquet
//! drift instead of assuming `frame / fps`.

use makepad_widgets::*;

use crate::api::{Intent, PlaybackState, PlotChannel, Tag};
use crate::playback_controls::PlaybackControlsWidgetExt;
use crate::ui::flex_layout::{FlexPanelsWidgetExt, PanelLayout};
use crate::ui::frame::{apply_light, apply_light_in, theme_chip, theme_panel_head, Themed};
use crate::widgets::time_series_plot::TimeSeriesPlotWidgetExt;
use crate::widgets::timeline::TimelineWidgetExt;
use crate::widgets::video_player::VideoPlayerWidgetExt;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    use mod.widgets.ux.*

    // Episode row in the tree: index, duration, quality tag.
    let EpisodeRow = View{
        width: Fill height: 44
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
                let on = mix(#x19334D, #xE5F0FF, self.light)
                sdf.box(4.0, 1.0, self.rect_size.x - 8.0, self.rect_size.y - 2.0, 8.0)
                sdf.fill(mix(base, on, self.sel))
                return sdf.result
            }
        }
        ep_name := Label{
            width: Fill height: Fit padding: 0 max_lines: 1 text_overflow: Ellipsis text: "ep 000"
            draw_text +: {
                sel: instance(0.0)
                light: instance(0.0)
                text_style: mod.widgets.ux.TEXT_META{}
                get_color: fn() {
                    let idle = mix(#xD1D1D6, #x636366, self.light)
                    let on = mix(#x64B5FF, #x0060CC, self.light)
                    return mix(idle, on, self.sel)
                }
            }
        }
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
            get_color: fn() { return mix(#xAEAEB2, #x636366, self.light) }
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
                get_color: fn() { return mix(#xAEAEB2, #x636366, self.light) }
            }
        }
        v := Label{
            text: "-"
            draw_text +: {
                warn: instance(0.0)
                light: instance(0.0)
                text_style: mod.widgets.ux.TEXT_META{}
                get_color: fn() {
                    let normal = mix(#xD1D1D6, #x38383A, self.light)
                    let alert = mix(#xFFB340, #x996300, self.light)
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
            sel: instance(0.0)
            pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                // 0 neutral, 1 good, 2 bad, 3 primary
                let d_ok = mix(#x2C2C2E, #x38383A, step(0.5, self.tone))
                let d_bad = mix(d_ok, #x38383A, step(1.5, self.tone))
                let dark_f = mix(d_bad, #x0060CC, step(2.5, self.tone))
                let l_ok = mix(#xFFFFFF, #xF2F2F7, step(0.5, self.tone))
                let l_bad = mix(l_ok, #xF2F2F7, step(1.5, self.tone))
                let light_f = mix(l_bad, #x0060CC, step(2.5, self.tone))
                let fill = mix(dark_f, light_f, self.light)
                let d_e = mix(#x38383A, #x30D158, step(0.5, self.tone))
                let d_e2 = mix(d_e, #xFF453A, step(1.5, self.tone))
                let dark_e = mix(d_e2, #x0060CC, step(2.5, self.tone))
                let l_e = mix(#xD1D1D6, #xD1D1D6, step(0.5, self.tone))
                let l_e2 = mix(l_e, #xD70015, step(1.5, self.tone))
                let light_e = mix(l_e2, #x0060CC, step(2.5, self.tone))
                sdf.box(0.5, 0.5, self.rect_size.x - 1.0, self.rect_size.y - 1.0, mod.widgets.nx.BUTTON_RADIUS)
                // The tag an episode already carries reads as pressed, so the
                // sidebar answers "what is this tagged?" without looking away.
                let hot = mix(fill, mix(fill * 1.55, fill * 1.22, self.light), self.sel)
                sdf.fill_keep(mix(hot, hot * 1.12, self.hover))
                let edge = mix(dark_e, light_e, self.light)
                sdf.stroke(mix(edge, edge * 1.6, self.sel), mix(1.0, 1.6, self.sel))
                return sdf.result
            }
        }
        draw_text +: {
            tone: instance(0.0)
            light: instance(0.0)
            sel: instance(0.0)
            text_style: mod.widgets.ux.TEXT_BODY{}
            get_color: fn() {
                let d_ok = mix(#xD1D1D6, #x30D158, step(0.5, self.tone))
                let d_bad = mix(d_ok, #xFF6961, step(1.5, self.tone))
                let dark_c = mix(d_bad, #xFFFFFF, step(2.5, self.tone))
                let l_ok = mix(#x38383A, #x248A3D, step(0.5, self.tone))
                let l_bad = mix(l_ok, #xD70015, step(1.5, self.tone))
                let light_c = mix(l_bad, #xFFFFFF, step(2.5, self.tone))
                let base = mix(dark_c, light_c, self.light)
                // A selected button fills with its own colour, so the label has
                // to leave that colour to stay legible on it.
                return mix(base, mix(#xF2F2F7, #x1C1C1E, self.light), self.sel)
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
                return mix(mix(#x2C2C2E, #x000000, clamp(d, 0.0, 1.0)),
                           mix(#xF2F2F7, #xE5E5EA, clamp(d, 0.0, 1.0)), self.light)
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
                let base = mix(#x1C1C1E, #xF2F2F7, self.light)
                return mix(base, mix(#x38383A, #xD1D1D6, self.light), g)
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
        spacing: 6.0
        padding: 6.0
        show_bg: true
        draw_bg +: {
            light: instance(0.0)
            pixel: fn() { return mix(#x111114, #xF2F2F7, self.light) }
        }

        toolbar := View{width: Fill height: 32 flow: Right spacing: 8.0
            episodes_btn := mod.widgets.ux.ToolButton{height: 32 text: "Episodes"}
            signals_btn := mod.widgets.ux.ToolButton{height: 32 text: "Show signals"}
            view_mode_btn := mod.widgets.ux.ToolButton{height: 32 text: "View: Follow" visible: false}
            signal_nav := View{width: Fit height: 32 flow: Right spacing: 4.0 align: Align{y: 0.5} visible: false
                signal_prev := mod.widgets.ux.ToolButton{width: 32 height: 32 text: "↑"}
                signal_count := Label{text: "1–2 / 6" draw_text +: {color: mod.theme.color_text text_style: mod.widgets.ux.TEXT_META{}}}
                signal_next := mod.widgets.ux.ToolButton{width: 32 height: 32 text: "↓"}
            }
            Filler{}
            details_btn := mod.widgets.ux.ToolButton{height: 32 text: "Details"}
            panels_btn := mod.widgets.ux.ToolButton{height: 32 text: "Panels"}
        }
        panel_tools := View{visible: false width: Fill height: Fit flow: Flow.Right{wrap: true} spacing: 6.0
            robot_toggle := mod.widgets.ux.ToolButton{height: 32 text: "3D view"}
            camera_a_toggle := mod.widgets.ux.ToolButton{height: 32 text: "Main camera"}
            camera_b_toggle := mod.widgets.ux.ToolButton{height: 32 text: "Wrist camera"}
            camera_c_toggle := mod.widgets.ux.ToolButton{height: 32 text: "Right wrist" visible: false}
            error_toggle := mod.widgets.ux.ToolButton{height: 32 text: "Tracking error"}
            layout_cycle := mod.widgets.ux.ToolButton{height: 32 text: "Arrange"}
            layout_reset := mod.widgets.ux.ToolButton{height: 32 text: "Reset layout"}
            new_window := mod.widgets.ux.ToolButton{height: 32 text: "Replay window"}
        }
        upper := View{
            width: Fill height: Fill
            flow: Right
            spacing: 6.0

            // ---- episode tree ----
            tree := mod.widgets.ux.Card{
                width: 240 height: Fill
                flow: Down
                tree_head := mod.widgets.ux.PanelHead{ title +: { text: "dataset" } }
                tree_body := View{
                    width: Fill height: Fill flow: Down
                    // Right padding clears the scroll bar; without it the bar
                    // overlaps the quality chips at the row's trailing edge.
                    padding: Inset{bottom: 8. right: 10.}
                    // The row window above only engages once the list is longer
                    // than the fixed row slots; when the panel is shorter than
                    // those slots the tail was simply clipped and unreachable.
                    // Real scroll bars cover that case, and both together mean
                    // the list is reachable whatever the panel height.
                    scroll_bars: ScrollBars{
                        show_scroll_x: false
                        show_scroll_y: true
                    }
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

            grid := FlexPanels{
                robot_panel := StudioPanel{
                    head +: {title +: {text: "3D view"}}
                    no_robot := View{width: Fill height: Fill align: Align{x: 0.5 y: 0.5} padding: 16.0
                        no_robot_label := Label{width: Fill align: Align{x: 0.5} text: "No robot model in this dataset"
                            draw_text +: {color: mod.theme.color_text text_style: mod.widgets.ux.TEXT_META{}}}
                    }
                    stage_3d := RobotView{
                        keyboard_controls: false width: Fill height: Fill
                        material_roughness: 0.68 material_metallic: 0.02
                        sky_horizon: mod.theme.color_bg_app sky_zenith: mod.theme.color_bg_app
                        ground_color: mod.theme.color_bg_container ground2_color: mod.theme.color_bg_container
                        grid_color: mod.theme.color_text_disabled
                    }
                }
                camera_a_panel := StudioPanel{
                    head +: {title +: {text: "Main camera"}}
                    stage_a := VideoPlayer{}
                }
                camera_b_panel := StudioPanel{
                    head +: {title +: {text: "Wrist camera"}}
                    stage_b := VideoPlayer{}
                }
                camera_c_panel := StudioPanel{
                    head +: {title +: {text: "Right wrist"}}
                    stage_c := VideoPlayer{}
                }
                error_panel := StudioPanel{
                    head +: {title +: {text: "Tracking error"}}
                    stage_plot := PlotPane{}
                    control_body := View{visible: false width: Fill height: Fill flow: Down spacing: 2.0
                        control_3d := RobotView{
                            keyboard_controls: false width: Fill height: Fill show_grid: false
                            material_roughness: 0.68 material_metallic: 0.02
                            sky_horizon: mod.theme.color_bg_app sky_zenith: mod.theme.color_bg_app
                        }
                        control_angles := Label{width: Fill padding: 0.0 text: ""
                            draw_text +: {color: mod.theme.color_text text_style: mod.widgets.ux.TEXT_CHIP{}}}
                        control_feedback := Label{width: Fill padding: 0.0 max_lines: 1 text_overflow: Ellipsis text: ""
                            draw_text +: {color: mod.theme.color_text text_style: mod.widgets.ux.TEXT_CHIP{}}}
                        control_hint := Label{width: Fill padding: 0.0 text: "Drag: orbit · Scroll: zoom"
                            draw_text +: {color: mod.theme.color_text_disabled text_style: mod.widgets.ux.TEXT_CHIP{}}}
                    }
                }
                empty := View{visible: false width: Fill height: Fill flow: Down padding: 24.0 spacing: 16.0
                    Label{width: Fill text: "All panels are closed. Use Panels to reopen a view or reset the layout."
                        draw_text +: {color: mod.theme.color_text text_style: mod.widgets.ux.TEXT_BODY{}}}
                }
            }

            side := mod.widgets.ux.Card{
                width: 264 height: Fill
                flow: Down
                side_head := mod.widgets.ux.PanelHead{ title +: { text: "Episode" } }
                // Scrolls, so dragging the footer up can never put the
                // curation buttons out of reach.
                side_body := ScrollYView{
                    width: Fill height: Fill flow: Down
                    padding: Inset{left: 10. right: 10. top: 8. bottom: 14.}
                    spacing: 2.0
                    btn_good := ActionBtn{ text: "Tag good"      draw_bg +: {tone: 1.0} draw_text +: {tone: 1.0} }
                    btn_bad := ActionBtn{ text: "Tag bad"        draw_bg +: {tone: 2.0} draw_text +: {tone: 2.0} margin: Inset{top: 8.} }
                    st_frames := StatRow{ k +: { text: "FRAMES" } }
                    st_dur := StatRow{ k +: { text: "DURATION" } }
                    st_fps := StatRow{ k +: { text: "FPS" } }
                    st_drift := StatRow{ k +: { text: "Sync offset" } }
                    task_h := Label{
                        text: "TASK"
                        margin: Inset{top: 14. bottom: 4.}
                        draw_text +: {
                            light: instance(0.0)
                            text_style: mod.widgets.ux.TEXT_CHIP{}
                            get_color: fn() { return mix(#xAEAEB2, #x636366, self.light) }
                        }
                    }
                    task_t := Label{
                        width: Fill
                        text: ""
                        draw_text +: {
                            light: instance(0.0)
                            text_style: mod.widgets.ux.TEXT_BODY{}
                            get_color: fn() { return mix(#xD1D1D6, #x38383A, self.light) }
                        }
                    }
                    cur_h := Label{
                        text: "CURATION"
                        margin: Inset{top: 16. bottom: 8.}
                        draw_text +: {
                            light: instance(0.0)
                            text_style: mod.widgets.ux.TEXT_CHIP{}
                            get_color: fn() { return mix(#xAEAEB2, #x636366, self.light) }
                        }
                    }
                    btn_del := ActionBtn{ text: "Exclude episode" margin: Inset{top: 8.} }
                    btn_push := ActionBtn{ text: "Push to Hub"   draw_bg +: {tone: 3.0} draw_text +: {tone: 3.0} margin: Inset{top: 8.} }
                    Filler{}
                }
            }
        }

        // ---- timeline ----
        timeline := mod.widgets.ux.Card{
            width: Fill height: 104
            flow: Down padding: 6.0
            // No close button: the transport is not something to dismiss. The
            // head doubles as the resize grip for the footer.
            tl_head := mod.widgets.ux.PanelHead{
                height: 8
                title +: { visible: false text: "Playback" }
                cursor: MouseCursor.RowResize
                btn_max +: { visible: false }
                btn_close +: { visible: false }
            }
            tl_body := View{
                width: Fill height: Fill flow: Down
                padding: Inset{left: 4. right: 4. top: 2. bottom: 2.}
                spacing: 4.0

                controls := View{
                    width: Fill height: Fit
                    flow: Right
                    align: Align{y: 0.5}
                    spacing: 12.0
                    transport := PlaybackControls{
                        width: 136 height: 32 padding: 0.0 spacing: 0.0
                        show_bg: false
                        buttons_row +: {height: 32
                            step_back_btn +: {width: 32 height: 32 min_height: 32 padding: 0.0 margin: 0.0}
                            play_btn +: {width: 60 height: 32 min_height: 32 padding: 0.0 margin: 0.0}
                            step_fwd_btn +: {width: 32 height: 32 min_height: 32 padding: 0.0 margin: 0.0}
                        }
                        // The shared transport row owns the clock and speed control.
                        time_row +: { visible: false }
                        speed_row +: { visible: false }
                    }
                    frame_lbl := Label{
                        text: "0 / 0"
                        draw_text +: {
                            light: instance(0.0)
                            text_style: mod.widgets.ux.TEXT_META{}
                            get_color: fn() { return mix(#xD1D1D6, #x38383A, self.light) }
                        }
                    }
                    Filler{}
                    speed_btn := mod.widgets.ux.ToolButton{width: 100 height: 32 text: "Speed 1×"}
                    drift_lbl := Label{
                        text: "Sync offset"
                        draw_text +: {
                            light: instance(0.0)
                            text_style: mod.widgets.ux.TEXT_CHIP{}
                            get_color: fn() { return mix(#xFFB340, #x996300, self.light) }
                        }
                    }
                }

                tl := Timeline{ width: Fill height: 36 }

                // One lane per joint, each on its own scale, with the command
                // drawn over the measurement. Twelve traces sharing one axis
                // told you nothing: the joints overlap, and a joint that moves
                // a little is flattened by one that moves a lot.
                lanes := View{
                    width: Fill height: Fill
                    content := View{width: Fill height: Fit flow: Down spacing: 1.0
                    lane_0 := View{
                        width: Fill height: 44 flow: Right align: Align{y: 0.5}
                        caption_0 := View{width: 216 height: Fill flow: Down spacing: 2.0 align: Align{y: 0.5}
                        name_0 := Label{
                            width: Fill height: Fit padding: 0 max_lines: 1 text_overflow: Ellipsis
                            draw_text +: {
                                light: instance(0.0)
                                text_style: mod.widgets.ux.TEXT_CHIP{}
                                get_color: fn() { return mix(#xD1D1D6, #x636366, self.light) }
                            }
                        }
                            value_0 := Label{
                                width: Fill height: Fit padding: 0 max_lines: 1 text_overflow: Ellipsis
                                draw_text +: {
                                    light: instance(0.0)
                                    text_style: mod.widgets.ux.TEXT_CHIP{}
                                    get_color: fn() { return mix(#xAEAEB2, #x636366, self.light) }
                                }
                            }
                        }
                        plot_0 := TimeSeriesPlot{
                            width: Fill height: Fill
                            header +: { visible: false }
                        }
                    }
                    lane_1 := View{
                        width: Fill height: 44 flow: Right align: Align{y: 0.5}
                        caption_1 := View{width: 216 height: Fill flow: Down spacing: 2.0 align: Align{y: 0.5}
                        name_1 := Label{
                            width: Fill height: Fit padding: 0 max_lines: 1 text_overflow: Ellipsis
                            draw_text +: {
                                light: instance(0.0)
                                text_style: mod.widgets.ux.TEXT_CHIP{}
                                get_color: fn() { return mix(#xD1D1D6, #x636366, self.light) }
                            }
                        }
                            value_1 := Label{
                                width: Fill height: Fit padding: 0 max_lines: 1 text_overflow: Ellipsis
                                draw_text +: {
                                    light: instance(0.0)
                                    text_style: mod.widgets.ux.TEXT_CHIP{}
                                    get_color: fn() { return mix(#xAEAEB2, #x636366, self.light) }
                                }
                            }
                        }
                        plot_1 := TimeSeriesPlot{
                            width: Fill height: Fill
                            header +: { visible: false }
                        }
                    }
                    lane_2 := View{
                        width: Fill height: 44 flow: Right align: Align{y: 0.5}
                        caption_2 := View{width: 216 height: Fill flow: Down spacing: 2.0 align: Align{y: 0.5}
                        name_2 := Label{
                            width: Fill height: Fit padding: 0 max_lines: 1 text_overflow: Ellipsis
                            draw_text +: {
                                light: instance(0.0)
                                text_style: mod.widgets.ux.TEXT_CHIP{}
                                get_color: fn() { return mix(#xD1D1D6, #x636366, self.light) }
                            }
                        }
                            value_2 := Label{
                                width: Fill height: Fit padding: 0 max_lines: 1 text_overflow: Ellipsis
                                draw_text +: {
                                    light: instance(0.0)
                                    text_style: mod.widgets.ux.TEXT_CHIP{}
                                    get_color: fn() { return mix(#xAEAEB2, #x636366, self.light) }
                                }
                            }
                        }
                        plot_2 := TimeSeriesPlot{
                            width: Fill height: Fill
                            header +: { visible: false }
                        }
                    }
                    lane_3 := View{
                        width: Fill height: 44 flow: Right align: Align{y: 0.5}
                        caption_3 := View{width: 216 height: Fill flow: Down spacing: 2.0 align: Align{y: 0.5}
                        name_3 := Label{
                            width: Fill height: Fit padding: 0 max_lines: 1 text_overflow: Ellipsis
                            draw_text +: {
                                light: instance(0.0)
                                text_style: mod.widgets.ux.TEXT_CHIP{}
                                get_color: fn() { return mix(#xD1D1D6, #x636366, self.light) }
                            }
                        }
                            value_3 := Label{
                                width: Fill height: Fit padding: 0 max_lines: 1 text_overflow: Ellipsis
                                draw_text +: {
                                    light: instance(0.0)
                                    text_style: mod.widgets.ux.TEXT_CHIP{}
                                    get_color: fn() { return mix(#xAEAEB2, #x636366, self.light) }
                                }
                            }
                        }
                        plot_3 := TimeSeriesPlot{
                            width: Fill height: Fill
                            header +: { visible: false }
                        }
                    }
                    lane_4 := View{
                        width: Fill height: 44 flow: Right align: Align{y: 0.5}
                        caption_4 := View{width: 216 height: Fill flow: Down spacing: 2.0 align: Align{y: 0.5}
                        name_4 := Label{
                            width: Fill height: Fit padding: 0 max_lines: 1 text_overflow: Ellipsis
                            draw_text +: {
                                light: instance(0.0)
                                text_style: mod.widgets.ux.TEXT_CHIP{}
                                get_color: fn() { return mix(#xD1D1D6, #x636366, self.light) }
                            }
                        }
                            value_4 := Label{
                                width: Fill height: Fit padding: 0 max_lines: 1 text_overflow: Ellipsis
                                draw_text +: {
                                    light: instance(0.0)
                                    text_style: mod.widgets.ux.TEXT_CHIP{}
                                    get_color: fn() { return mix(#xAEAEB2, #x636366, self.light) }
                                }
                            }
                        }
                        plot_4 := TimeSeriesPlot{
                            width: Fill height: Fill
                            header +: { visible: false }
                        }
                    }
                    lane_5 := View{
                        width: Fill height: 44 flow: Right align: Align{y: 0.5}
                        caption_5 := View{width: 216 height: Fill flow: Down spacing: 2.0 align: Align{y: 0.5}
                        name_5 := Label{
                            width: Fill height: Fit padding: 0 max_lines: 1 text_overflow: Ellipsis
                            draw_text +: {
                                light: instance(0.0)
                                text_style: mod.widgets.ux.TEXT_CHIP{}
                                get_color: fn() { return mix(#xD1D1D6, #x636366, self.light) }
                            }
                        }
                            value_5 := Label{
                                width: Fill height: Fit padding: 0 max_lines: 1 text_overflow: Ellipsis
                                draw_text +: {
                                    light: instance(0.0)
                                    text_style: mod.widgets.ux.TEXT_CHIP{}
                                    get_color: fn() { return mix(#xAEAEB2, #x636366, self.light) }
                                }
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
}

#[derive(Script, ScriptHook, Widget)]
pub struct PlayScreen {
    #[deref]
    view: View,
    #[rust(false)]
    episodes_open: bool,
    #[rust]
    signal_first: usize,
    #[rust]
    signal_rows: usize,
    #[rust]
    signal_order: Vec<usize>,
    #[rust]
    loaded_signal_first: Option<usize>,
    #[rust(false)]
    details_open: bool,
    #[rust(false)]
    signals_open: bool,
    #[rust]
    live_mode: bool,
    #[rust(true)]
    follow_robot: bool,
    #[rust]
    layout_width: f64,
    #[rust]
    toolbar_layout: Option<PanelLayout>,
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
    #[rust]
    loaded_dataset: String,
    #[rust]
    model_loader: crate::data::robot_model::RobotModelLoader,
    #[rust]
    model_pose: ModelPose,
    #[rust]
    model_error: Option<String>,
    /// Footer height, dragged by its head.
    #[rust(208f64)]
    footer_h: f64,
    /// `(pointer y at grab, footer height at grab)` while resizing.
    #[rust]
    footer_drag: Option<(f64, f64)>,
}

#[derive(Default)]
struct ModelPose {
    names: Vec<String>,
    joints: Vec<f32>,
    base: Option<crate::api::scene::BasePose>,
    scene: bool,
    requested: bool,
}

impl PlayScreen {
    fn capture_model_pose(&mut self, st: &PlaybackState) {
        self.model_pose.names.clone_from(&st.state_names);
        self.model_pose.joints.clear();
        if let Some(joints) = st.joints_now() { self.model_pose.joints.extend_from_slice(joints); }
        self.model_pose.base = st.scene.as_ref().and_then(|s| s.frames.get(st.frame_index() as usize)).and_then(|f| f.base_pose.clone());
        self.model_pose.scene = st.scene.is_some();
        self.model_pose.requested = st.robot_urdf.is_some();
    }

    fn finish_model(&mut self, cx: &mut Cx) {
        let Some(ready) = self.model_loader.take_ready() else { return; };
        match ready {
            Ok(robot) => {
                let control_robot = robot.clone();
                let widget = self.view.widget(cx, ids!(stage_3d));
                if let Some(mut viewer) = widget.borrow_mut::<makepad_urdf_player::robot_view::RobotView>() {
                    let camera = (viewer.camera_target(), viewer.view_angles(), viewer.view_distance());
                    viewer.set_robot(cx, robot);
                    let pose = named_pose(&self.model_pose.names, &self.model_pose.joints, &viewer.movable_joint_names());
                    viewer.set_joint_angles(cx, &pose);
                    if let Some(base) = &self.model_pose.base { viewer.set_base_pose(cx, base.position, base.quat_xyzw); }
                    if self.model_pose.scene {
                        viewer.set_camera_target(cx, camera.0);
                        viewer.set_view_angles(cx, camera.1[0], camera.1[1]);
                        viewer.set_view_distance(cx, camera.2);
                    }
                }
                let widget = self.view.widget(cx, ids!(control_3d));
                if let Some(mut viewer) = widget.borrow_mut::<makepad_urdf_player::robot_view::RobotView>() {
                    viewer.set_robot(cx, control_robot);
                    let pose = named_pose(&self.model_pose.names, &self.model_pose.joints, &viewer.movable_joint_names());
                    viewer.set_joint_angles(cx, &pose);
                    viewer.set_camera_target(cx, [0., 0., 0.45]);
                    viewer.set_view_angles(cx, std::f32::consts::FRAC_PI_2, -0.2);
                    viewer.set_view_distance(cx, 1.35);
                }
                self.model_error = None;
            }
            Err(error) => self.model_error = Some(format!("Could not load robot model: {error}")),
        }
        self.sync_model_status(cx);
        self.view.redraw(cx);
    }

    fn sync_model_status(&self, cx: &mut Cx) {
        let waiting = !self.model_pose.requested || self.model_loader.loading() || self.model_error.is_some();
        self.view.widget(cx, ids!(stage_3d)).set_visible(cx, !waiting);
        self.view.widget(cx, ids!(no_robot)).set_visible(cx, waiting);
        self.view.label(cx, ids!(no_robot_label)).set_text(cx, self.model_error.as_deref().unwrap_or(if self.model_loader.loading() {
            "Loading robot model…"
        } else { "No robot model in this dataset" }));
    }

    fn update_signal_window(&mut self, cx: &mut Cx) {
        self.view
            .widget(cx, ids!(toolbar.signal_nav))
            .set_visible(cx, self.signals_open);
        if !self.signals_open {
            return;
        }
        let lanes = self.view.widget(cx, ids!(timeline.tl_body.lanes));
        let height = lanes.area().rect(cx).size.y;
        self.signal_rows = ((height / 45.0).floor() as usize).clamp(1, 6);
        let total = self.signal_order.len();
        let first = self
            .signal_first
            .min(total.saturating_sub(self.signal_rows));
        if self.signal_first != first {
            self.signal_first = first;
            self.scrolled = true;
        }
        for i in 0..LANE_IDS.len() {
            let path = format!("timeline.tl_body.lanes.content.lane_{i}");
            let ids: Vec<_> = path.split('.').map(LiveId::from_str).collect();
            self.view
                .widget(cx, &ids)
                .set_visible(cx, i < self.signal_rows && first + i < total);
        }
        self.view
            .label(cx, ids!(toolbar.signal_nav.signal_count))
            .set_text(
                cx,
                &if total == 0 {
                    "0 / 0".to_string()
                } else {
                    format!(
                        "{}–{} / {}",
                        first + 1,
                        (first + self.signal_rows).min(total),
                        total
                    )
                },
            );
        self.view
            .button(cx, ids!(toolbar.signal_nav.signal_prev))
            .set_enabled(cx, first > 0);
        self.view
            .button(cx, ids!(toolbar.signal_nav.signal_next))
            .set_enabled(cx, first + self.signal_rows < total);
    }
}

impl Widget for PlayScreen {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if matches!(event, Event::Signal) { self.finish_model(cx); }
        // The row window is driven here rather than by a ScrollY view: the rows
        // are fixed DSL widgets reused across a much longer list, so there is
        // nothing taller than the viewport for a scroll bar to move.
        // Footer resize. Driven from raw mouse events rather than the head's
        // own actions: a drag has to keep tracking after the pointer leaves
        // the resize strip it started in.
        match event {
            Event::MouseDown(e) => {
                let head = self.view.widget(cx, ids!(timeline.tl_head));
                if self.signals_open && !head.is_empty() && head.area().rect(cx).contains(e.abs) {
                    self.footer_drag = Some((e.abs.y, self.footer_h));
                }
            }
            Event::MouseMove(e) => {
                if let Some((y0, h0)) = self.footer_drag {
                    // Dragging up grows the footer, so the grip follows the
                    // pointer rather than running away from it.
                    let screen_h = self.view.area().rect(cx).size.y.max(FOOTER_MIN * 2.0);
                    let want = (h0 + (y0 - e.abs.y)).clamp(FOOTER_MIN, screen_h * FOOTER_MAX_FRAC);
                    if (want - self.footer_h).abs() > 0.5 {
                        self.footer_h = want;
                        // Set the walk directly. Height is a Size, not a
                        // number, so pushing a bare f64 through the script
                        // layer silently does nothing.
                        let card = self.view.widget(cx, ids!(timeline));
                        if let Some(mut card) = card.borrow_mut::<View>() {
                            card.walk.height = Size::Fixed(want);
                        }
                        self.view.redraw(cx);
                    }
                }
            }
            Event::MouseUp(_) => self.footer_drag = None,
            _ => {}
        }

        if let Event::Scroll(e) = event {
            let lanes = self.view.widget(cx, ids!(timeline.tl_body.lanes));
            if self.signals_open && lanes.area().rect(cx).contains(e.abs) && e.scroll.y.abs() > 0.0
            {
                let step = if e.scroll.y > 0.0 { 1 } else { -1 };
                self.signal_first = (self.signal_first as i64 + step).clamp(
                    0,
                    self.signal_order
                        .len()
                        .saturating_sub(self.signal_rows.max(1)) as i64,
                ) as usize;
                self.scrolled = true;
                self.update_signal_window(cx);
                self.view.redraw(cx);
                e.handled_y.set(true);
            }
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

/// Total width of the moving time window. Short episodes retain their full
/// trace while the cursor and sampled readouts follow playback.
const PLOT_WINDOW_S: f64 = 10.0;

fn matching_action<'a>(state: &PlotChannel, actions: &'a [PlotChannel]) -> Option<&'a PlotChannel> {
    actions.iter().find(|action| action.name == state.name)
}

fn signal_order(states: &[PlotChannel], actions: &[PlotChannel]) -> Vec<usize> {
    let moving = |channel: &PlotChannel| {
        let channel = matching_action(channel, actions).unwrap_or(channel);
        let (min, max) = channel
            .points
            .iter()
            .filter(|(_, v)| v.is_finite())
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(min, max), (_, v)| {
                (min.min(*v), max.max(*v))
            });
        max - min > 0.001
    };
    let mut order: Vec<_> = (0..states.len()).collect();
    // Show commanded motion first; retain every held channel for inspection.
    order.sort_by_key(|&i| (!moving(&states[i]), i));
    order
}

fn signal_value(value: Option<f64>) -> String {
    match value.filter(|v| v.is_finite()) {
        Some(v) => format!("{:.3}", if v.abs() < 0.0005 { 0.0 } else { v }),
        None => "—".into(),
    }
}

/// How far the footer can be dragged. The lanes share whatever is left after
/// the transport and scrubber, so the floor keeps them legible rather than
/// merely present.
const FOOTER_MIN: f64 = 288.0;
/// The ceiling is a share of the screen, not a constant: the inspector beside
/// the video does not scroll, so a footer tall enough to clip it would put the
/// curation buttons out of reach on a short window and waste room on a tall one.
const FOOTER_MAX_FRAC: f64 = 0.55;

/// Footer lanes, one per joint.
const LANE_IDS: [(&[LiveId], &[LiveId], &[LiveId]); 6] = [
    (
        ids!(timeline.tl_body.lanes.content.lane_0.plot_0),
        ids!(timeline.tl_body.lanes.content.lane_0.caption_0.name_0),
        ids!(timeline.tl_body.lanes.content.lane_0.caption_0.value_0),
    ),
    (
        ids!(timeline.tl_body.lanes.content.lane_1.plot_1),
        ids!(timeline.tl_body.lanes.content.lane_1.caption_1.name_1),
        ids!(timeline.tl_body.lanes.content.lane_1.caption_1.value_1),
    ),
    (
        ids!(timeline.tl_body.lanes.content.lane_2.plot_2),
        ids!(timeline.tl_body.lanes.content.lane_2.caption_2.name_2),
        ids!(timeline.tl_body.lanes.content.lane_2.caption_2.value_2),
    ),
    (
        ids!(timeline.tl_body.lanes.content.lane_3.plot_3),
        ids!(timeline.tl_body.lanes.content.lane_3.caption_3.name_3),
        ids!(timeline.tl_body.lanes.content.lane_3.caption_3.value_3),
    ),
    (
        ids!(timeline.tl_body.lanes.content.lane_4.plot_4),
        ids!(timeline.tl_body.lanes.content.lane_4.caption_4.name_4),
        ids!(timeline.tl_body.lanes.content.lane_4.caption_4.value_4),
    ),
    (
        ids!(timeline.tl_body.lanes.content.lane_5.plot_5),
        ids!(timeline.tl_body.lanes.content.lane_5.caption_5.name_5),
        ids!(timeline.tl_body.lanes.content.lane_5.caption_5.value_5),
    ),
];

/// Must track `EpisodeRow`'s height, so a wheel notch advances whole rows.
const ROW_H: f64 = 44.0;

const ROW_IDS: [&[LiveId]; 13] = [
    ids!(upper.tree.tree_body.row_0),
    ids!(upper.tree.tree_body.row_1),
    ids!(upper.tree.tree_body.row_2),
    ids!(upper.tree.tree_body.row_3),
    ids!(upper.tree.tree_body.row_4),
    ids!(upper.tree.tree_body.row_5),
    ids!(upper.tree.tree_body.row_6),
    ids!(upper.tree.tree_body.row_7),
    ids!(upper.tree.tree_body.row_8),
    ids!(upper.tree.tree_body.row_9),
    ids!(upper.tree.tree_body.row_10),
    ids!(upper.tree.tree_body.row_11),
    ids!(upper.tree.tree_body.row_12),
];
const GRP_IDS: [&[LiveId]; 3] = [
    ids!(upper.tree.tree_body.grp_0),
    ids!(upper.tree.tree_body.grp_1),
    ids!(upper.tree.tree_body.grp_2),
];

impl PlayScreenRef {
    pub fn signal_probe(&self, cx: &mut Cx) -> serde_json::Value {
        let Some(inner) = self.borrow() else {
            return serde_json::Value::Null;
        };
        let lanes: Vec<_> = LANE_IDS.iter().enumerate().map(|(slot, (plot, name, value))| {
            let r = inner.view.label(cx,value).area().rect(cx);
            serde_json::json!({
                "slot":slot, "visible":inner.signals_open && slot < inner.signal_rows && inner.signal_first + slot < inner.signal_order.len(),
                "name":inner.view.label(cx, name).text(),
                "readout":inner.view.label(cx, value).text(),
                "readout_rect": [r.pos.x,r.pos.y,r.size.x,r.size.y],
                "plot":inner.view.time_series_plot(cx, plot).probe(cx)
            })
        }).collect();
        serde_json::json!({"total":inner.signal_order.len(), "first":inner.signal_first, "rows":inner.signal_rows, "lanes":lanes})
    }

    pub fn shift_signals(&self, cx: &mut Cx, step: i64) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.signal_first = (inner.signal_first as i64 + step).clamp(
                0,
                inner
                    .signal_order
                    .len()
                    .saturating_sub(inner.signal_rows.max(1)) as i64,
            ) as usize;
            inner.scrolled = true;
            inner.update_signal_window(cx);
            inner.view.redraw(cx);
        }
    }

    pub fn toggle_pane(&self, cx: &mut Cx, pane: &str) {
        if let Some(mut inner) = self.borrow_mut() {
            match pane {
                "details" => inner.details_open = !inner.details_open,
                "episodes" => inner.episodes_open = !inner.episodes_open,
                "signals" => inner.signals_open = !inner.signals_open,
                _ => return,
            }
            inner.layout_width = -1.0;
        }
        self.adapt_layout(cx);
    }

    pub fn adapt_layout(&self, cx: &mut Cx) {
        let Some(mut inner) = self.borrow_mut() else {
            return;
        };
        if let Some(layout) = inner.view.flex_panels(cx, ids!(upper.grid)).snapshot() {
            if inner.toolbar_layout.as_ref() != Some(&layout) {
                inner.view.button(cx, ids!(layout_cycle)).set_text(
                    cx,
                    match layout.columns {
                        3 if inner.view.flex_panels(cx, ids!(upper.grid)).is_camera_layout() => "Layout: Camera",
                        1 => "Layout: Column",
                        4 | 5 => "Layout: Row",
                        _ => "Layout: Grid",
                    },
                );
                for (id, visible) in [
                    "robot_toggle",
                    "camera_a_toggle",
                    "camera_b_toggle",
                    "error_toggle",
                    "camera_c_toggle",
                ]
                .into_iter()
                .zip(layout.visible.iter().copied())
                {
                    let on = if visible { 1.0 } else { 0.0 };
                    let mut button = inner.view.widget(cx, &[LiveId::from_str(id)]);
                    script_apply_eval!(cx, button, {draw_bg +: {on: #(on)} draw_text +: {on: #(on)}});
                }
                inner.toolbar_layout = Some(layout);
            }
        }

        inner.update_signal_window(cx);
        let width = inner.view.area().rect(cx).size.x;
        if width <= 0.0 || (inner.layout_width - width).abs() < 1.0 {
            return;
        }
        inner.layout_width = width;
        let compact = width < 1080.0;
        let details = inner.details_open;
        let episodes = inner.episodes_open && !inner.live_mode && (!compact || !details);
        let signals = inner.signals_open;
        inner
            .view
            .widget(cx, ids!(upper.tree))
            .set_visible(cx, episodes);
        inner
            .view
            .widget(cx, ids!(upper.side))
            .set_visible(cx, details);
        inner
            .view
            .widget(cx, ids!(timeline.tl_body.lanes))
            .set_visible(cx, signals);
        inner
            .view
            .button(cx, ids!(toolbar.details_btn))
            .set_text(cx, if details { "Hide details" } else { "Details" });
        inner.view.button(cx, ids!(toolbar.episodes_btn)).set_text(
            cx,
            if episodes {
                "Hide episodes"
            } else {
                "Episodes"
            },
        );
        inner.view.button(cx, ids!(toolbar.signals_btn)).set_text(
            cx,
            if signals {
                "Hide signals"
            } else {
                "Show signals"
            },
        );
        let height = if signals {
            inner.footer_h.max(288.0)
        } else if inner.live_mode {
            52.0
        } else {
            104.0
        };
        if let Some(mut card) = inner.view.widget(cx, ids!(timeline)).borrow_mut::<View>() {
            card.walk.height = Size::Fixed(height);
        }
        inner.view.redraw(cx);
    }
    /// Theme changes reapply script defaults; restore the current pane choices.
    pub fn restore_panel_design(&self, cx: &mut Cx) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.layout_width = -1.0;
            inner.toolbar_layout = None;
            inner.loaded_signal_first = None;
        }
        self.adapt_layout(cx);
    }

    pub fn sync(&self, cx: &mut Cx, st: &PlaybackState) {
        let light = crate::ui::frame::light_mode();
        let Some(mut inner) = self.borrow_mut() else {
            return;
        };
        inner.capture_model_pose(st);
        if inner.model_loader.request(st.robot_urdf.clone()) {
            inner.model_error = None;
            for path in [ids!(stage_3d) as &[LiveId], ids!(control_3d)] {
                let widget = inner.view.widget(cx, path);
                if let Some(mut viewer) = widget.borrow_mut::<makepad_urdf_player::robot_view::RobotView>() { viewer.clear_robot(cx); };
            }
        } else { inner.finish_model(cx); }
        inner.sync_model_status(cx);
        if inner.live_mode != st.live.is_some() {
            inner.live_mode = st.live.is_some();
            inner.view.flex_panels(cx, ids!(upper.grid)).set_live_mode(cx, inner.live_mode);
            inner.toolbar_layout = None;
            inner.layout_width = -1.;
        }

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
        let need_media =
            inner.loaded_episode != st.selected || inner.loaded_dataset != st.dataset_name;
        if need_media {
            inner.loaded_episode = st.selected;
            inner.loaded_dataset = st.dataset_name.clone();
            inner.signal_order = signal_order(&st.state_series, &st.action_series);
            inner.signal_first = 0;
            inner.loaded_signal_first = None;
        }
        inner.update_signal_window(cx);
        let signal_indices: Vec<_> = inner
            .signal_order
            .iter()
            .skip(inner.signal_first)
            .take(LANE_IDS.len())
            .copied()
            .collect();
        let bind_signals =
            st.live.is_some() || inner.loaded_signal_first != Some(inner.signal_first);
        inner.loaded_signal_first = Some(inner.signal_first);

        let follow_robot = inner.follow_robot;
        let root = &mut inner.view;
        root.widget(cx, ids!(toolbar.episodes_btn)).set_visible(cx, st.live.is_none());
        root.button(cx, ids!(new_window)).set_text(cx, if st.live.is_some() { "Live window" } else { "Replay window" });

        script_apply_eval!(cx, root, { draw_bg +: { light: #(light) } });
        for id in [
            "episodes_btn",
            "signals_btn",
            "view_mode_btn",
            "details_btn",
            "panels_btn",
            "signal_prev",
            "signal_next",
            "robot_toggle",
            "camera_a_toggle",
            "camera_b_toggle",
            "error_toggle",
            "camera_c_toggle",
            "layout_cycle",
            "layout_reset",
            "new_window",
            "speed_btn",
        ] {
            let mut button = root.widget(cx, &[LiveId::from_str(id)]);
            script_apply_eval!(cx, button, {draw_bg +: {light: #(light)} draw_text +: {light: #(light)}});
        }

        apply_light_in(
            cx,
            root,
            &[
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
            ],
            light,
        );
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
        let sky = if light > 0.5 {
            dorobot_ux::tokens::pal::VOID_L
        } else {
            dorobot_ux::tokens::pal::DEEP_D
        };
        let ground = if light > 0.5 {
            dorobot_ux::tokens::pal::VOID_L
        } else {
            dorobot_ux::tokens::pal::LIFT_D
        };
        for path in [ids!(stage_3d) as &[LiveId], ids!(control_3d)] {
            let mut robot = root.widget(cx, path);
            script_apply_eval!(cx, robot, {sky_horizon: #(sky) sky_zenith: #(sky) ground_color: #(ground) ground2_color: #(ground)});
        }
        let dark_mode = 1.0 - light;
        makepad_app_shell::theme::set_global_dark_mode(dark_mode);
        for (_, name_id, _) in LANE_IDS {
            let mut label = root.widget(cx, name_id);
            script_apply_eval!(cx, label, { draw_text +: { light: #(light) } });
        }
        for (path, message) in [
            (
                ids!(stage_a.video_area.placeholder.placeholder_label) as &[LiveId],
                "No recording",
            ),
            (
                ids!(stage_b.video_area.placeholder.placeholder_label),
                "No recording",
            ),
            (
                ids!(stage_c.video_area.placeholder.placeholder_label),
                "No recording",
            ),
        ] {
            root.label(cx, path).set_text(cx, if st.live.is_some() { "Waiting for stream" } else { message });
        }

        let hardware_camera = root.flex_panels(cx, ids!(upper.grid)).is_hardware_camera();
        root.label(cx, ids!(robot_panel.head.title)).set_text(cx,
            if hardware_camera { "Trace map" } else { "3D view" });
        root.button(cx, ids!(robot_toggle)).set_text(cx,
            if hardware_camera { "Trace map" } else { "3D view" });
        root.label(cx, ids!(error_panel.head.title)).set_text(cx,
            if hardware_camera { "3D joints" } else { "Tracking error" });
        root.button(cx, ids!(error_toggle)).set_text(cx,
            if hardware_camera { "3D joints" } else { "Tracking error" });
        root.widget(cx, ids!(error_panel.stage_plot)).set_visible(cx, !hardware_camera);
        root.widget(cx, ids!(error_panel.control_body)).set_visible(cx, hardware_camera);
        root.widget(cx, ids!(view_mode_btn)).set_visible(cx,
            !hardware_camera && st.scene.as_ref().is_some_and(|scene| scene.version == 2));
        root.button(cx, ids!(view_mode_btn)).set_text(cx,
            if follow_robot { "View: Follow" } else { "View: Room" });

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
                    Some(if groups.len() == 1 {
                        range
                    } else {
                        range + "  ·  all tasks"
                    })
                }
                (true, _) => None,
                (false, 0) => Some(if groups.len() == 1 {
                    groups[0].to_string()
                } else {
                    format!("{} episodes · {} tasks", total, groups.len())
                }),
                (false, _) => None,
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
                    let sel = if st.selected == Some(e.index) {
                        1.0
                    } else {
                        0.0
                    };
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
            (
                ids!(upper.side.side_body.st_frames) as &[LiveId],
                format!("{}", s.frames),
                0.0,
            ),
            (
                ids!(upper.side.side_body.st_dur),
                format!("{:.1}s", s.duration_s),
                0.0,
            ),
            (
                ids!(upper.side.side_body.st_fps),
                format!("{}", s.fps as u32),
                0.0,
            ),
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
        root.widget(cx, ids!(timeline.tl_body.controls.drift_lbl))
            .set_visible(cx, s.drift_frames.abs() > 0.2);
        root.label(cx, ids!(upper.side.side_body.task_t))
            .set_text(cx, &s.task);

        // Curation buttons carry the current tag, so the answer to "is this one
        // marked?" is where the marking happens, not only in the far-left list.
        for path in [
            ids!(upper.side.side_body.btn_good) as &[LiveId],
            ids!(upper.side.side_body.btn_bad),
            ids!(upper.side.side_body.btn_del),
        ] {
            root.button(cx, path).set_enabled(cx, st.live.is_none());
        }
        let tag_now = st
            .selected
            .and_then(|i| st.episodes.iter().find(|e| e.index == i))
            .and_then(|e| e.tag);
        for (path, want) in [
            (ids!(upper.side.side_body.btn_good) as &[LiveId], Tag::Good),
            (ids!(upper.side.side_body.btn_bad), Tag::Bad),
        ] {
            let on = if tag_now == Some(want) { 1.0 } else { 0.0 };
            let mut b = root.widget(cx, path);
            script_apply_eval!(cx, b, { draw_bg +: { sel: #(on) } draw_text +: { sel: #(on) } });
        }

        // ---- plot: measured vs commanded for the selected episode ----------
        // The plot reads the app-shell theme global rather than our light
        // instance, so the two are kept in step here.
        makepad_app_shell::theme::set_global_dark_mode(1.0 - light);

        // One lane per joint: measurement and command together, each lane on
        // its own scale. Rebuilt only when the episode changes, since this
        // copies every sample and sync runs on each transport tick.
        let plot = root.time_series_plot(cx, ids!(stage_plot));
        for (_, name_id, value_id) in LANE_IDS.iter() {
            apply_light_in(
                cx,
                root,
                &[(name_id, Themed::Text), (value_id, Themed::Text)],
                light,
            );
        }
        if bind_signals {
            for (slot, (plot_id, name_id, _)) in LANE_IDS.iter().enumerate() {
                let lane = root.time_series_plot(cx, plot_id);
                lane.clear();
                let channel = signal_indices
                    .get(slot)
                    .and_then(|&i| st.state_series.get(i));
                root.label(cx, name_id).set_text(
                    cx,
                    channel
                        .map(|c| c.name.trim_start_matches("main_"))
                        .unwrap_or(""),
                );
                if let Some(c) = channel {
                    lane.set_channel_data(0, &c.name, c.points.clone());
                    if let Some(target) = matching_action(c, &st.action_series) {
                        lane.set_channel_data(
                            1,
                            &format!("{} target", target.name),
                            target.points.clone(),
                        );
                    }
                }
                lane.set_time_range(0.0, st.stats.duration_s);
                lane.set_auto_scale_y(true);
                lane.set_window_size(PLOT_WINDOW_S);
                lane.recompute_scale(cx);
            }
        }
        if need_media || st.live.is_some() {
            // The grid pane keeps a view the lanes cannot give: how far the
            // arm was from what it was told, all joints against one zero.
            plot.clear();
            for (i, st_c) in st.state_series.iter().enumerate() {
                let Some(act) = matching_action(st_c, &st.action_series) else {
                    continue;
                };
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
        Self::sync_camera_titles(cx, root, st);
        if need_media {
            Self::load_media(cx, root, st, follow_robot);
        }
        Self::follow_playhead(cx, root, st, &signal_indices, follow_robot);

        inner.row_episode = row_map;
    }

    /// Everything that depends on the playhead, and nothing that does not.
    ///
    /// The transport ticks at 60Hz; running a whole `sync` there costs more
    /// than the frame budget (every row re-evaluated, every theme re-applied)
    /// and playback visibly runs slow as a result.
    fn follow_playhead(cx: &mut Cx, root: &mut View, st: &PlaybackState, signal_indices: &[usize], follow_robot: bool) {
        let frames = st.stats.frames.max(1);
        let frame_idx = st.frame_index();

        for (path, name) in [
            (ids!(stage_a) as &[LiveId], "head"),
            (ids!(stage_b), "left_wrist"),
            (ids!(stage_c), "right_wrist"),
        ] {
            let player = root.video_player(cx, path);
            if player.borrow().is_some() {
                if let Some(live) = &st.live {
                    if let Some(image) = live.images.get(name) {
                        player.set_frame_info(cx, live.sequence, live.sequence + 1);
                        player.set_live_frame(cx, image.width, image.height, &image.bgra);
                    }
                } else {
                    player.show_frame_at_time(cx, st.current_time);
                    player.set_frame_info(cx, frame_idx, frames);
                }
            }
        }

        // The 3D mirror follows the same playhead, which is what the shipping
        // player never did — its robot view did not move during playback.
        let hardware_camera = root.flex_panels(cx, ids!(upper.grid)).is_hardware_camera();
        let viewer = root.widget(cx, ids!(stage_3d));
        if let Some(joints) = st.joints_now() {
            if let Some(mut rv) = viewer.borrow_mut::<makepad_urdf_player::robot_view::RobotView>()
            {
                // Already in URDF convention: the backend owns that mapping,
                // because it is a property of the robot, not of the view.
                let names = rv.movable_joint_names();
                let pose = named_pose(&st.state_names, joints, &names);
                rv.set_joint_angles(cx, &pose);
                if let Some(scene) = &st.scene {
                    if let Some(base) = scene
                        .frames
                        .get(frame_idx as usize)
                        .and_then(|f| f.base_pose.as_ref())
                    {
                        rv.set_base_pose(cx, base.position, base.quat_xyzw);
                        if follow_robot && !hardware_camera {
                            rv.set_camera_target(cx, [base.position[0], base.position[1], base.position[2] + 0.3]);
                        }
                    }
                    if hardware_camera {
                        super::replay_scene::show_trace(cx, &mut rv, scene, frame_idx as usize);
                    } else {
                        super::replay_scene::show(cx, &mut rv, scene, frame_idx as usize);
                    }
                }
            }
        }

        if hardware_camera {
            Self::sync_control_model(cx, root, st);
        }
        root.time_series_plot(cx, ids!(stage_plot))
            .set_cursor_time(cx, st.current_time);
        for (slot, (plot_id, _, value_id)) in LANE_IDS.iter().enumerate() {
            root.time_series_plot(cx, plot_id)
                .set_cursor_time(cx, st.current_time);
            let channel = signal_indices
                .get(slot)
                .and_then(|&i| st.state_series.get(i));
            let text = channel
                .map(|c| {
                    let actual = c.points.get(frame_idx as usize).map(|p| p.1);
                    let target = matching_action(c, &st.action_series)
                        .and_then(|a| a.points.get(frame_idx as usize))
                        .map(|p| p.1);
                    format!(
                        "Actual {} · Target {}",
                        signal_value(actual),
                        signal_value(target)
                    )
                })
                .unwrap_or_default();
            root.label(cx, value_id).set_text(cx, &text);
        }

        root.widget(cx, ids!(timeline.tl_body.controls.transport))
            .set_visible(cx, st.live.is_none());
        root.widget(cx, ids!(timeline.tl_body.controls.speed_btn))
            .set_visible(cx, st.live.is_none());
        root.widget(cx, ids!(timeline.tl_body.tl))
            .set_visible(cx, st.live.is_none());
        let tl = root.timeline(cx, ids!(timeline.tl_body.tl));
        tl.set_duration(cx, st.stats.duration_s, st.stats.fps);
        tl.set_current_time(cx, st.current_time);
        tl.set_playing(cx, st.is_playing);
        tl.set_speed(cx, st.speed);
        root.button(cx, ids!(timeline.tl_body.controls.speed_btn))
            .set_text(cx, &format!("Speed {}×", st.speed));
        root.playback_controls(cx, ids!(timeline.tl_body.controls.transport))
            .set_playing(cx, st.is_playing);
        root.label(cx, ids!(timeline.tl_body.controls.frame_lbl))
            .set_text(
                cx,
                &if let Some(live) = &st.live {
                    if st.stats.frames == 0 { "Waiting for first frame".into() } else { format!(
                        "{} · {:.1}s · frame {}",
                        if live.finished { "ENDED" } else if live.connected { "LIVE" } else { "LAST FRAME" },
                        st.current_time,
                        live.sequence
                    ) }
                } else {
                    format!("{} / {}", frame_idx.min(frames), frames)
                },
            );
    }

    fn sync_control_model(cx: &mut Cx, root: &mut View, st: &PlaybackState) {
        let widget = root.widget(cx, ids!(control_3d));
        let (selected, active) = control_joints(st);
        let mut limits = Vec::new();
        if let Some(mut viewer) = widget.borrow_mut::<makepad_urdf_player::robot_view::RobotView>() {
            // Keep the inspection model in the robot's own frame. Never reset
            // its orbit or zoom when a live joint sample arrives.
            if let Some(joints) = st.joints_now() {
                let pose = named_pose(&st.state_names, joints, &viewer.movable_joint_names());
                viewer.set_joint_angles(cx, &pose);
            }
            viewer.set_joint_highlights(cx, &selected, &active);
            if let Some(robot) = viewer.robot() { limits = control_limits(st, robot); }
        }
        let readout = arm_joint_readout(st);
        root.widget(cx, ids!(control_angles)).set_visible(cx, readout.is_some());
        root.label(cx, ids!(control_angles)).set_text(cx, readout.as_deref().unwrap_or(""));
        let label = if !limits.is_empty() && !active.is_empty() {
            format!("Moving: {} · Near limit: {}",
                active.iter().map(|n| short_joint_name(n)).collect::<Vec<_>>().join(", "),
                limits.iter().map(|l| short_joint_name(&l.name)).collect::<Vec<_>>().join(", "))
        } else if !limits.is_empty() {
            format!("Near limit: {}", limits.iter().map(|l| format!("{} {:+.0}°", short_joint_name(&l.name), l.angle.to_degrees())).collect::<Vec<_>>().join(" · "))
        } else if active.is_empty() {
            "Blue select · Yellow move".to_owned()
        } else {
            format!("Moving: {}", active.iter().map(|name| joint_caption(name)).collect::<Vec<_>>().join(" · "))
        };
        root.label(cx, ids!(control_feedback)).set_text(cx, &label);
        root.label(cx, ids!(control_hint)).set_text(cx, &if limits.is_empty() {
            "Drag: orbit · Scroll: zoom".into()
        } else {
            format!("L1 + {}", limits.iter().map(|l| limit_release_control(&l.name, l.upper)).collect::<Vec<_>>().join(" · "))
        });
    }

    /// Transport tick: advance the views the playhead drives, nothing else.
    pub fn tick(&self, cx: &mut Cx, st: &PlaybackState) {
        let Some(mut inner) = self.borrow_mut() else {
            return;
        };
        inner.capture_model_pose(st);
        inner.finish_model(cx);
        let signal_indices: Vec<_> = inner
            .signal_order
            .iter()
            .skip(inner.signal_first)
            .take(LANE_IDS.len())
            .copied()
            .collect();
        let follow_robot = inner.follow_robot;
        let root = &mut inner.view;
        Self::follow_playhead(cx, root, st, &signal_indices, follow_robot);
    }

    /// Each replay window keeps its own overview/follow choice. Orbit and zoom
    /// remain user-controlled while following the recorded base position.
    pub fn toggle_room_view(&self, cx: &mut Cx, st: &PlaybackState) {
        if !st.scene.as_ref().is_some_and(|scene| scene.version == 2) { return; }
        let Some(mut inner) = self.borrow_mut() else { return; };
        inner.follow_robot = !inner.follow_robot;
        let follow = inner.follow_robot;
        let widget = inner.view.widget(cx, ids!(stage_3d));
        if let Some(mut rv) = widget.borrow_mut::<makepad_urdf_player::robot_view::RobotView>() {
            rv.set_camera_target(cx, [0., 1., 0.3]);
            rv.set_view_angles(cx, -0.95, if follow { -0.65 } else { -0.85 });
            rv.set_view_distance(cx, if follow { 2.6 } else { 9.0 });
        }
        drop(inner);
        self.sync(cx, st);
    }

    // Live styles can reapply the generic labels without changing the episode.
    // Restore the dataset's camera names without reopening its video streams.
    fn sync_camera_titles(cx: &mut Cx, root: &mut View, st: &PlaybackState) {
        let slots = if st.live.is_some() {
            [
                Some("head".into()),
                Some("left_wrist".into()),
                Some("right_wrist".into()),
            ]
        } else {
            camera_slots(st.video_paths.keys().map(String::as_str))
        };
        for ((panel, toggle), key) in [
            (
                ids!(camera_a_panel.head.title) as &[LiveId],
                ids!(camera_a_toggle) as &[LiveId],
            ),
            (ids!(camera_b_panel.head.title), ids!(camera_b_toggle)),
            (ids!(camera_c_panel.head.title), ids!(camera_c_toggle)),
        ]
        .into_iter()
        .zip(slots)
        {
            let mut title = key
                .as_deref()
                .map(camera_title)
                .unwrap_or("No camera".into());
            if root.flex_panels(cx, ids!(upper.grid)).is_hardware_camera() {
                title = match key.as_deref() {
                    Some("head") => "Front camera".into(),
                    Some("left_wrist") => "Left arm".into(),
                    Some("right_wrist") => "Right arm".into(),
                    _ => title,
                };
            }
            root.label(cx, panel).set_text(cx, &title);
            root.button(cx, toggle).set_text(cx, &title);
        }
    }

    /// Point the three camera panes and the robot at this episode's media.
    ///
    /// Camera keys are matched by preference and then by position, the way the
    /// shipping player does it, so a dataset naming its cameras `top`/`wrist`
    /// lands in the same panes as one naming them `cam_high`/`cam_left_wrist`.
    fn load_media(cx: &mut Cx, root: &mut View, st: &PlaybackState, follow_robot: bool) {
        let [main, wrist, third] = if st.live.is_some() {
            [
                Some("head".into()),
                Some("left_wrist".into()),
                Some("right_wrist".into()),
            ]
        } else {
            camera_slots(st.video_paths.keys().map(String::as_str))
        };
        let has_third = third.is_some();
        root.flex_panels(cx, ids!(upper.grid))
            .set_panel_available(cx, 4, has_third);
        root.widget(cx, ids!(camera_c_toggle))
            .set_visible(cx, has_third);
        for (path, key) in [
            (ids!(stage_a) as &[LiveId], main),
            (ids!(stage_b), wrist),
            (ids!(stage_c), third),
        ] {
            let player = root.video_player(cx, path);
            if player.borrow().is_none() {
                continue;
            }
            player.clear(cx);
            let Some(key) = key else { continue };
            // A second pane showing the same file is worse than an empty one.
            player.set_episode_info(
                st.video_frame_offsets
                    .get(&key)
                    .copied()
                    .unwrap_or(st.video_frame_offset),
                st.stats.frames,
            );
            player.set_fps_display(cx, st.stats.fps);
            if let Some(file) = st.video_paths.get(&key) {
                if let Err(e) = player.load_video(cx, &file.to_string_lossy()) {
                    ::log::error!("play: video {key} failed to load: {e}");
                }
            }
        }

        let hardware_camera = root.flex_panels(cx, ids!(upper.grid)).is_hardware_camera();
        let viewer = root.widget(cx, ids!(stage_3d));
        if st.robot_urdf.is_some() {
            if let Some(mut rv) = viewer.borrow_mut::<makepad_urdf_player::robot_view::RobotView>()
            {
                rv.clear_external(cx);
                rv.set_base_pose(cx, [0., 0., 0.], [0., 0., 0., 1.]);
                if let Some(scene) = &st.scene {
                    super::replay_scene::load(cx, &mut rv);
                    // Frame a new scene once, leaving orbit and zoom to the viewer.
                    let (target, yaw, pitch, distance) = if hardware_camera {
                        ([0.0, 0.0, 0.0], 0.0, -1.55, 16.0)
                    } else if scene.version == 2 {
                        let (pitch, distance) = if follow_robot { (-0.65, 2.6) } else { (-0.85, 9.0) };
                        ([0.0, 1.0, 0.3], -0.95, pitch, distance)
                    } else {
                        ([0.32, 0.0, 0.24], -0.95, -0.42, 1.15)
                    };
                    rv.set_camera_target(cx, target);
                    rv.set_view_angles(cx, yaw, pitch);
                    rv.set_view_distance(cx, distance);
                }
            }
        }
    }

    /// True once if the wheel moved the row window, so the app re-syncs it.
    pub fn take_scrolled(&self) -> bool {
        let Some(mut inner) = self.borrow_mut() else {
            return false;
        };
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
            return Some(Intent::TagEpisode {
                episode,
                tag: Some(Tag::Good),
            });
        }
        if hit(v, cx, ids!(upper.side.side_body.btn_bad)) {
            return Some(Intent::TagEpisode {
                episode,
                tag: Some(Tag::Bad),
            });
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

/// Derive annotations from the same guarded target stream as the cameras.
/// Motion of measured joints alone (settling/collisions) is not a command.
fn control_joints(st: &PlaybackState) -> (Vec<String>, Vec<String>) {
    let phase = &st.stats.task;
    let Some(live) = &st.live else { return (Vec::new(), Vec::new()); };
    let prefix = if phase.contains("· Left arm ·") { "left_" }
        else if phase.contains("· Right arm ·") { "right_" }
        else if phase.contains("· Head ·") { "head_" }
        else if phase.contains("· Drive ·") { "wheel_" }
        else { return (Vec::new(), Vec::new()); };
    let selected: Vec<_> = st.state_names.iter().filter(|name| name.starts_with(prefix)).cloned().collect();
    let moving = live.connected && !live.finished && phase.ends_with("· Moving");
    let active = if moving {
        st.action_series.iter().filter(|channel| {
            selected.contains(&channel.name) && channel.points.len() >= 2 && {
                let n = channel.points.len();
                let a = channel.points[n - 2];
                let b = channel.points[n - 1];
                b.0 > a.0 && (b.1 - a.1).abs() > 0.0001
            }
        }).map(|channel| channel.name.clone()).collect()
    } else { Vec::new() };
    (selected, active)
}

fn joint_caption(name: &str) -> String {
    let label = name.trim_start_matches("left_").trim_start_matches("right_")
        .replace("_joint", "").replace('_', " ");
    label.replace("j1", "J1").replace("j2", "J2").replace("j3", "J3")
        .replace("j4", "J4").replace("j5", "J5").replace("j6", "J6")
}

pub const ARM_CONTROL_MAP: &str = "L1: hold · R1: mode · Left stick: J1 ↔ J2 ↕ · Right stick: J3 ↕ J4 ↔ · D-pad ↔: J5 roll · ×/○: J6 gripper";

fn arm_joint_readout(st: &PlaybackState) -> Option<String> {
    let side = if st.stats.task.contains("· Left arm ·") { "left" }
        else if st.stats.task.contains("· Right arm ·") { "right" }
        else { return None; };
    let values = st.joints_now()?;
    let suffixes = ["j1_yaw", "j2_pitch", "j3_pitch", "j4_pitch", "j5_roll", "j6_gripper"];
    let labels: Vec<_> = suffixes.iter().enumerate().map(|(i, suffix)| {
        let name = format!("{side}_{suffix}");
        let angle = st.state_names.iter().position(|n| n == &name)
            .and_then(|index| values.get(index)).filter(|v| v.is_finite());
        format!("J{} {}", i+1, angle.map_or_else(|| "—".into(), |v| format!("{:+.0}°", v.to_degrees())))
    }).collect();
    Some(format!("{} · {}\n{}", if side == "left" { "Left" } else { "Right" }, labels[..3].join(" · "), labels[3..].join(" · ")))
}

struct ControlLimit {
    name: String,
    angle: f32,
    bound: f32,
    upper: bool,
}

fn short_joint_name(name: &str) -> String {
    let name = name.trim_start_matches("left_").trim_start_matches("right_");
    if name.starts_with('j') { name.split('_').next().unwrap_or(name).to_uppercase() }
    else { joint_caption(name) }
}

fn control_limits(st: &PlaybackState, robot: &makepad_urdf_player::robot::Robot) -> Vec<ControlLimit> {
    if !st.live.as_ref().is_some_and(|live| live.connected && !live.finished) { return Vec::new(); }
    let (selected, _) = control_joints(st);
    robot.joints.iter().filter_map(|joint| {
        if !selected.contains(&joint.name)
            || joint.joint_type != makepad_urdf_player::robot::JointType::Revolute
            || !joint.angle.is_finite() || !joint.limit_lower.is_finite() || !joint.limit_upper.is_finite()
            || joint.limit_upper <= joint.limit_lower { return None; }
        let threshold = 2.0f32.to_radians();
        let upper = if joint.angle >= joint.limit_upper - threshold { true }
            else if joint.angle <= joint.limit_lower + threshold { false }
            else { return None; };
        Some(ControlLimit { name: joint.name.clone(), angle: joint.angle,
            bound: if upper { joint.limit_upper } else { joint.limit_lower }, upper })
    }).collect()
}

fn limit_release_control(name: &str, upper: bool) -> &'static str {
    match (name.trim_start_matches("left_").trim_start_matches("right_"), upper) {
        ("j1_yaw", true) => "left stick ←", ("j1_yaw", false) => "left stick →",
        ("j2_pitch", true) => "left stick ↓", ("j2_pitch", false) => "left stick ↑",
        ("j3_pitch", true) => "right stick ↓", ("j3_pitch", false) => "right stick ↑",
        ("j4_pitch", true) => "right stick ←", ("j4_pitch", false) => "right stick →",
        ("j5_roll", true) => "D-pad ←", ("j5_roll", false) => "D-pad →",
        ("j6_gripper", true) => "×", ("j6_gripper", false) => "○",
        ("head_pan_joint", true) => "left stick →", ("head_pan_joint", false) => "left stick ←",
        ("head_tilt_joint", true) => "left stick ↓", ("head_tilt_joint", false) => "left stick ↑",
        _ => "reverse input",
    }
}

/// Uses the loaded URDF's actual limits and streamed measurements; no altered
/// limits, inferred motor commands, or simulated motion in the UI.
pub fn control_limit_hint(st: &PlaybackState, robot: &makepad_urdf_player::robot::Robot) -> Option<String> {
    let limits = control_limits(st, robot);
    if limits.is_empty() { return None; }
    let detail = limits.iter().take(2).map(|l| format!("{} {:+.0}° / {:+.0}°: {}",
        short_joint_name(&l.name), l.angle.to_degrees(), l.bound.to_degrees(), limit_release_control(&l.name, l.upper)))
        .collect::<Vec<_>>().join(" · ");
    Some(format!("Near limits · {detail}{} · Hold L1 · R1: mode",
        if limits.len() > 2 { format!(" · +{} more", limits.len()-2) } else { String::new() }))
}

fn camera_slots<'a>(keys: impl Iterator<Item = &'a str>) -> [Option<String>; 3] {
    let mut remaining: Vec<_> = keys.map(str::to_owned).collect();
    remaining.sort();
    let mut take = |aliases: &[&str]| {
        let index = aliases
            .iter()
            .find_map(|alias| remaining.iter().position(|k| k.contains(alias)))
            .or_else(|| (!remaining.is_empty()).then_some(0));
        index.map(|i| remaining.remove(i))
    };
    [
        take(&["head", "cam_high", "top", "front"]),
        take(&[
            "left_wrist",
            "wrist_left",
            "cam_left",
            "wrist",
            "cam_low",
            "side",
        ]),
        take(&["right_wrist", "wrist_right", "cam_right"]),
    ]
}

fn camera_title(key: &str) -> String {
    let name = key.rsplit('.').next().unwrap_or(key);
    match name {
        "head" => "Head camera".into(),
        "left_wrist" => "Left wrist".into(),
        "right_wrist" => "Right wrist".into(),
        _ => name.replace('_', " "),
    }
}

/// Dataset-defined camera names are data; only Nano's built-in captions are UI.
pub fn custom_camera_caption_paths(st: &PlaybackState) -> Vec<&'static str> {
    if st.live.is_some() { return Vec::new(); }
    camera_slots(st.video_paths.keys().map(String::as_str)).into_iter().zip([
        ["camera_a_panel.head.title", "camera_a_toggle"],
        ["camera_b_panel.head.title", "camera_b_toggle"],
        ["camera_c_panel.head.title", "camera_c_toggle"],
    ]).filter(|(key, _)| key.as_deref().is_some_and(|key|
        !matches!(key.rsplit('.').next(), Some("head" | "left_wrist" | "right_wrist"))))
        .flat_map(|(_, paths)| paths).collect()
}

#[cfg(test)]
mod camera_tests {
    use super::*;
    #[test]
    fn selects_all_nano_cameras_once_regardless_of_key_order() {
        assert_eq!(
            camera_slots(
                [
                    "observation.images.right_wrist",
                    "observation.images.head",
                    "observation.images.left_wrist"
                ]
                .into_iter()
            ),
            [
                Some("observation.images.head".into()),
                Some("observation.images.left_wrist".into()),
                Some("observation.images.right_wrist".into())
            ]
        );
    }
    #[test]
    fn one_camera_is_never_duplicated_and_generic_keys_are_stable() {
        assert_eq!(
            camera_slots(["front"].into_iter()),
            [Some("front".into()), None, None]
        );
        assert_eq!(
            camera_slots(["c", "a", "b"].into_iter()),
            [Some("a".into()), Some("b".into()), Some("c".into())]
        );
        assert_eq!(camera_slots([].into_iter()), [None, None, None]);
        let mut state = PlaybackState::default();
        state.video_paths.insert("observation.images.Record".into(), "record.mp4".into());
        state.video_paths.insert("observation.images.head".into(), "head.mp4".into());
        assert_eq!(custom_camera_caption_paths(&state), ["camera_b_panel.head.title", "camera_b_toggle"]);
    }
}

// Named datasets can order joints differently from the loaded URDF. Keep the
// historical positional convention for datasets with generic channel labels.
fn named_pose(source: &[String], values: &[f32], target: &[String]) -> Vec<f32> {
    let indices: Option<Vec<_>> = target
        .iter()
        .map(|name| source.iter().position(|s| s == name))
        .collect();
    match indices {
        Some(indices) if indices.iter().all(|i| *i < values.len()) => {
            indices.iter().map(|i| values[*i]).collect()
        }
        _ => values.to_vec(),
    }
}

#[cfg(test)]
mod pose_tests {
    use super::*;
    #[test]
    fn named_pose_follows_urdf_identity_not_dataset_column_order() {
        assert_eq!(
            named_pose(
                &["right".into(), "left".into()],
                &[0.7, -0.3],
                &["left".into(), "right".into()]
            ),
            [-0.3, 0.7]
        );
        assert_eq!(
            named_pose(&["channel_0".into()], &[0.7], &["joint".into()]),
            [0.7]
        );
    }
}

#[cfg(test)]
mod signal_tests {
    use super::*;

    #[test]
    fn moving_joints_come_first_without_losing_held_channels() {
        let states: Vec<_> = (0..17)
            .map(|i| PlotChannel {
                name: format!("joint_{i}"),
                points: vec![(0.0, 0.0), (0.1, 0.00001)],
            })
            .collect();
        let mut actions = states.clone();
        actions[5].points[1].1 = 0.1;
        actions[16].points[1].1 = 0.2;
        actions.reverse(); // Pair commands by identity, never by column position.
        let order = signal_order(&states, &actions);
        assert_eq!(&order[..2], &[5, 16]);
        let mut all = order.clone();
        all.sort();
        assert_eq!(all, (0..17).collect::<Vec<_>>());
        assert_eq!(
            matching_action(&states[5], &actions).unwrap().points[1].1,
            0.1
        );
    }

    #[test]
    fn missing_commands_do_not_hide_measured_motion_or_invent_a_target() {
        let states = vec![PlotChannel {
            name: "left".into(),
            points: vec![(0.0, 0.0), (1.0, 1.0)],
        }];
        assert_eq!(signal_order(&states, &[]), [0]);
        assert!(matching_action(&states[0], &[]).is_none());
        assert_eq!(signal_value(None), "—");
        assert_eq!(signal_value(Some(-1e-8)), "0.000");
    }
}

#[cfg(test)]
mod control_joint_tests {
    use super::*;

    #[test]
    fn both_arm_readouts_keep_all_six_joint_numbers_and_match_by_name() {
        let mut state = crate::api::live::empty_preview();
        state.stats.frames = 1;
        state.stats.fps = 5.;
        let suffixes = ["j1_yaw", "j2_pitch", "j3_pitch", "j4_pitch", "j5_roll", "j6_gripper"];
        let mut values = Vec::new();
        for side in ["right", "left"] {
            for (i, suffix) in suffixes.iter().enumerate().rev() {
                state.state_names.push(format!("{side}_{suffix}"));
                values.push(((i+1) as f32 * if side == "left" { 10. } else { -10. }).to_radians());
            }
        }
        state.joint_frames = vec![values];
        state.stats.task = "Preview only · Gamepad · Left arm · Hold L1 to move".into();
        assert_eq!(arm_joint_readout(&state).unwrap(), "Left · J1 +10° · J2 +20° · J3 +30°\nJ4 +40° · J5 +50° · J6 +60°");
        state.stats.task = "Preview only · Gamepad · Right arm · Moving".into();
        assert_eq!(arm_joint_readout(&state).unwrap(), "Right · J1 -10° · J2 -20° · J3 -30°\nJ4 -40° · J5 -50° · J6 -60°");
    }

    fn limit_robot(name: &str, degrees: f32) -> makepad_urdf_player::robot::Robot {
        use makepad_urdf_player::robot::{Robot, RobotJoint, JointType};
        let mut robot = Robot::new("test".into());
        let mut joint = RobotJoint::new(name.into(), "base".into(), "arm".into());
        joint.joint_type = JointType::Revolute;
        joint.angle = degrees.to_radians();
        joint.limit_lower = -150.0f32.to_radians();
        joint.limit_upper = 150.0f32.to_radians();
        robot.joints.push(joint);
        robot
    }

    #[test]
    fn limits_explain_clamped_targets_even_when_the_controller_is_released() {
        let mut state = moving_arm();
        state.stats.task = "Preview only · Gamepad · Left arm · Hold L1 to move".into();
        let robot = limit_robot("left_j3_pitch", 150.);
        let hint = control_limit_hint(&state, &robot).unwrap();
        assert!(hint.contains("J3 +150° / +150°: right stick ↓"));
        assert!(control_joints(&state).1.is_empty());
        assert!(control_limit_hint(&state, &limit_robot("left_j3_pitch", 140.)).is_none());
        assert!(control_limit_hint(&state, &limit_robot("right_j1_yaw", 150.)).is_none());
    }

    #[test]
    fn limits_use_urdf_bounds_and_the_correct_reverse_input() {
        let state = moving_arm();
        let mut robot = limit_robot("left_j3_pitch", -90.);
        robot.joints[0].limit_lower = -90.0f32.to_radians();
        assert!(control_limit_hint(&state, &robot).unwrap().contains("-90° / -90°: right stick ↑"));
        assert_eq!(limit_release_control("left_j2_pitch", true), "left stick ↓");
        assert_eq!(limit_release_control("right_j2_pitch", true), "left stick ↓");
        assert_eq!(limit_release_control("right_j1_yaw", false), "left stick →");
        let mut stale = state;
        stale.live.as_mut().unwrap().connected = false;
        assert!(control_limit_hint(&stale, &robot).is_none());
    }

    fn moving_arm() -> PlaybackState {
        let mut state = crate::api::live::empty_preview();
        state.live.as_mut().unwrap().connected = true;
        state.stats.task = "Preview only · Gamepad · Left arm · Moving".into();
        state.state_names = vec!["left_j1_yaw".into(), "left_j3_pitch".into(), "right_j1_yaw".into()];
        state.action_series = state.state_names.iter().enumerate().map(|(i, name)| PlotChannel {
            name: name.clone(), points: vec![(0., 0.), (0.2, if i == 1 { 0. } else { 0.06 })],
        }).collect();
        state
    }

    #[test]
    fn control_marks_only_changing_targets_in_the_selected_group() {
        let state = moving_arm();
        let (selected, active) = control_joints(&state);
        assert_eq!(selected, ["left_j1_yaw", "left_j3_pitch"]);
        assert_eq!(active, ["left_j1_yaw"]);
    }

    #[test]
    fn releasing_pausing_or_stale_stream_clears_active_highlights() {
        for status in ["Hold L1 to move", "Paused", "L1 held · move a stick or D-pad", "Input timed out · release L1"] {
            let mut state = moving_arm();
            state.stats.task = format!("Preview only · Gamepad · Left arm · {status}");
            assert!(control_joints(&state).1.is_empty());
        }
        let mut state = moving_arm();
        state.live.as_mut().unwrap().connected = false;
        assert!(control_joints(&state).1.is_empty());
        state.live.as_mut().unwrap().connected = true;
        state.live.as_mut().unwrap().finished = true;
        assert!(control_joints(&state).1.is_empty());
    }

    #[test]
    fn measured_settling_is_not_shown_as_a_control_command() {
        let mut state = moving_arm();
        for channel in &mut state.action_series { channel.points[1].1 = channel.points[0].1; }
        state.joint_frames = vec![vec![0.; 3], vec![0.1; 3]];
        assert!(control_joints(&state).1.is_empty());
        state.stats.task = "Preview only · Gamepad · Right arm · Moving".into();
        assert_eq!(control_joints(&state).0, ["right_j1_yaw"]);
    }
}
