//! Playback Controls Widget for Footer

use makepad_widgets::*;
use makepad_app_shell::theme::get_global_dark_mode;
use crate::app_data::AppData;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    mod.widgets.PlaybackControlsBase = #(PlaybackControls::register_widget(vm))
    mod.widgets.PlaybackControls = set_type_default() do mod.widgets.PlaybackControlsBase{
        width: Fill
        height: Fill
        flow: Down
        padding: 8
        spacing: 6

        show_bg: true
        draw_bg +: {
            dark_mode: instance(0.0)
            pixel: fn() {
                let light_bg = vec4(0.984, 0.980, 0.973, 1.0)
                let dark_bg = vec4(0.078, 0.075, 0.071, 1.0)
                return mix(light_bg, dark_bg, self.dark_mode)
            }
        }

        // Playback buttons row
        buttons_row := View{
            width: Fill
            height: Fit
            flow: Right
            spacing: 6
            align: Align{x: 0.5 y: 0.5}

            step_back_btn := Button{
                width: 36
                height: 28
                text: "<<"
                draw_bg +: {
                    dark_mode: instance(0.0)
                    pixel: fn() {
                        let light_bg = vec4(0.910, 0.898, 0.882, 1.0)
                        let dark_bg = vec4(0.165, 0.153, 0.145, 1.0)
                        let base = mix(light_bg, dark_bg, self.dark_mode)
                        let hover_mix = mix(vec4(0.847, 0.831, 0.812, 1.0), vec4(0.165, 0.153, 0.145, 1.0), self.dark_mode)
                        return mix(base, hover_mix, self.hover)
                    }
                }
                draw_text +: {
                    text_style: mod.widgets.flex.TEXT_SMALL{}
                    dark_mode: instance(0.0)
                    get_color: fn() {
                        let light_text = vec4(0.106, 0.098, 0.090, 1.0)
                        let dark_text = vec4(0.910, 0.898, 0.882, 1.0)
                        return mix(light_text, dark_text, self.dark_mode)
                    }
                }
            }

            play_btn := Button{
                width: 52
                height: 28
                text: "Play"
                draw_bg +: {
                    color: #xD15010
                    pixel: fn() {
                        let base = self.color
                        let hover_color = vec4(0.820, 0.314, 0.063, 1.0)
                        let pressed_color = vec4(0.937, 0.435, 0.180, 1.0)
                        let color = mix(base, hover_color, self.hover)
                        return mix(color, pressed_color, self.down)
                    }
                }
                draw_text +: {
                    text_style: mod.widgets.flex.TEXT_SMALL{}
                    color: #xFFFFFF
                    get_color: fn() {
                        return vec4(1.000, 1.000, 1.000, 1.0)
                    }
                }
            }

            step_fwd_btn := Button{
                width: 36
                height: 28
                text: ">>"
                draw_bg +: {
                    dark_mode: instance(0.0)
                    pixel: fn() {
                        let light_bg = vec4(0.910, 0.898, 0.882, 1.0)
                        let dark_bg = vec4(0.165, 0.153, 0.145, 1.0)
                        let base = mix(light_bg, dark_bg, self.dark_mode)
                        let hover_mix = mix(vec4(0.847, 0.831, 0.812, 1.0), vec4(0.165, 0.153, 0.145, 1.0), self.dark_mode)
                        return mix(base, hover_mix, self.hover)
                    }
                }
                draw_text +: {
                    text_style: mod.widgets.flex.TEXT_SMALL{}
                    dark_mode: instance(0.0)
                    get_color: fn() {
                        let light_text = vec4(0.106, 0.098, 0.090, 1.0)
                        let dark_text = vec4(0.910, 0.898, 0.882, 1.0)
                        return mix(light_text, dark_text, self.dark_mode)
                    }
                }
            }
        }

        // Time display row
        time_row := View{
            width: Fill
            height: Fit
            flow: Right
            spacing: 4
            align: Align{x: 0.5}

            current_time := Label{
                draw_text +: {
                    text_style: mod.widgets.flex.TEXT_MONO{}
                    dark_mode: instance(0.0)
                    get_color: fn() {
                        let light_text = vec4(0.106, 0.098, 0.090, 1.0)
                        let dark_text = vec4(0.910, 0.898, 0.882, 1.0)
                        return mix(light_text, dark_text, self.dark_mode)
                    }
                }
                text: "0:00.00"
            }

            slash_label := Label{
                draw_text +: {
                    text_style: mod.widgets.flex.TEXT_SMALL{}
                    dark_mode: instance(0.0)
                    get_color: fn() {
                        let light_text = vec4(0.478, 0.443, 0.412, 1.0)
                        let dark_text = vec4(0.478, 0.443, 0.412, 1.0)
                        return mix(light_text, dark_text, self.dark_mode)
                    }
                }
                text: "/"
            }

            total_time := Label{
                draw_text +: {
                    text_style: mod.widgets.flex.TEXT_MONO{}
                    dark_mode: instance(0.0)
                    get_color: fn() {
                        let light_text = vec4(0.420, 0.384, 0.357, 1.0)
                        let dark_text = vec4(0.478, 0.443, 0.412, 1.0)
                        return mix(light_text, dark_text, self.dark_mode)
                    }
                }
                text: "0:00.00"
            }
        }

        // Speed display row
        speed_row := View{
            width: Fill
            height: Fit
            flow: Right
            spacing: 4
            align: Align{x: 0.5}

            speed_text := Label{
                draw_text +: {
                    text_style: mod.widgets.flex.TEXT_SMALL{}
                    dark_mode: instance(0.0)
                    get_color: fn() {
                        let light_text = vec4(0.478, 0.443, 0.412, 1.0)
                        let dark_text = vec4(0.478, 0.443, 0.412, 1.0)
                        return mix(light_text, dark_text, self.dark_mode)
                    }
                }
                text: "Speed:"
            }

            speed_label := Label{
                draw_text +: {
                    text_style: mod.widgets.flex.TEXT_SMALL{}
                    dark_mode: instance(0.0)
                    get_color: fn() {
                        let light_text = vec4(0.106, 0.098, 0.090, 1.0)
                        let dark_text = vec4(0.910, 0.898, 0.882, 1.0)
                        return mix(light_text, dark_text, self.dark_mode)
                    }
                }
                text: "1.0x"
            }
        }
    }
}

#[derive(Clone, Debug, Default)]
pub enum PlaybackAction {
    Play,
    Pause,
    StepForward,
    StepBackward,
    #[default]
    None,
}

#[derive(Script, ScriptHook, Widget)]
pub struct PlaybackControls {
    #[deref]
    view: View,

    #[rust]
    is_playing: bool,
}

impl Widget for PlaybackControls {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        // Capture actions from child widgets (buttons emit ButtonAction internally)
        let actions = cx.capture_actions(|cx| {
            self.view.handle_event(cx, event, scope);
        });

        // Check button clicks using the clicked() method on captured actions
        if self.view.button(cx, ids!(buttons_row.play_btn)).clicked(&actions) {
            self.is_playing = !self.is_playing;
            let action = if self.is_playing {
                PlaybackAction::Play
            } else {
                PlaybackAction::Pause
            };
            cx.widget_action(self.widget_uid(), action);
            let text = if self.is_playing { "Pause" } else { "Play" };
            self.view.button(cx, ids!(buttons_row.play_btn)).set_text(cx, text);
        }

        if self.view.button(cx, ids!(buttons_row.step_back_btn)).clicked(&actions) {
            cx.widget_action(self.widget_uid(), PlaybackAction::StepBackward);
        }

        if self.view.button(cx, ids!(buttons_row.step_fwd_btn)).clicked(&actions) {
            cx.widget_action(self.widget_uid(), PlaybackAction::StepForward);
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        let dm = get_global_dark_mode();
        self.apply_theme(cx, dm);

        if let Some(data) = scope.data.get::<AppData>() {
            if self.is_playing != data.is_playing {
                self.is_playing = data.is_playing;
                let text = if self.is_playing { "Pause" } else { "Play" };
                self.view.button(cx, ids!(buttons_row.play_btn)).set_text(cx, text);
            }

            self.view.label(cx, ids!(time_row.current_time))
                .set_text(cx, &AppData::format_time(data.current_time));
            self.view.label(cx, ids!(time_row.total_time))
                .set_text(cx, &AppData::format_time(data.episode_duration));
            self.view.label(cx, ids!(speed_row.speed_label))
                .set_text(cx, &format!("{:.1}x", data.playback_speed));
        }

        self.view.draw_walk(cx, scope, walk)
    }
}

impl PlaybackControls {
    fn apply_theme(&mut self, cx: &mut Cx, dm: f64) {
        script_apply_eval!(cx, self.view, {
            draw_bg +: { dark_mode: #(dm) }
        });

        // Buttons - apply dark_mode to both background and text
        for path in [
            ids!(buttons_row.step_back_btn),
            ids!(buttons_row.step_fwd_btn),
        ] {
            let mut btn = self.view.button(cx, path);
            script_apply_eval!(cx, btn, {
                draw_bg +: { dark_mode: #(dm) }
                draw_text +: { dark_mode: #(dm) }
            });
        }

        // Labels
        for path in [
            ids!(time_row.current_time),
            ids!(time_row.slash_label),
            ids!(time_row.total_time),
            ids!(speed_row.speed_text),
            ids!(speed_row.speed_label),
        ] {
            let mut label = self.view.label(cx, path);
            script_apply_eval!(cx, label, {
                draw_text +: { dark_mode: #(dm) }
            });
        }
    }
}

impl PlaybackControlsRef {
    /// Drive the play/pause label from an owner that has no [`AppData`] in
    /// scope. Without this the widget only learns the transport state during
    /// `draw_walk`, so a screen holding its playback state elsewhere shows a
    /// button labelled "Play" while the episode is playing.
    pub fn set_playing(&self, cx: &mut Cx, playing: bool) {
        let Some(mut inner) = self.borrow_mut() else { return };
        if inner.is_playing == playing {
            return;
        }
        inner.is_playing = playing;
        let text = if playing { "Pause" } else { "Play" };
        inner.view.button(cx, ids!(buttons_row.play_btn)).set_text(cx, text);
    }
}
