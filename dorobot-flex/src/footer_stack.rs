//! Vertical stack panel for footer containing State Plot, Action Plot, and Timeline
//!
//! This widget stacks three components vertically:
//! 1. State plot (observation.state)
//! 2. Action plot (action)
//! 3. Timeline scrubber

use makepad_widgets::*;
use crate::app_data::AppData;
use crate::widgets::time_series_plot::{TimeSeriesPlotAction, TimeSeriesPlotWidgetExt};
use crate::widgets::timeline::{TimelineAction, TimelineWidgetExt};
use crate::shared::styles::*;

live_design! {
    use link::theme::*;
    use link::shaders::*;
    use link::widgets::*;
    use crate::shared::styles::*;
    use crate::widgets::time_series_plot::TimeSeriesPlot;
    use crate::widgets::timeline::Timeline;

    pub FooterStack = {{FooterStack}} {
        width: Fill
        height: Fill
        flow: Down
        spacing: 2

        show_bg: true
        draw_bg: { color: (COLOR_BG_APP) }

        // State plot (top)
        state_plot_container = <View> {
            width: Fill
            height: Fill

            state_plot = <TimeSeriesPlot> {}
        }

        // Action plot (middle)
        action_plot_container = <View> {
            width: Fill
            height: Fill

            action_plot = <TimeSeriesPlot> {}
        }

        // Timeline (bottom, fixed height)
        timeline_container = <View> {
            width: Fill
            height: 50

            timeline = <Timeline> {}
        }
    }
}

#[derive(Live, LiveHook, Widget)]
pub struct FooterStack {
    #[deref]
    view: View,

    #[rust]
    last_episode: Option<u64>,

    #[rust]
    plots_initialized: bool,
}

impl Widget for FooterStack {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        let actions = cx.capture_actions(|cx| {
            self.view.handle_event(cx, event, scope);
        });

        // Forward timeline and plot actions to parent
        for action in actions.iter() {
            if let Some(widget_action) = action.as_widget_action() {
                // Re-emit timeline actions
                match widget_action.cast::<TimelineAction>() {
                    TimelineAction::Seek(time) => {
                        cx.widget_action(self.widget_uid(), &scope.path, TimelineAction::Seek(time));
                    }
                    TimelineAction::Play => {
                        cx.widget_action(self.widget_uid(), &scope.path, TimelineAction::Play);
                    }
                    TimelineAction::Pause => {
                        cx.widget_action(self.widget_uid(), &scope.path, TimelineAction::Pause);
                    }
                    _ => {}
                }

                // Re-emit plot cursor actions
                match widget_action.cast::<TimeSeriesPlotAction>() {
                    TimeSeriesPlotAction::CursorMoved(time) => {
                        cx.widget_action(self.widget_uid(), &scope.path,
                            TimeSeriesPlotAction::CursorMoved(time));
                    }
                    _ => {}
                }
            }
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        if let Some(data) = scope.data.get::<AppData>() {
            // Check if episode changed - reinitialize plots
            if data.current_episode != self.last_episode {
                self.last_episode = data.current_episode;
                self.plots_initialized = false;
            }

            // Initialize plot data once per episode
            if !self.plots_initialized && !data.episode_frames.is_empty() {
                self.init_plot_data(cx, data);
                self.plots_initialized = true;
            }

            // Update state plot
            let state_plot = self.view.time_series_plot(id!(state_plot_container.state_plot));
            state_plot.set_title(cx, "observation.state");
            state_plot.set_time_range(0.0, data.episode_duration);
            state_plot.set_cursor_time(cx, data.current_time);

            // Update action plot
            let action_plot = self.view.time_series_plot(id!(action_plot_container.action_plot));
            action_plot.set_title(cx, "action");
            action_plot.set_time_range(0.0, data.episode_duration);
            action_plot.set_cursor_time(cx, data.current_time);

            // Update timeline
            let timeline = self.view.timeline(id!(timeline_container.timeline));
            timeline.set_duration(cx, data.episode_duration, data.episode_fps);
            timeline.set_current_time(cx, data.current_time);
            timeline.set_playing(cx, data.is_playing);
        }

        self.view.draw_walk(cx, scope, walk)
    }
}

impl FooterStack {
    fn init_plot_data(&mut self, cx: &mut Cx, data: &AppData) {
        let frames = &data.episode_frames;
        if frames.is_empty() {
            return;
        }

        let state_channels = frames.first().map(|f| f.state.len()).unwrap_or(0);
        let action_channels = frames.first().map(|f| f.action.len()).unwrap_or(0);

        // Populate state plot
        let state_plot = self.view.time_series_plot(id!(state_plot_container.state_plot));
        state_plot.set_auto_scale_y(true);
        state_plot.set_window_size(10.0);

        for ch in 0..state_channels.min(14) {
            let plot_data: Vec<(f64, f64)> = frames.iter()
                .map(|f| (f.timestamp, f.state.get(ch).copied().unwrap_or(0.0) as f64))
                .collect();
            state_plot.set_channel_data(ch, &format!("state[{}]", ch), plot_data);
        }
        state_plot.recompute_scale(cx);

        // Populate action plot
        let action_plot = self.view.time_series_plot(id!(action_plot_container.action_plot));
        action_plot.set_auto_scale_y(true);
        action_plot.set_window_size(10.0);

        for ch in 0..action_channels.min(14) {
            let plot_data: Vec<(f64, f64)> = frames.iter()
                .map(|f| (f.timestamp, f.action.get(ch).copied().unwrap_or(0.0) as f64))
                .collect();
            action_plot.set_channel_data(ch, &format!("action[{}]", ch), plot_data);
        }
        action_plot.recompute_scale(cx);
    }
}

