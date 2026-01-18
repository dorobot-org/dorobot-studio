//! Playback Controls Widget for Footer
//!
//! Contains play/pause, step forward/backward buttons,
//! time display, and speed indicator.

use makepad_widgets::*;
use crate::app_data::AppData;
use crate::shared::styles::*;

live_design! {
    use link::theme::*;
    use link::shaders::*;
    use link::widgets::*;
    use crate::shared::styles::*;

    pub PlaybackControls = {{PlaybackControls}} {
        width: Fill
        height: Fill
        flow: Down
        padding: 8
        spacing: 6

        show_bg: true
        draw_bg: { color: (COLOR_BG_SIDEBAR) }

        // Playback buttons row
        buttons_row = <View> {
            width: Fill
            height: Fit
            flow: Right
            spacing: 6
            align: { x: 0.5, y: 0.5 }

            step_back_btn = <Button> {
                width: 36
                height: 28
                text: "<<"
                draw_bg: {
                    color: (COLOR_BG_PANEL)
                }
                draw_text: {
                    text_style: <TEXT_SMALL> {}
                    color: (COLOR_TEXT_PRIMARY)
                }
            }

            play_btn = <Button> {
                width: 52
                height: 28
                text: "Play"
                draw_bg: {
                    color: (COLOR_ACCENT)
                }
                draw_text: {
                    text_style: <TEXT_SMALL> {}
                    color: #ffffff
                }
            }

            step_fwd_btn = <Button> {
                width: 36
                height: 28
                text: ">>"
                draw_bg: {
                    color: (COLOR_BG_PANEL)
                }
                draw_text: {
                    text_style: <TEXT_SMALL> {}
                    color: (COLOR_TEXT_PRIMARY)
                }
            }
        }

        // Time display row
        time_row = <View> {
            width: Fill
            height: Fit
            flow: Right
            spacing: 4
            align: { x: 0.5 }

            current_time = <Label> {
                draw_text: {
                    text_style: <TEXT_MONO> {}
                    color: (COLOR_TEXT_PRIMARY)
                }
                text: "0:00.00"
            }

            <Label> {
                draw_text: {
                    text_style: <TEXT_SMALL> {}
                    color: (COLOR_TEXT_MUTED)
                }
                text: "/"
            }

            total_time = <Label> {
                draw_text: {
                    text_style: <TEXT_MONO> {}
                    color: (COLOR_TEXT_SECONDARY)
                }
                text: "0:00.00"
            }
        }

        // Speed display row
        speed_row = <View> {
            width: Fill
            height: Fit
            flow: Right
            spacing: 4
            align: { x: 0.5 }

            <Label> {
                draw_text: {
                    text_style: <TEXT_SMALL> {}
                    color: (COLOR_TEXT_MUTED)
                }
                text: "Speed:"
            }

            speed_label = <Label> {
                draw_text: {
                    text_style: <TEXT_SMALL> {}
                    color: (COLOR_TEXT_PRIMARY)
                }
                text: "1.0x"
            }
        }
    }
}

#[derive(Clone, Debug, DefaultNone)]
pub enum PlaybackAction {
    Play,
    Pause,
    StepForward,
    StepBackward,
    None,
}

#[derive(Live, LiveHook, Widget)]
pub struct PlaybackControls {
    #[deref]
    view: View,

    #[rust]
    is_playing: bool,
}

impl Widget for PlaybackControls {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        let actions = cx.capture_actions(|cx| {
            self.view.handle_event(cx, event, scope);
        });

        // Handle play/pause button
        if self.view.button(id!(buttons_row.play_btn)).clicked(&actions) {
            self.is_playing = !self.is_playing;
            let action = if self.is_playing {
                PlaybackAction::Play
            } else {
                PlaybackAction::Pause
            };
            cx.widget_action(self.widget_uid(), &scope.path, action);

            // Update button text
            let text = if self.is_playing { "Pause" } else { "Play" };
            self.view.button(id!(buttons_row.play_btn)).set_text(cx, text);
        }

        // Handle step back button
        if self.view.button(id!(buttons_row.step_back_btn)).clicked(&actions) {
            cx.widget_action(self.widget_uid(), &scope.path, PlaybackAction::StepBackward);
        }

        // Handle step forward button
        if self.view.button(id!(buttons_row.step_fwd_btn)).clicked(&actions) {
            cx.widget_action(self.widget_uid(), &scope.path, PlaybackAction::StepForward);
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        // Update from AppData
        if let Some(data) = scope.data.get::<AppData>() {
            // Sync playing state
            if self.is_playing != data.is_playing {
                self.is_playing = data.is_playing;
                let text = if self.is_playing { "Pause" } else { "Play" };
                self.view.button(id!(buttons_row.play_btn)).set_text(cx, text);
            }

            // Update time display
            self.view.label(id!(time_row.current_time))
                .set_text(cx, &AppData::format_time(data.current_time));
            self.view.label(id!(time_row.total_time))
                .set_text(cx, &AppData::format_time(data.episode_duration));

            // Update speed display
            self.view.label(id!(speed_row.speed_label))
                .set_text(cx, &format!("{:.1}x", data.playback_speed));
        }

        self.view.draw_walk(cx, scope, walk)
    }
}
