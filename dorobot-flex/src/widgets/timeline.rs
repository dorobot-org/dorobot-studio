//! Timeline Widget with playback controls and scrubbing

use makepad_widgets::*;

live_design! {
    use link::theme::*;
    use link::shaders::*;
    use link::widgets::*;
    use crate::shared::styles::*;

    // Drawing primitive for ruler ticks
    DrawRulerTick = {{DrawRulerTick}} {
        fn pixel(self) -> vec4 {
            return self.color;
        }
    }

    // Main timeline widget
    pub Timeline = {{Timeline}} {
        width: Fill
        height: 80

        flow: Down

        draw_tick: {}
        draw_text: {
            text_style: <THEME_FONT_REGULAR>{ font_size: 9.0 }
            color: #888888
        }

        // Playback controls bar - hidden since we use FooterTimeline's controls
        controls = <View> {
            width: Fill
            height: 0
            visible: false
        }

        // Time ruler with ticks and labels
        ruler_area = <View> {
            width: Fill
            height: 20
            padding: { left: 4, right: 4 }

            show_bg: true
            draw_bg: { color: #1e1e24 }

            // Labels will be drawn dynamically
        }

        // Timeline track
        track_area = <View> {
            width: Fill
            height: Fill
            cursor: Hand
            flow: Overlay

            show_bg: true
            draw_bg: { color: #282830 }

            // Playhead indicator (positioned via margin)
            playhead = <View> {
                width: 2
                height: Fill
                margin: { left: 0 }
                show_bg: true
                draw_bg: { color: #ff4545 }
            }

            // Episode markers (rendered as overlays)
            episode_markers = <View> {
                width: Fill
                height: Fill
            }
        }
    }
}

// Drawing primitive for ruler ticks
#[derive(Live, LiveHook, LiveRegister)]
#[repr(C)]
pub struct DrawRulerTick {
    #[deref]
    draw_super: DrawQuad,
    #[live]
    color: Vec4,
}

#[derive(Clone, Debug, DefaultNone)]
pub enum TimelineAction {
    Play,
    Pause,
    Seek(f64),         // Seek to time in seconds
    StepForward,
    StepBackward,
    SpeedChanged(f64),
    None,
}

#[derive(Live, LiveHook, Widget)]
pub struct Timeline {
    #[deref]
    view: View,

    // Drawing primitives
    #[live]
    draw_tick: DrawRulerTick,
    #[live]
    draw_text: DrawText,

    // Playback state
    #[rust]
    is_playing: bool,
    #[rust]
    playback_speed: f64,

    // Time state
    #[rust]
    current_time: f64,
    #[rust]
    duration: f64,
    #[rust]
    fps: f64,

    // Frame state
    #[rust]
    current_frame: u64,
    #[rust]
    total_frames: u64,

    // Interaction
    #[rust]
    is_scrubbing: bool,

    // Episode markers
    #[rust]
    episode_boundaries: Vec<f64>,

    // Cached ruler rect
    #[rust]
    ruler_rect: Rect,
}

impl Widget for Timeline {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        // Capture actions from this event cycle (this also handles view events)
        let actions = cx.capture_actions(|cx| {
            self.view.handle_event(cx, event, scope);
        });

        // Handle button clicks
        if self.view.button(id!(play_btn)).clicked(&actions) {
            self.is_playing = !self.is_playing;
            let action = if self.is_playing {
                self.view.button(id!(play_btn)).set_text(cx, "||");
                TimelineAction::Play
            } else {
                self.view.button(id!(play_btn)).set_text(cx, ">");
                TimelineAction::Pause
            };
            cx.widget_action(self.widget_uid(), &scope.path, action);
        }

        if self.view.button(id!(step_back_btn)).clicked(&actions) {
            cx.widget_action(self.widget_uid(), &scope.path, TimelineAction::StepBackward);
        }

        if self.view.button(id!(step_fwd_btn)).clicked(&actions) {
            cx.widget_action(self.widget_uid(), &scope.path, TimelineAction::StepForward);
        }

        // Handle track scrubbing
        let track_area = self.view.view(id!(track_area)).area();
        match event.hits(cx, track_area) {
            Hit::FingerDown(fe) => {
                self.is_scrubbing = true;
                self.scrub_to_position(cx, scope, fe.abs.x, track_area.rect(cx));
            }
            Hit::FingerMove(fe) if self.is_scrubbing => {
                self.scrub_to_position(cx, scope, fe.abs.x, track_area.rect(cx));
            }
            Hit::FingerUp(_) => {
                self.is_scrubbing = false;
            }
            _ => {}
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        // First draw the view structure
        let _ = self.view.draw_walk(cx, scope, walk);

        // Get ruler area for drawing time ticks
        let ruler_area = self.view.view(id!(ruler_area));
        self.ruler_rect = ruler_area.area().rect(cx);

        // Draw time ruler ticks and labels
        if self.duration > 0.0 && self.ruler_rect.size.x > 10.0 {
            self.draw_time_ruler(cx);
        }

        DrawStep::done()
    }
}

impl Timeline {
    /// Draw time ruler with ticks and labels
    fn draw_time_ruler(&mut self, cx: &mut Cx2d) {
        let rect = self.ruler_rect;
        if rect.size.x <= 0.0 || rect.size.y <= 0.0 {
            return;
        }

        // Calculate tick interval based on duration
        // Aim for roughly 1 tick per 50-100 pixels
        let pixels_per_second = rect.size.x / self.duration;
        let tick_interval = if pixels_per_second > 100.0 {
            1.0  // 1 second
        } else if pixels_per_second > 20.0 {
            5.0  // 5 seconds
        } else if pixels_per_second > 5.0 {
            10.0  // 10 seconds
        } else {
            30.0  // 30 seconds
        };

        // Draw ticks
        self.draw_tick.color = Vec4 { x: 0.5, y: 0.5, z: 0.5, w: 1.0 };

        let mut time = 0.0;
        while time <= self.duration {
            let x = rect.pos.x + (time / self.duration) * rect.size.x;

            // Draw tick mark
            let tick_height = if (time % (tick_interval * 2.0)).abs() < 0.01 {
                rect.size.y * 0.6  // Major tick
            } else {
                rect.size.y * 0.3  // Minor tick
            };

            self.draw_tick.draw_abs(cx, Rect {
                pos: dvec2(x - 0.5, rect.pos.y + rect.size.y - tick_height),
                size: dvec2(1.0, tick_height),
            });

            // Draw time label for major ticks
            if (time % (tick_interval * 2.0)).abs() < 0.01 || time == 0.0 {
                let label = Self::format_ruler_time(time);
                self.draw_text.draw_abs(cx, dvec2(x + 2.0, rect.pos.y + 2.0), &label);
            }

            time += tick_interval;
        }
    }

    fn format_ruler_time(seconds: f64) -> String {
        let mins = (seconds / 60.0) as u32;
        let secs = (seconds % 60.0) as u32;
        if mins > 0 {
            format!("{}:{:02}", mins, secs)
        } else {
            format!("{}s", secs)
        }
    }

    fn scrub_to_position(&mut self, cx: &mut Cx, scope: &mut Scope, abs_x: f64, rect: Rect) {
        let rel_x = ((abs_x - rect.pos.x) / rect.size.x).clamp(0.0, 1.0);
        let time = rel_x * self.duration;

        self.set_current_time(cx, time);
        cx.widget_action(self.widget_uid(), &scope.path, TimelineAction::Seek(time));
    }

    /// Set the total duration
    pub fn set_duration(&mut self, cx: &mut Cx, duration: f64, fps: f64) {
        self.duration = duration;
        self.fps = fps;
        self.total_frames = (duration * fps) as u64;
        self.update_displays(cx);
    }

    /// Set current time (from external sync or scrubbing)
    pub fn set_current_time(&mut self, cx: &mut Cx, time: f64) {
        self.current_time = time.clamp(0.0, self.duration);
        self.current_frame = (self.current_time * self.fps) as u64;
        self.update_displays(cx);
        self.update_playhead_position(cx);
        self.view.redraw(cx);
    }

    /// Update playhead position based on current time
    fn update_playhead_position(&mut self, cx: &mut Cx) {
        // Calculate position as percentage of track width
        let progress = if self.duration > 0.0 {
            self.current_time / self.duration
        } else {
            0.0
        };

        // Get track area dimensions
        let track_rect = self.view.view(id!(track_area)).area().rect(cx);
        let playhead_x = progress * track_rect.size.x;

        // Update playhead margin to position it
        self.view.view(id!(track_area.playhead)).apply_over(cx, live! {
            margin: { left: (playhead_x) }
        });
    }

    /// Set playback speed
    pub fn set_speed(&mut self, cx: &mut Cx, speed: f64) {
        self.playback_speed = speed;
        self.view.label(id!(controls.speed_label))
            .set_text(cx, &format!("{:.1}x", speed));
    }

    /// Set playing state
    pub fn set_playing(&mut self, cx: &mut Cx, playing: bool) {
        self.is_playing = playing;
        let text = if playing { "||" } else { ">" };
        self.view.button(id!(play_btn)).set_text(cx, text);
    }

    /// Add episode boundary marker
    pub fn add_episode_boundary(&mut self, time: f64) {
        self.episode_boundaries.push(time);
    }

    /// Clear episode markers
    pub fn clear_episode_markers(&mut self) {
        self.episode_boundaries.clear();
    }

    fn update_displays(&mut self, cx: &mut Cx) {
        // Time display: MM:SS.mmm / MM:SS.mmm
        let current_str = Self::format_time(self.current_time);
        let duration_str = Self::format_time(self.duration);
        self.view.label(id!(controls.time_display))
            .set_text(cx, &format!("{} / {}", current_str, duration_str));

        // Frame display
        self.view.label(id!(controls.frame_display))
            .set_text(cx, &format!("F: {} / {}", self.current_frame, self.total_frames));
    }

    fn format_time(seconds: f64) -> String {
        let mins = (seconds / 60.0) as u32;
        let secs = seconds % 60.0;
        format!("{:02}:{:06.3}", mins, secs)
    }
}

impl TimelineRef {
    pub fn set_duration(&self, cx: &mut Cx, duration: f64, fps: f64) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_duration(cx, duration, fps);
        }
    }

    pub fn set_current_time(&self, cx: &mut Cx, time: f64) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_current_time(cx, time);
        }
    }

    pub fn set_playing(&self, cx: &mut Cx, playing: bool) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_playing(cx, playing);
        }
    }

    pub fn set_speed(&self, cx: &mut Cx, speed: f64) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_speed(cx, speed);
        }
    }
}
