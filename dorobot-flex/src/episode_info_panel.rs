//! Right sidebar with episode information display

use makepad_widgets::*;
use makepad_app_shell::theme::get_global_dark_mode;
use crate::app_data::AppData;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    // Info row component
    let InfoRow = View{
        width: Fill
        height: Fit
        flow: Right
        spacing: 8

        label := Label{
            width: 80
            draw_text +: {
                text_style: mod.widgets.flex.TEXT_SMALL{}
                dark_mode: instance(0.0)
                get_color: fn() {
                    let light_text = vec4(0.478, 0.443, 0.412, 1.0)
                    let dark_text = vec4(0.478, 0.443, 0.412, 1.0)
                    return mix(light_text, dark_text, self.dark_mode)
                }
            }
        }

        value := Label{
            draw_text +: {
                text_style: mod.widgets.flex.TEXT_BODY{}
                dark_mode: instance(0.0)
                get_color: fn() {
                    let light_text = vec4(0.106, 0.098, 0.090, 1.0)
                    let dark_text = vec4(0.910, 0.898, 0.882, 1.0)
                    return mix(light_text, dark_text, self.dark_mode)
                }
            }
            text: "-"
        }
    }

    mod.widgets.EpisodeInfoPanelBase = #(EpisodeInfoPanel::register_widget(vm))
    mod.widgets.EpisodeInfoPanel = set_type_default() do mod.widgets.EpisodeInfoPanelBase{
        width: Fill
        height: Fill
        flow: Down
        padding: 12
        spacing: 12

        show_bg: true
        draw_bg +: {
            dark_mode: instance(0.0)
            pixel: fn() {
                let light_bg = vec4(0.984, 0.980, 0.973, 1.0)
                let dark_bg = vec4(0.078, 0.075, 0.071, 1.0)
                return mix(light_bg, dark_bg, self.dark_mode)
            }
        }

        // Header
        info_header := Label{
            draw_text +: {
                text_style: mod.widgets.flex.TEXT_SMALL{}
                dark_mode: instance(0.0)
                get_color: fn() {
                    let light_text = vec4(0.478, 0.443, 0.412, 1.0)
                    let dark_text = vec4(0.478, 0.443, 0.412, 1.0)
                    return mix(light_text, dark_text, self.dark_mode)
                }
            }
            text: "EPISODE INFO"
        }

        // Episode details container
        info_container := View{
            width: Fill
            height: Fit
            flow: Down
            spacing: 8

            episode_row := InfoRow{
                label +: { text: "Episode:" }
                episode_value := Label{
                    draw_text +: {
                        text_style: mod.widgets.flex.TEXT_BODY{}
                        dark_mode: instance(0.0)
                        get_color: fn() {
                            let light_text = vec4(0.106, 0.098, 0.090, 1.0)
                            let dark_text = vec4(0.910, 0.898, 0.882, 1.0)
                            return mix(light_text, dark_text, self.dark_mode)
                        }
                    }
                    text: "-"
                }
            }

            frames_row := InfoRow{
                label +: { text: "Frames:" }
                frames_value := Label{
                    draw_text +: {
                        text_style: mod.widgets.flex.TEXT_BODY{}
                        dark_mode: instance(0.0)
                        get_color: fn() {
                            let light_text = vec4(0.106, 0.098, 0.090, 1.0)
                            let dark_text = vec4(0.910, 0.898, 0.882, 1.0)
                            return mix(light_text, dark_text, self.dark_mode)
                        }
                    }
                    text: "-"
                }
            }

            duration_row := InfoRow{
                label +: { text: "Duration:" }
                duration_value := Label{
                    draw_text +: {
                        text_style: mod.widgets.flex.TEXT_BODY{}
                        dark_mode: instance(0.0)
                        get_color: fn() {
                            let light_text = vec4(0.106, 0.098, 0.090, 1.0)
                            let dark_text = vec4(0.910, 0.898, 0.882, 1.0)
                            return mix(light_text, dark_text, self.dark_mode)
                        }
                    }
                    text: "-"
                }
            }

            fps_row := InfoRow{
                label +: { text: "FPS:" }
                fps_value := Label{
                    draw_text +: {
                        text_style: mod.widgets.flex.TEXT_BODY{}
                        dark_mode: instance(0.0)
                        get_color: fn() {
                            let light_text = vec4(0.106, 0.098, 0.090, 1.0)
                            let dark_text = vec4(0.910, 0.898, 0.882, 1.0)
                            return mix(light_text, dark_text, self.dark_mode)
                        }
                    }
                    text: "-"
                }
            }

            time_row := InfoRow{
                label +: { text: "Time:" }
                time_value := Label{
                    draw_text +: {
                        text_style: mod.widgets.flex.TEXT_MONO{}
                        dark_mode: instance(0.0)
                        get_color: fn() {
                            let light_text = vec4(0.106, 0.098, 0.090, 1.0)
                            let dark_text = vec4(0.910, 0.898, 0.882, 1.0)
                            return mix(light_text, dark_text, self.dark_mode)
                        }
                    }
                    text: "-"
                }
            }

            frame_row := InfoRow{
                label +: { text: "Frame:" }
                frame_value := Label{
                    draw_text +: {
                        text_style: mod.widgets.flex.TEXT_MONO{}
                        dark_mode: instance(0.0)
                        get_color: fn() {
                            let light_text = vec4(0.106, 0.098, 0.090, 1.0)
                            let dark_text = vec4(0.910, 0.898, 0.882, 1.0)
                            return mix(light_text, dark_text, self.dark_mode)
                        }
                    }
                    text: "-"
                }
            }
        }

        // Divider
        divider := View{
            width: Fill
            height: 1
            margin: Inset{top: 4. bottom: 4.}
            show_bg: true
            draw_bg +: {
                dark_mode: instance(0.0)
                pixel: fn() {
                    let light_div = vec4(0.910, 0.898, 0.882, 1.0)
                    let dark_div = vec4(0.165, 0.153, 0.145, 1.0)
                    return mix(light_div, dark_div, self.dark_mode)
                }
            }
        }

        // Task section
        task_header := Label{
            draw_text +: {
                text_style: mod.widgets.flex.TEXT_SMALL{}
                dark_mode: instance(0.0)
                get_color: fn() {
                    let light_text = vec4(0.478, 0.443, 0.412, 1.0)
                    let dark_text = vec4(0.478, 0.443, 0.412, 1.0)
                    return mix(light_text, dark_text, self.dark_mode)
                }
            }
            text: "TASK"
        }

        task_description := Label{
            width: Fill
            draw_text +: {
                text_style: mod.widgets.flex.TEXT_BODY{}
                dark_mode: instance(0.0)
                get_color: fn() {
                    let light_text = vec4(0.420, 0.384, 0.357, 1.0)
                    let dark_text = vec4(0.478, 0.443, 0.412, 1.0)
                    return mix(light_text, dark_text, self.dark_mode)
                }
            }
            text: "No episode selected"
        }

        // Spacer
        View{ height: Fill }

        // State channels info
        state_header := Label{
            draw_text +: {
                text_style: mod.widgets.flex.TEXT_SMALL{}
                dark_mode: instance(0.0)
                get_color: fn() {
                    let light_text = vec4(0.478, 0.443, 0.412, 1.0)
                    let dark_text = vec4(0.478, 0.443, 0.412, 1.0)
                    return mix(light_text, dark_text, self.dark_mode)
                }
            }
            text: "STATE CHANNELS"
        }
        state_channels := Label{
            width: Fill
            draw_text +: {
                text_style: mod.widgets.flex.TEXT_SMALL{}
                dark_mode: instance(0.0)
                get_color: fn() {
                    let light_text = vec4(0.420, 0.384, 0.357, 1.0)
                    let dark_text = vec4(0.478, 0.443, 0.412, 1.0)
                    return mix(light_text, dark_text, self.dark_mode)
                }
            }
            text: "-"
        }

        // Action channels info
        action_header := Label{
            margin: Inset{top: 8.}
            draw_text +: {
                text_style: mod.widgets.flex.TEXT_SMALL{}
                dark_mode: instance(0.0)
                get_color: fn() {
                    let light_text = vec4(0.478, 0.443, 0.412, 1.0)
                    let dark_text = vec4(0.478, 0.443, 0.412, 1.0)
                    return mix(light_text, dark_text, self.dark_mode)
                }
            }
            text: "ACTION CHANNELS"
        }
        action_channels := Label{
            width: Fill
            draw_text +: {
                text_style: mod.widgets.flex.TEXT_SMALL{}
                dark_mode: instance(0.0)
                get_color: fn() {
                    let light_text = vec4(0.420, 0.384, 0.357, 1.0)
                    let dark_text = vec4(0.478, 0.443, 0.412, 1.0)
                    return mix(light_text, dark_text, self.dark_mode)
                }
            }
            text: "-"
        }
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct EpisodeInfoPanel {
    #[deref]
    view: View,
}

impl Widget for EpisodeInfoPanel {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        let dm = get_global_dark_mode();
        self.apply_theme(cx, dm);

        if let Some(data) = scope.data.get::<AppData>() {
            if let Some(ep_idx) = data.current_episode {
                self.view.label(cx, ids!(info_container.episode_row.episode_value))
                    .set_text(cx, &format!("{}", ep_idx));
                self.view.label(cx, ids!(info_container.frames_row.frames_value))
                    .set_text(cx, &format!("{}", data.total_frames()));
                self.view.label(cx, ids!(info_container.duration_row.duration_value))
                    .set_text(cx, &format!("{:.2}s", data.episode_duration));
                self.view.label(cx, ids!(info_container.fps_row.fps_value))
                    .set_text(cx, &format!("{:.0}", data.episode_fps));
                self.view.label(cx, ids!(info_container.time_row.time_value))
                    .set_text(cx, &AppData::format_time(data.current_time));
                self.view.label(cx, ids!(info_container.frame_row.frame_value))
                    .set_text(cx, &format!("{} / {}", data.current_frame_index(), data.total_frames()));

                if let Some(episode_info) = data.episodes.get(ep_idx as usize) {
                    self.view.label(cx, ids!(task_description))
                        .set_text(cx, &episode_info.task_description);
                }

                if let Some(frame) = data.episode_frames.first() {
                    self.view.label(cx, ids!(state_channels))
                        .set_text(cx, &format!("{} channels", frame.state.len()));
                    self.view.label(cx, ids!(action_channels))
                        .set_text(cx, &format!("{} channels", frame.action.len()));
                }
            } else {
                self.view.label(cx, ids!(info_container.episode_row.episode_value)).set_text(cx, "-");
                self.view.label(cx, ids!(info_container.frames_row.frames_value)).set_text(cx, "-");
                self.view.label(cx, ids!(info_container.duration_row.duration_value)).set_text(cx, "-");
                self.view.label(cx, ids!(info_container.fps_row.fps_value)).set_text(cx, "-");
                self.view.label(cx, ids!(info_container.time_row.time_value)).set_text(cx, "-");
                self.view.label(cx, ids!(info_container.frame_row.frame_value)).set_text(cx, "-");
                self.view.label(cx, ids!(task_description)).set_text(cx, "No episode selected");
                self.view.label(cx, ids!(state_channels)).set_text(cx, "-");
                self.view.label(cx, ids!(action_channels)).set_text(cx, "-");
            }
        }

        self.view.draw_walk(cx, scope, walk)
    }
}

impl EpisodeInfoPanel {
    fn apply_theme(&mut self, cx: &mut Cx, dm: f64) {
        script_apply_eval!(cx, self.view, {
            draw_bg +: {dark_mode: #(dm)}
        });
        let mut divider = self.view.view(cx, ids!(divider));
        script_apply_eval!(cx, divider, {
            draw_bg +: {dark_mode: #(dm)}
        });

        // Headers, content labels, and info row labels/values
        for path in [
            ids!(info_header),
            ids!(task_header),
            ids!(state_header),
            ids!(action_header),
            ids!(task_description),
            ids!(state_channels),
            ids!(action_channels),
        ] {
            let mut label = self.view.label(cx, path);
            script_apply_eval!(cx, label, {
                draw_text +: {dark_mode: #(dm)}
            });
        }
        for path in [
            ids!(info_container.episode_row.label),
            ids!(info_container.episode_row.episode_value),
            ids!(info_container.frames_row.label),
            ids!(info_container.frames_row.frames_value),
            ids!(info_container.duration_row.label),
            ids!(info_container.duration_row.duration_value),
            ids!(info_container.fps_row.label),
            ids!(info_container.fps_row.fps_value),
            ids!(info_container.time_row.label),
            ids!(info_container.time_row.time_value),
            ids!(info_container.frame_row.label),
            ids!(info_container.frame_row.frame_value),
        ] {
            let mut label = self.view.label(cx, path);
            script_apply_eval!(cx, label, {
                draw_text +: {dark_mode: #(dm)}
            });
        }
    }
}
