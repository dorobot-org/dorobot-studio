//! Time Series Plot Widget for observation.state and action visualization

use makepad_widgets::*;
use makepad_app_shell::theme::get_global_dark_mode;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    // Drawing primitive for plot lines
    let DrawPlotLine = set_type_default() do #(DrawPlotLine::script_shader(vm)){
        ..mod.draw.DrawQuad
        pixel: fn() {
            return self.color
        }
    }

    // Main time series plot widget
    mod.widgets.TimeSeriesPlotBase = #(TimeSeriesPlot::register_widget(vm))
    mod.widgets.TimeSeriesPlot = set_type_default() do mod.widgets.TimeSeriesPlotBase{
        width: Fill
        height: 150

        flow: Down

        draw_line: DrawPlotLine{}

        // Header with channel toggles
        header := View{
            width: Fill
            height: 28
            padding: Inset{left: 8. right: 8.}
            spacing: 8
            align: Align{y: 0.5}

            show_bg: true
            draw_bg +: {
                dark_mode: instance(0.0)
                pixel: fn() {
                    let light_bg = vec4(0.91, 0.91, 0.93, 1.0)
                    let dark_bg = vec4(0.12, 0.12, 0.14, 1.0)
                    return mix(light_bg, dark_bg, self.dark_mode)
                }
            }

            title := Label{
                width: Fit
                draw_text +: {
                    text_style: mod.widgets.flex.TEXT_PANEL_TITLE{}
                    dark_mode: instance(0.0)
                    get_color: fn() {
                        let light_text = vec4(0.1, 0.1, 0.12, 1.0)
                        let dark_text = vec4(0.878, 0.878, 0.878, 1.0)
                        return mix(light_text, dark_text, self.dark_mode)
                    }
                }
                text: "observation.state"
            }

            View{ width: Fill }  // Spacer
        }

        // Content area: Y-axis on left, plot on right
        content := View{
            width: Fill
            height: Fill
            flow: Right

            // Y-axis labels (left side)
            y_axis := View{
                width: 40
                height: Fill
                flow: Down
                padding: Inset{top: 4. bottom: 4.}

                show_bg: true
                draw_bg +: {
                    dark_mode: instance(0.0)
                    pixel: fn() {
                        let light_bg = vec4(0.94, 0.94, 0.96, 1.0)
                        let dark_bg = vec4(0.10, 0.10, 0.12, 1.0)
                        return mix(light_bg, dark_bg, self.dark_mode)
                    }
                }

                max_label := Label{
                    draw_text +: {
                        text_style: mod.widgets.flex.TEXT_SMALL{}
                        dark_mode: instance(0.0)
                        get_color: fn() {
                            let light_text = vec4(0.45, 0.45, 0.5, 1.0)
                            let dark_text = vec4(0.55, 0.55, 0.58, 1.0)
                            return mix(light_text, dark_text, self.dark_mode)
                        }
                    }
                    text: "1.0"
                }

                View{ height: Fill }

                min_label := Label{
                    draw_text +: {
                        text_style: mod.widgets.flex.TEXT_SMALL{}
                        dark_mode: instance(0.0)
                        get_color: fn() {
                            let light_text = vec4(0.45, 0.45, 0.5, 1.0)
                            let dark_text = vec4(0.55, 0.55, 0.58, 1.0)
                            return mix(light_text, dark_text, self.dark_mode)
                        }
                    }
                    text: "-1.0"
                }
            }

            // Plot area
            plot_area := View{
                width: Fill
                height: Fill
                cursor: MouseCursor.Hand

                show_bg: true
                draw_bg +: {
                    dark_mode: instance(0.0)
                    pixel: fn() {
                        let light_bg = vec4(0.97, 0.97, 0.99, 1.0)
                        let dark_bg = vec4(0.08, 0.08, 0.10, 1.0)
                        return mix(light_bg, dark_bg, self.dark_mode)
                    }
                }
            }
        }
    }

    // Channel legend item
    mod.widgets.ChannelLegendItem = View{
        width: Fit
        height: 20
        spacing: 4
        align: Align{y: 0.5}

        color_dot := View{
            width: 8
            height: 8
            show_bg: true
            draw_bg +: {
                color: instance(mod.widgets.flex.COLOR_CHANNEL_0)
                border_radius: uniform(4.0)
                pixel: fn() {
                    let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                    sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius)
                    sdf.fill(self.color)
                    return sdf.result
                }
            }
        }

        name := Label{
            width: Fit
            draw_text +: {
                text_style: mod.widgets.flex.TEXT_SMALL{}
                dark_mode: instance(0.0)
                get_color: fn() {
                    let light_text = vec4(0.35, 0.35, 0.4, 1.0)
                    let dark_text = vec4(0.533, 0.533, 0.565, 1.0)
                    return mix(light_text, dark_text, self.dark_mode)
                }
            }
            text: "ch0"
        }
    }
}

// Channel colors for up to 14 channels
const CHANNEL_COLORS: [Vec4f; 14] = [
    Vec4f { x: 0.30, y: 0.55, z: 0.96, w: 1.0 },  // Blue
    Vec4f { x: 0.30, y: 0.69, z: 0.31, w: 1.0 },  // Green
    Vec4f { x: 1.00, y: 0.60, z: 0.00, w: 1.0 },  // Orange
    Vec4f { x: 0.91, y: 0.12, z: 0.39, w: 1.0 },  // Pink
    Vec4f { x: 0.61, y: 0.15, z: 0.69, w: 1.0 },  // Purple
    Vec4f { x: 0.00, y: 0.74, z: 0.83, w: 1.0 },  // Cyan
    Vec4f { x: 0.55, y: 0.76, z: 0.29, w: 1.0 },  // Light green
    Vec4f { x: 0.80, y: 0.80, z: 0.20, w: 1.0 },  // Yellow
    Vec4f { x: 0.90, y: 0.40, z: 0.40, w: 1.0 },  // Red
    Vec4f { x: 0.50, y: 0.70, z: 0.90, w: 1.0 },  // Light blue
    Vec4f { x: 0.70, y: 0.50, z: 0.70, w: 1.0 },  // Lavender
    Vec4f { x: 0.40, y: 0.80, z: 0.60, w: 1.0 },  // Mint
    Vec4f { x: 0.85, y: 0.65, z: 0.45, w: 1.0 },  // Tan
    Vec4f { x: 0.60, y: 0.60, z: 0.60, w: 1.0 },  // Gray
];

// Drawing primitive for plot lines
#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawPlotLine {
    #[deref]
    draw_super: DrawQuad,
    #[live]
    color: Vec4f,
}

#[derive(Clone, Debug)]
pub struct TimeSeriesChannel {
    pub name: String,
    pub data: Vec<(f64, f64)>,  // (timestamp, value)
    pub color: Vec4f,
    pub visible: bool,
}

#[derive(Script, ScriptHook, Widget)]
pub struct TimeSeriesPlot {
    #[deref]
    view: View,

    // Drawing primitive for lines
    #[live]
    draw_line: DrawPlotLine,

    // Data
    #[rust]
    channels: Vec<TimeSeriesChannel>,
    #[rust]
    title: String,

    // Full data range (entire episode)
    #[rust]
    data_time_range: (f64, f64),  // Full data range

    // Visible time range (sliding window)
    #[rust]
    time_range: (f64, f64),  // (start, end) in seconds - what's visible
    #[rust((-1.0, 1.0))]
    value_range: (f64, f64), // (min, max) - default to -1..1 for normalized data
    #[rust(true)]
    auto_scale_y: bool,

    // Sliding window settings
    #[rust]
    window_size: f64,  // Window size in seconds (0 = show all)
    #[rust]
    follow_cursor: bool,  // Whether to follow cursor position

    // Cursor
    #[rust]
    cursor_time: f64,
    #[rust]
    show_cursor: bool,

    // Cached plot area for drawing
    #[rust]
    plot_rect: Rect,
}

impl Widget for TimeSeriesPlot {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);

        // Handle mouse interaction for cursor
        match event.hits(cx, self.view.area()) {
            Hit::FingerDown(fe) => {
                // Convert mouse X to time
                let rect = self.view.area().rect(cx);
                let rel_x = (fe.abs.x - rect.pos.x) / rect.size.x;
                let time = self.time_range.0 + rel_x as f64 * (self.time_range.1 - self.time_range.0);
                self.cursor_time = time.clamp(self.time_range.0, self.time_range.1);
                self.show_cursor = true;

                // Emit cursor changed action
                cx.widget_action(
                    self.widget_uid(),
                    TimeSeriesPlotAction::CursorMoved(self.cursor_time),
                );

                self.view.redraw(cx);
            }
            Hit::FingerMove(fe) => {
                // Convert mouse X to time
                let rect = self.view.area().rect(cx);
                let rel_x = (fe.abs.x - rect.pos.x) / rect.size.x;
                let time = self.time_range.0 + rel_x as f64 * (self.time_range.1 - self.time_range.0);
                self.cursor_time = time.clamp(self.time_range.0, self.time_range.1);
                self.show_cursor = true;

                // Emit cursor changed action
                cx.widget_action(
                    self.widget_uid(),
                    TimeSeriesPlotAction::CursorMoved(self.cursor_time),
                );

                self.view.redraw(cx);
            }
            Hit::FingerHoverOut(_) => {
                // Optionally hide cursor on hover out
            }
            _ => {}
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        // Apply theme
        let dm = get_global_dark_mode();
        let mut header = self.view.view(cx, ids!(header));
        script_apply_eval!(cx, header, { draw_bg +: { dark_mode: #(dm) } });
        let mut title = self.view.label(cx, ids!(header.title));
        script_apply_eval!(cx, title, { draw_text +: { dark_mode: #(dm) } });
        let mut y_axis = self.view.view(cx, ids!(content.y_axis));
        script_apply_eval!(cx, y_axis, { draw_bg +: { dark_mode: #(dm) } });
        let mut max_label = self.view.label(cx, ids!(content.y_axis.max_label));
        script_apply_eval!(cx, max_label, { draw_text +: { dark_mode: #(dm) } });
        let mut min_label = self.view.label(cx, ids!(content.y_axis.min_label));
        script_apply_eval!(cx, min_label, { draw_text +: { dark_mode: #(dm) } });
        let mut plot_area = self.view.view(cx, ids!(content.plot_area));
        script_apply_eval!(cx, plot_area, { draw_bg +: { dark_mode: #(dm) } });

        // First draw the view (background, header, etc.)
        let _ = self.view.draw_walk(cx, scope, walk);

        // Get the plot area rect for drawing waveforms
        let plot_area = self.view.view(cx, ids!(content.plot_area));
        self.plot_rect = plot_area.area().rect(cx);

        // Draw the waveforms on top of the plot area
        if !self.channels.is_empty() && self.plot_rect.size.x > 10.0 && self.plot_rect.size.y > 10.0 {
            self.draw_waveforms(cx);
        }

        DrawStep::done()
    }
}

#[derive(Clone, Debug, Default)]
pub enum TimeSeriesPlotAction {
    CursorMoved(f64),
    #[default]
    None,
}

impl TimeSeriesPlot {
    /// Set the plot title
    pub fn set_title(&mut self, cx: &mut Cx, title: &str) {
        self.title = title.to_string();
        self.view.label(cx, ids!(header.title)).set_text(cx, title);
    }

    /// Set placeholder text
    pub fn set_placeholder_text(&mut self, cx: &mut Cx, text: &str) {
        self.view.label(cx, ids!(plot_area.placeholder.placeholder_label)).set_text(cx, text);
    }

    /// Set the full data time range (x-axis)
    pub fn set_time_range(&mut self, start: f64, end: f64) {
        self.data_time_range = (start, end);
        // If no window set, show all data
        if self.window_size <= 0.0 {
            self.time_range = (start, end);
        } else {
            // Initialize window at start
            self.time_range = (start, (start + self.window_size).min(end));
        }
    }

    /// Set the sliding window size in seconds
    /// Set to 0 to show all data (no sliding window)
    pub fn set_window_size(&mut self, seconds: f64) {
        self.window_size = seconds;
        self.follow_cursor = seconds > 0.0;

        // Update visible range
        if seconds > 0.0 {
            let center = self.cursor_time;
            self.update_window_position(center);
        } else {
            self.time_range = self.data_time_range;
        }
    }

    /// Update window position to center on given time
    fn update_window_position(&mut self, center_time: f64) {
        if self.window_size <= 0.0 {
            return;
        }

        let half_window = self.window_size / 2.0;
        let (data_start, data_end) = self.data_time_range;
        let data_duration = data_end - data_start;

        // If window is larger than data, show all data
        if self.window_size >= data_duration {
            self.time_range = (data_start, data_end);
            return;
        }

        // Calculate window bounds, clamping to data range
        let mut win_start = center_time - half_window;
        let mut win_end = center_time + half_window;

        // Clamp to data bounds
        if win_start < data_start {
            win_start = data_start;
            win_end = data_start + self.window_size;
        } else if win_end > data_end {
            win_end = data_end;
            win_start = data_end - self.window_size;
        }

        self.time_range = (win_start, win_end);
    }

    /// Set the value range (y-axis)
    pub fn set_value_range(&mut self, min: f64, max: f64) {
        self.value_range = (min, max);
        self.auto_scale_y = false;
    }

    /// Enable auto-scaling for Y axis
    pub fn set_auto_scale_y(&mut self, enabled: bool) {
        self.auto_scale_y = enabled;
    }

    /// Add or update a channel
    pub fn set_channel_data(&mut self, channel_idx: usize, name: &str, data: Vec<(f64, f64)>) {
        let color = CHANNEL_COLORS[channel_idx % CHANNEL_COLORS.len()];

        if channel_idx < self.channels.len() {
            self.channels[channel_idx].name = name.to_string();
            self.channels[channel_idx].data = data;
        } else {
            // Add new channel
            while self.channels.len() < channel_idx {
                self.channels.push(TimeSeriesChannel {
                    name: format!("ch{}", self.channels.len()),
                    data: vec![],
                    color: CHANNEL_COLORS[self.channels.len() % CHANNEL_COLORS.len()],
                    visible: true,
                });
            }
            self.channels.push(TimeSeriesChannel {
                name: name.to_string(),
                data,
                color,
                visible: true,
            });
        }

        // Auto-scale if enabled
        if self.auto_scale_y {
            self.compute_auto_scale();
        }
    }

    /// Clear all channels
    pub fn clear(&mut self) {
        self.channels.clear();
    }

    /// Set cursor position (from external sync)
    pub fn set_cursor_time(&mut self, cx: &mut Cx, time: f64) {
        self.cursor_time = time;
        self.show_cursor = true;

        // Update sliding window position if following cursor
        if self.follow_cursor && self.window_size > 0.0 {
            self.update_window_position(time);
            // Recompute Y-axis scale for the visible window so waveforms fill vertical space
            if self.auto_scale_y {
                self.compute_auto_scale();
                self.update_y_axis_labels(cx);
            }
        }

        self.view.redraw(cx);
    }

    /// Get interpolated values at a given time
    pub fn get_values_at_time(&self, time: f64) -> Vec<(String, f64)> {
        let mut values = Vec::new();

        for channel in &self.channels {
            if !channel.visible || channel.data.is_empty() {
                continue;
            }

            // Binary search for nearest samples
            let idx = channel.data.partition_point(|(t, _)| *t < time);

            let value = if idx == 0 {
                channel.data[0].1
            } else if idx >= channel.data.len() {
                channel.data.last().unwrap().1
            } else {
                // Linear interpolation
                let (t0, v0) = channel.data[idx - 1];
                let (t1, v1) = channel.data[idx];
                let t = (time - t0) / (t1 - t0);
                v0 + t * (v1 - v0)
            };

            values.push((channel.name.clone(), value));
        }

        values
    }

    fn compute_auto_scale(&mut self) {
        // Use visible window range so waveforms fill the vertical space
        self.compute_auto_scale_for_range(self.time_range.0, self.time_range.1);
    }

    fn compute_auto_scale_for_range(&mut self, time_start: f64, time_end: f64) {
        let mut min_val = f64::MAX;
        let mut max_val = f64::MIN;

        for channel in &self.channels {
            for (t, v) in &channel.data {
                // Only consider values within the visible time range
                if *t >= time_start && *t <= time_end {
                    min_val = min_val.min(*v);
                    max_val = max_val.max(*v);
                }
            }
        }

        if min_val < max_val {
            // Add 5% padding
            let padding = (max_val - min_val) * 0.05;
            self.value_range = (min_val - padding, max_val + padding);
        } else if !self.channels.is_empty() {
            // Fallback: if all values are the same, center around that value
            if min_val != f64::MAX {
                self.value_range = (min_val - 0.5, min_val + 0.5);
            }
        }
    }

    /// Force recompute Y-axis scale based on current data
    pub fn recompute_scale(&mut self, cx: &mut Cx) {
        if self.auto_scale_y {
            self.compute_auto_scale();
        }
        // Update Y-axis labels to reflect the actual value range
        self.update_y_axis_labels(cx);
    }

    /// Update Y-axis labels to show actual value range
    fn update_y_axis_labels(&mut self, cx: &mut Cx) {
        let max_text = format!("{:.2}", self.value_range.1);
        let min_text = format!("{:.2}", self.value_range.0);
        self.view.label(cx, ids!(content.y_axis.max_label)).set_text(cx, &max_text);
        self.view.label(cx, ids!(content.y_axis.min_label)).set_text(cx, &min_text);
    }

    /// Draw waveforms for all visible channels
    fn draw_waveforms(&mut self, cx: &mut Cx2d) {
        let rect = self.plot_rect;
        let time_span = self.time_range.1 - self.time_range.0;
        let value_span = self.value_range.1 - self.value_range.0;

        if time_span <= 0.0 || value_span <= 0.0 || rect.size.x <= 0.0 || rect.size.y <= 0.0 {
            return;
        }

        // Use full height with small padding
        let padding = 4.0;
        let draw_top = rect.pos.y + padding;
        let draw_height = rect.size.y - (padding * 2.0);

        if draw_height <= 0.0 {
            return;
        }

        let visible_start = self.time_range.0;
        let visible_end = self.time_range.1;

        // Draw each channel as vertical bars (1 pixel wide) connecting consecutive points
        // This works well for dense time series data
        for channel in &self.channels {
            if !channel.visible || channel.data.is_empty() {
                continue;
            }

            self.draw_line.color = channel.color;

            let mut prev_y: Option<f64> = None;
            let mut prev_x: Option<f64> = None;

            for (t, v) in &channel.data {
                // Skip points outside visible range
                if *t < visible_start || *t > visible_end {
                    prev_y = None;
                    prev_x = None;
                    continue;
                }

                // Map time to x coordinate
                let x = rect.pos.x + ((*t - visible_start) / time_span) * rect.size.x;

                // Map value to y coordinate (inverted: high values at top)
                let y_norm = ((*v - self.value_range.0) / value_span).clamp(0.0, 1.0);
                let y = draw_top + (1.0 - y_norm) * draw_height;

                // Draw a vertical bar from prev_y to current y at this x position
                if let (Some(py), Some(px)) = (prev_y, prev_x) {
                    let min_y = py.min(y);
                    let max_y = py.max(y);
                    let bar_height = (max_y - min_y).max(2.0);

                    // Draw vertical bar
                    self.draw_line.draw_abs(
                        cx,
                        Rect {
                            pos: dvec2(px, min_y),
                            size: dvec2((x - px).max(1.0), bar_height),
                        },
                    );
                }

                prev_y = Some(y);
                prev_x = Some(x);
            }
        }
    }
}

impl TimeSeriesPlotRef {
    pub fn set_title(&self, cx: &mut Cx, title: &str) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_title(cx, title);
        }
    }

    pub fn set_placeholder_text(&self, cx: &mut Cx, text: &str) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_placeholder_text(cx, text);
        }
    }

    pub fn set_channel_data(&self, channel_idx: usize, name: &str, data: Vec<(f64, f64)>) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_channel_data(channel_idx, name, data);
        }
    }

    pub fn set_cursor_time(&self, cx: &mut Cx, time: f64) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_cursor_time(cx, time);
        }
    }

    pub fn set_time_range(&self, start: f64, end: f64) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_time_range(start, end);
        }
    }

    pub fn set_auto_scale_y(&self, enabled: bool) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_auto_scale_y(enabled);
        }
    }

    pub fn set_window_size(&self, seconds: f64) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_window_size(seconds);
        }
    }

    pub fn recompute_scale(&self, cx: &mut Cx) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.recompute_scale(cx);
        }
    }

    pub fn clear(&self) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.clear();
        }
    }
}
