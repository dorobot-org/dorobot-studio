//! Right sidebar with episode information display
//!
//! Shows detailed information about the currently selected episode:
//! - Episode index, frame count, duration, FPS
//! - Current playback time
//! - Task description
//! - State and action channel counts

use makepad_widgets::*;
use crate::app_data::AppData;
use crate::shared::styles::*;

live_design! {
    use link::theme::*;
    use link::shaders::*;
    use link::widgets::*;
    use crate::shared::styles::*;

    // Info row component
    InfoRow = <View> {
        width: Fill
        height: Fit
        flow: Right
        spacing: 8

        label = <Label> {
            width: 80
            draw_text: {
                text_style: <TEXT_SMALL> {}
                color: (COLOR_TEXT_MUTED)
            }
        }

        value = <Label> {
            draw_text: {
                text_style: <TEXT_BODY> {}
                color: (COLOR_TEXT_PRIMARY)
            }
            text: "-"
        }
    }

    pub EpisodeInfoPanel = {{EpisodeInfoPanel}} {
        width: Fill
        height: Fill
        flow: Down
        padding: 12
        spacing: 12

        show_bg: true
        draw_bg: { color: (COLOR_BG_SIDEBAR) }

        // Header
        <Label> {
            draw_text: {
                text_style: <TEXT_SMALL> {}
                color: (COLOR_TEXT_MUTED)
            }
            text: "EPISODE INFO"
        }

        // Episode details container
        info_container = <View> {
            width: Fill
            height: Fit
            flow: Down
            spacing: 8

            // Episode index
            episode_row = <InfoRow> {
                label = { text: "Episode:" }
                episode_value = <Label> {
                    draw_text: {
                        text_style: <TEXT_BODY> {}
                        color: (COLOR_TEXT_PRIMARY)
                    }
                    text: "-"
                }
            }

            // Frame count
            frames_row = <InfoRow> {
                label = { text: "Frames:" }
                frames_value = <Label> {
                    draw_text: {
                        text_style: <TEXT_BODY> {}
                        color: (COLOR_TEXT_PRIMARY)
                    }
                    text: "-"
                }
            }

            // Duration
            duration_row = <InfoRow> {
                label = { text: "Duration:" }
                duration_value = <Label> {
                    draw_text: {
                        text_style: <TEXT_BODY> {}
                        color: (COLOR_TEXT_PRIMARY)
                    }
                    text: "-"
                }
            }

            // FPS
            fps_row = <InfoRow> {
                label = { text: "FPS:" }
                fps_value = <Label> {
                    draw_text: {
                        text_style: <TEXT_BODY> {}
                        color: (COLOR_TEXT_PRIMARY)
                    }
                    text: "-"
                }
            }

            // Current time
            time_row = <InfoRow> {
                label = { text: "Time:" }
                time_value = <Label> {
                    draw_text: {
                        text_style: <TEXT_MONO> {}
                        color: (COLOR_TEXT_PRIMARY)
                    }
                    text: "-"
                }
            }

            // Current frame
            frame_row = <InfoRow> {
                label = { text: "Frame:" }
                frame_value = <Label> {
                    draw_text: {
                        text_style: <TEXT_MONO> {}
                        color: (COLOR_TEXT_PRIMARY)
                    }
                    text: "-"
                }
            }
        }

        // Divider
        <View> {
            width: Fill
            height: 1
            margin: { top: 4, bottom: 4 }
            show_bg: true
            draw_bg: { color: (COLOR_DIVIDER) }
        }

        // Task section
        <Label> {
            draw_text: {
                text_style: <TEXT_SMALL> {}
                color: (COLOR_TEXT_MUTED)
            }
            text: "TASK"
        }

        task_description = <Label> {
            width: Fill
            draw_text: {
                text_style: <TEXT_BODY> {}
                color: (COLOR_TEXT_SECONDARY)
                wrap: Word
            }
            text: "No episode selected"
        }

        // Spacer
        <View> { height: Fill }

        // State channels info
        <Label> {
            draw_text: {
                text_style: <TEXT_SMALL> {}
                color: (COLOR_TEXT_MUTED)
            }
            text: "STATE CHANNELS"
        }

        state_channels = <Label> {
            width: Fill
            draw_text: {
                text_style: <TEXT_SMALL> {}
                color: (COLOR_TEXT_SECONDARY)
            }
            text: "-"
        }

        // Action channels info
        <Label> {
            margin: { top: 8 }
            draw_text: {
                text_style: <TEXT_SMALL> {}
                color: (COLOR_TEXT_MUTED)
            }
            text: "ACTION CHANNELS"
        }

        action_channels = <Label> {
            width: Fill
            draw_text: {
                text_style: <TEXT_SMALL> {}
                color: (COLOR_TEXT_SECONDARY)
            }
            text: "-"
        }
    }
}

#[derive(Live, LiveHook, Widget)]
pub struct EpisodeInfoPanel {
    #[deref]
    view: View,
}

impl Widget for EpisodeInfoPanel {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        if let Some(data) = scope.data.get::<AppData>() {
            if let Some(ep_idx) = data.current_episode {
                // Episode index
                self.view.label(id!(info_container.episode_row.episode_value))
                    .set_text(cx, &format!("{}", ep_idx));

                // Frame count
                let frame_count = data.total_frames();
                self.view.label(id!(info_container.frames_row.frames_value))
                    .set_text(cx, &format!("{}", frame_count));

                // Duration
                self.view.label(id!(info_container.duration_row.duration_value))
                    .set_text(cx, &format!("{:.2}s", data.episode_duration));

                // FPS
                self.view.label(id!(info_container.fps_row.fps_value))
                    .set_text(cx, &format!("{:.0}", data.episode_fps));

                // Current time
                self.view.label(id!(info_container.time_row.time_value))
                    .set_text(cx, &AppData::format_time(data.current_time));

                // Current frame
                self.view.label(id!(info_container.frame_row.frame_value))
                    .set_text(cx, &format!("{} / {}", data.current_frame_index(), frame_count));

                // Task description
                if let Some(episode_info) = data.episodes.get(ep_idx as usize) {
                    self.view.label(id!(task_description))
                        .set_text(cx, &episode_info.task_description);
                }

                // Channel counts
                if let Some(frame) = data.episode_frames.first() {
                    self.view.label(id!(state_channels))
                        .set_text(cx, &format!("{} channels", frame.state.len()));
                    self.view.label(id!(action_channels))
                        .set_text(cx, &format!("{} channels", frame.action.len()));
                } else {
                    self.view.label(id!(state_channels)).set_text(cx, "-");
                    self.view.label(id!(action_channels)).set_text(cx, "-");
                }
            } else {
                // No episode selected - show placeholders
                self.view.label(id!(info_container.episode_row.episode_value)).set_text(cx, "-");
                self.view.label(id!(info_container.frames_row.frames_value)).set_text(cx, "-");
                self.view.label(id!(info_container.duration_row.duration_value)).set_text(cx, "-");
                self.view.label(id!(info_container.fps_row.fps_value)).set_text(cx, "-");
                self.view.label(id!(info_container.time_row.time_value)).set_text(cx, "-");
                self.view.label(id!(info_container.frame_row.frame_value)).set_text(cx, "-");
                self.view.label(id!(task_description)).set_text(cx, "No episode selected");
                self.view.label(id!(state_channels)).set_text(cx, "-");
                self.view.label(id!(action_channels)).set_text(cx, "-");
            }
        }

        self.view.draw_walk(cx, scope, walk)
    }
}
