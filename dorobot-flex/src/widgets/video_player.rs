//! Video Player Widget for LeRobot camera feeds
//!
//! ## Overlay Positioning Limitation
//!
//! When video aspect ratio differs from container, Makepad's `fit: Smallest` scales
//! and centers the video, leaving empty space. The overlay needs to be positioned
//! relative to the actual video bounds, not the container.
//!
//! ### Approaches Tried (all failed):
//!
//! 1. **Dynamic margin/padding** - Calculate empty space and apply via `apply_over`
//!    in `draw_walk`. Math was correct but layout wasn't updated.
//!
//! 2. **Absolute positioning (abs_pos)** - Calculate video bounds and position overlay.
//!    Labels disappeared - abs_pos didn't work correctly.
//!
//! 3. **Fit-sized container** - `width: Fit, height: Fit` on Image to shrink to content.
//!    Video stopped rendering - Image widget needs Fill for textures.
//!
//! ### Root Cause:
//! Makepad's Image widget with `fit: Smallest` occupies full container space but
//! renders content smaller and centered. No API exposes actual rendered bounds.
//! Child positioning is relative to widget bounds, not content bounds.
//!
//! ### Solution:
//! Render overlay text directly onto video frame buffer in `set_frame()` before
//! creating the texture. This burns the text into video pixels, guaranteeing it
//! appears within video bounds regardless of aspect ratio.

use makepad_widgets::*;
use makepad_app_shell::theme::get_global_dark_mode;
use crate::data::video_decoder::PlaceholderDecoder;

#[cfg(feature = "video")]
use crate::data::video_decoder::VideoDecoder;

live_design! {
    use link::theme::*;
    use link::shaders::*;
    use link::widgets::*;
    use crate::shared::styles::*;

    // Main video player widget
    pub VideoPlayer = {{VideoPlayer}} {
        width: Fill
        height: Fill

        show_bg: true
        draw_bg: {
            instance dark_mode: 0.0
            fn pixel(self) -> vec4 {
                let light_bg = vec4(0.97, 0.97, 0.98, 1.0);  // #f8f8fa
                let dark_bg = vec4(0.05, 0.05, 0.06, 1.0);   // near black
                return mix(light_bg, dark_bg, self.dark_mode);
            }
        }

        flow: Overlay

        // Video frame display
        video_area = <View> {
            width: Fill
            height: Fill
            align: { x: 0.5, y: 0.5 }

            frame_image = <Image> {
                width: Fill
                height: Fill
                fit: Smallest
                visible: false
            }

            placeholder = <View> {
                width: Fit
                height: Fit
                align: { x: 0.5, y: 0.5 }

                placeholder_label = <Label> {
                    draw_text: {
                        text_style: <TEXT_BODY> {}
                        instance dark_mode: 0.0
                        fn get_color(self) -> vec4 {
                            let light_text = vec4(0.33, 0.33, 0.33, 1.0);
                            let dark_text = vec4(0.6, 0.6, 0.6, 1.0);
                            return mix(light_text, dark_text, self.dark_mode);
                        }
                        wrap: Word
                    }
                    text: "No video loaded"
                }
            }
        }

        // Note: Frame info is now rendered directly onto video frames
        // See render_text_on_frame() for the baked-in overlay implementation
    }
}

#[derive(Live, LiveHook, Widget)]
pub struct VideoPlayer {
    #[deref]
    view: View,

    // Video state
    #[rust]
    current_frame: u64,
    #[rust]
    total_frames: u64,
    #[rust]
    fps: f64,
    #[rust]
    has_video: bool,

    /// Episode frame offset in the video file
    /// In LeRobot v3.0, all episodes are concatenated in one MP4.
    /// This offset indicates where the current episode starts.
    #[rust]
    episode_frame_offset: u64,

    /// Number of frames in the current episode
    #[rust]
    episode_frame_count: u64,

    // Placeholder decoder for demo
    #[rust]
    placeholder_decoder: Option<PlaceholderDecoder>,

    // Real video decoder (when video feature enabled)
    #[cfg(feature = "video")]
    #[rust]
    video_decoder: Option<VideoDecoder>,

    /// Native video dimensions for overlay positioning
    #[rust]
    video_width: usize,
    #[rust]
    video_height: usize,

    /// Cached overlay text to render onto video frames
    #[rust]
    overlay_text: String,
}

// Simple 5x7 bitmap font for digits, colon, space, slash, period, 'f', 'p', 's', 'F', 'r', 'a', 'm', 'e'
const FONT_WIDTH: usize = 5;
const FONT_HEIGHT: usize = 7;

fn get_char_bitmap(c: char) -> Option<[u8; 7]> {
    match c {
        '0' => Some([0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110]),
        '1' => Some([0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110]),
        '2' => Some([0b01110, 0b10001, 0b00001, 0b00110, 0b01000, 0b10000, 0b11111]),
        '3' => Some([0b01110, 0b10001, 0b00001, 0b00110, 0b00001, 0b10001, 0b01110]),
        '4' => Some([0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010]),
        '5' => Some([0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110]),
        '6' => Some([0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110]),
        '7' => Some([0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000]),
        '8' => Some([0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110]),
        '9' => Some([0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100]),
        ' ' => Some([0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b00000]),
        ':' => Some([0b00000, 0b00100, 0b00100, 0b00000, 0b00100, 0b00100, 0b00000]),
        '/' => Some([0b00001, 0b00010, 0b00010, 0b00100, 0b01000, 0b01000, 0b10000]),
        '.' => Some([0b00000, 0b00000, 0b00000, 0b00000, 0b00000, 0b01100, 0b01100]),
        'f' => Some([0b00110, 0b01000, 0b01000, 0b11100, 0b01000, 0b01000, 0b01000]),
        'p' => Some([0b00000, 0b00000, 0b11110, 0b10001, 0b11110, 0b10000, 0b10000]),
        's' => Some([0b00000, 0b00000, 0b01111, 0b10000, 0b01110, 0b00001, 0b11110]),
        'F' => Some([0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000]),
        'r' => Some([0b00000, 0b00000, 0b10110, 0b11001, 0b10000, 0b10000, 0b10000]),
        'a' => Some([0b00000, 0b00000, 0b01110, 0b00001, 0b01111, 0b10001, 0b01111]),
        'm' => Some([0b00000, 0b00000, 0b11010, 0b10101, 0b10101, 0b10001, 0b10001]),
        'e' => Some([0b00000, 0b00000, 0b01110, 0b10001, 0b11111, 0b10000, 0b01110]),
        _ => None,
    }
}

impl Widget for VideoPlayer {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        // Apply theme
        let dm = get_global_dark_mode();
        self.view.apply_over(cx, live! { draw_bg: { dark_mode: (dm) } });
        self.view.label(id!(video_area.placeholder.placeholder_label)).apply_over(cx, live! {
            draw_text: { dark_mode: (dm) }
        });

        self.view.draw_walk(cx, scope, walk)
    }
}

impl VideoPlayer {
    /// Update frame info display (rendered onto video frame)
    pub fn set_frame_info(&mut self, _cx: &mut Cx, current: u64, total: u64) {
        self.current_frame = current;
        self.total_frames = total;
        // Update overlay text for rendering onto video frame
        self.overlay_text = format!("{}/{} {:.1}fps", current, total, self.fps);
    }

    /// Update FPS display (stored for rendering onto video frame)
    pub fn set_fps_display(&mut self, _cx: &mut Cx, fps: f64) {
        self.fps = fps;
        // Overlay text will be updated in set_frame_info
    }

    /// Show or hide placeholder
    pub fn set_has_video(&mut self, cx: &mut Cx, has_video: bool) {
        self.has_video = has_video;
        self.view.view(id!(video_area.placeholder)).set_visible(cx, !has_video);
        self.view.image(id!(video_area.frame_image)).apply_over(cx, live! {
            visible: (has_video)
        });
        self.view.redraw(cx);
    }

    /// Clear video display
    pub fn clear(&mut self, cx: &mut Cx) {
        self.has_video = false;
        self.placeholder_decoder = None;
        #[cfg(feature = "video")]
        {
            self.video_decoder = None;
        }
        // Clear texture by setting to None
        let image = self.view.image(id!(video_area.frame_image));
        image.set_texture(cx, None);
        self.view.view(id!(video_area.placeholder)).set_visible(cx, true);
        self.view.image(id!(video_area.frame_image)).apply_over(cx, live! {
            visible: false
        });
        self.view.redraw(cx);
    }

    /// Set placeholder text
    pub fn set_placeholder_text(&mut self, cx: &mut Cx, text: &str) {
        self.view.label(id!(video_area.placeholder.placeholder_label)).set_text(cx, text);
    }

    /// Initialize with placeholder decoder for demo mode
    pub fn init_placeholder(&mut self, width: u32, height: u32, fps: f64, frame_count: u64) {
        self.placeholder_decoder = Some(PlaceholderDecoder::new(width, height, fps, frame_count));
        self.total_frames = frame_count;
        self.fps = fps;  // Set fps for frame time calculations
        self.has_video = true;
    }

    /// Set frame from RGB24 data
    pub fn set_frame(&mut self, cx: &mut Cx, width: usize, height: usize, data: &[u8]) {
        // Track video dimensions for overlay positioning
        self.video_width = width;
        self.video_height = height;

        // Debug: log frame setting (occasional)
        static FRAME_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let count = FRAME_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if count % 100 == 0 {
            ::log::trace!("[VideoPlayer] set_frame called: {}x{}, data_len={}, count={}", width, height, data.len(), count);
        }

        // Render overlay text onto frame buffer
        let mut frame_data = data.to_vec();
        if !self.overlay_text.is_empty() {
            Self::render_text_on_frame(&mut frame_data, width, height, &self.overlay_text);
        }

        // Convert RGB24 to the format expected by the Image widget
        let bgra_data = Self::rgb_to_bgra_u32(&frame_data);

        // Create texture from RGB data
        let texture = Texture::new_with_format(cx, TextureFormat::VecBGRAu8_32 {
            data: Some(bgra_data),
            width,
            height,
            updated: TextureUpdated::Full,
        });

        // Set texture on the image widget
        let image = self.view.image(id!(video_area.frame_image));
        image.set_texture(cx, Some(texture));

        // Make frame visible and hide placeholder
        self.view.image(id!(video_area.frame_image)).apply_over(cx, live! {
            visible: true
        });
        self.view.view(id!(video_area.placeholder)).set_visible(cx, false);

        self.has_video = true;
        self.view.redraw(cx);
    }

    /// Set episode info for seeking within concatenated video files
    ///
    /// In LeRobot v3.0, all episodes are stored in a single MP4 file.
    /// This sets the frame offset where the current episode starts.
    pub fn set_episode_info(&mut self, frame_offset: u64, frame_count: u64) {
        self.episode_frame_offset = frame_offset;
        self.episode_frame_count = frame_count;
    }

    /// Display frame at given episode-relative time
    ///
    /// The time is relative to the episode start (0.0 = first frame of episode).
    /// Internally, we add the episode frame offset to seek to the correct position.
    pub fn show_frame_at_time(&mut self, cx: &mut Cx, time: f64) {
        // Calculate absolute frame index in the video file
        let episode_frame = (time * self.fps) as u64;
        let absolute_frame = self.episode_frame_offset + episode_frame;
        let absolute_time = absolute_frame as f64 / self.fps.max(1.0);

        // Debug: log occasionally to avoid spam
        if episode_frame == 0 || episode_frame % 100 == 0 {
            ::log::trace!("[VideoPlayer] show_frame_at_time: time={:.2}s, episode_frame={}, offset={}, absolute_frame={}, absolute_time={:.2}s",
                time, episode_frame, self.episode_frame_offset, absolute_frame, absolute_time);
        }

        // Try real video decoder first (when feature enabled)
        #[cfg(feature = "video")]
        if let Some(decoder) = &mut self.video_decoder {
            match decoder.get_frame_at_time(absolute_time) {
                Ok(frame) => {
                    self.current_frame = episode_frame;
                    self.set_frame(cx, frame.width as usize, frame.height as usize, &frame.data);
                    self.set_frame_info(cx, episode_frame, self.episode_frame_count);
                    return;
                }
                Err(e) => {
                    ::log::warn!("[VideoPlayer] ERROR decoding frame at time {} (absolute: {}): {}", time, absolute_time, e);
                }
            }
        }

        // Fall back to placeholder decoder
        if let Some(decoder) = &self.placeholder_decoder {
            let frame = decoder.get_frame_at_time(time);
            self.current_frame = frame.frame_index;
            self.set_frame(cx, frame.width as usize, frame.height as usize, &frame.data);
            self.set_frame_info(cx, frame.frame_index, self.total_frames);
        }
    }

    /// Load video from file path (requires video feature)
    #[cfg(feature = "video")]
    pub fn load_video(&mut self, cx: &mut Cx, path: &str) -> Result<(), String> {
        ::log::debug!("[VideoPlayer] load_video called with path: {}", path);

        match VideoDecoder::open(path) {
            Ok(decoder) => {
                let video_fps = decoder.fps();
                ::log::info!("[VideoPlayer] Successfully opened video: {} frames, {} fps", decoder.frame_count(), video_fps);
                self.total_frames = decoder.frame_count();
                self.fps = video_fps;
                self.has_video = true;
                self.video_decoder = Some(decoder);
                self.placeholder_decoder = None;

                // Update FPS display overlay
                self.set_fps_display(cx, self.fps);

                // Show first frame
                self.show_frame_at_time(cx, 0.0);
                Ok(())
            }
            Err(e) => {
                ::log::warn!("[VideoPlayer] Failed to load video {}: {}", path, e);
                Err(format!("Failed to load video: {}", e))
            }
        }
    }

    /// Load video - stub when video feature not enabled
    #[cfg(not(feature = "video"))]
    pub fn load_video(&mut self, _cx: &mut Cx, _path: &str) -> Result<(), String> {
        Err("Video feature not enabled. Build with --features video".to_string())
    }

    /// Render text onto RGB24 frame buffer at top-right corner (crisp pixels)
    fn render_text_on_frame(data: &mut [u8], width: usize, height: usize, text: &str) {
        let scale = 2; // 2x scale - crisp and readable
        let char_width = (FONT_WIDTH + 1) * scale; // +1 for letter spacing
        let char_height = FONT_HEIGHT * scale;
        let padding = 8;
        let bg_padding = 4;

        // Calculate text dimensions
        let text_width = text.len() * char_width;
        let text_height = char_height;

        // Position at top-right
        let text_x = width.saturating_sub(text_width + padding);
        let text_y = padding;

        // Draw solid dark background
        let bg_x1 = text_x.saturating_sub(bg_padding);
        let bg_y1 = text_y.saturating_sub(bg_padding);
        let bg_x2 = (text_x + text_width + bg_padding).min(width);
        let bg_y2 = (text_y + text_height + bg_padding).min(height);

        for y in bg_y1..bg_y2 {
            for x in bg_x1..bg_x2 {
                let idx = (y * width + x) * 3;
                if idx + 2 < data.len() {
                    // Dark background
                    data[idx] = 0;
                    data[idx + 1] = 0;
                    data[idx + 2] = 0;
                }
            }
        }

        // Draw each character - sharp pixels only
        for (char_idx, c) in text.chars().enumerate() {
            if let Some(bitmap) = get_char_bitmap(c) {
                let base_x = text_x + char_idx * char_width;

                for row in 0..FONT_HEIGHT {
                    for col in 0..FONT_WIDTH {
                        if (bitmap[row] >> (FONT_WIDTH - 1 - col)) & 1 == 1 {
                            // Draw scaled pixel block
                            for sy in 0..scale {
                                for sx in 0..scale {
                                    let px = base_x + col * scale + sx;
                                    let py = text_y + row * scale + sy;
                                    if px < width && py < height {
                                        let idx = (py * width + px) * 3;
                                        if idx + 2 < data.len() {
                                            data[idx] = 255;
                                            data[idx + 1] = 255;
                                            data[idx + 2] = 255;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Convert RGB24 to BGRA u32 array for texture
    fn rgb_to_bgra_u32(data: &[u8]) -> Vec<u32> {
        data.chunks(3)
            .map(|chunk| {
                if chunk.len() == 3 {
                    // BGRA format as u32 (little endian: ARGB in memory)
                    let b = chunk[2] as u32;
                    let g = chunk[1] as u32;
                    let r = chunk[0] as u32;
                    let a = 255u32;
                    (a << 24) | (r << 16) | (g << 8) | b
                } else {
                    0
                }
            })
            .collect()
    }
}

/// Macro to generate VideoPlayerRef delegate methods that take &mut Cx
macro_rules! delegate_ref_method {
    // Method with cx only
    ($name:ident, cx) => {
        pub fn $name(&self, cx: &mut Cx) {
            if let Some(mut inner) = self.borrow_mut() {
                inner.$name(cx);
            }
        }
    };
    // Method with cx and one parameter
    ($name:ident, cx, $p1:ident: $t1:ty) => {
        pub fn $name(&self, cx: &mut Cx, $p1: $t1) {
            if let Some(mut inner) = self.borrow_mut() {
                inner.$name(cx, $p1);
            }
        }
    };
    // Method with cx and two parameters
    ($name:ident, cx, $p1:ident: $t1:ty, $p2:ident: $t2:ty) => {
        pub fn $name(&self, cx: &mut Cx, $p1: $t1, $p2: $t2) {
            if let Some(mut inner) = self.borrow_mut() {
                inner.$name(cx, $p1, $p2);
            }
        }
    };
    // Method with cx and three parameters
    ($name:ident, cx, $p1:ident: $t1:ty, $p2:ident: $t2:ty, $p3:ident: $t3:ty) => {
        pub fn $name(&self, cx: &mut Cx, $p1: $t1, $p2: $t2, $p3: $t3) {
            if let Some(mut inner) = self.borrow_mut() {
                inner.$name(cx, $p1, $p2, $p3);
            }
        }
    };
    // Method without cx, two parameters
    ($name:ident, $p1:ident: $t1:ty, $p2:ident: $t2:ty) => {
        pub fn $name(&self, $p1: $t1, $p2: $t2) {
            if let Some(mut inner) = self.borrow_mut() {
                inner.$name($p1, $p2);
            }
        }
    };
    // Method without cx, four parameters
    ($name:ident, $p1:ident: $t1:ty, $p2:ident: $t2:ty, $p3:ident: $t3:ty, $p4:ident: $t4:ty) => {
        pub fn $name(&self, $p1: $t1, $p2: $t2, $p3: $t3, $p4: $t4) {
            if let Some(mut inner) = self.borrow_mut() {
                inner.$name($p1, $p2, $p3, $p4);
            }
        }
    };
}

impl VideoPlayerRef {
    delegate_ref_method!(set_frame_info, cx, current: u64, total: u64);
    delegate_ref_method!(set_fps_display, cx, fps: f64);
    delegate_ref_method!(set_placeholder_text, cx, text: &str);
    delegate_ref_method!(set_frame, cx, width: usize, height: usize, data: &[u8]);
    delegate_ref_method!(clear, cx);
    delegate_ref_method!(show_frame_at_time, cx, time: f64);
    delegate_ref_method!(set_has_video, cx, has_video: bool);
    delegate_ref_method!(init_placeholder, width: u32, height: u32, fps: f64, frame_count: u64);
    delegate_ref_method!(set_episode_info, frame_offset: u64, frame_count: u64);

    /// Load video - returns Result, so can't use macro
    pub fn load_video(&self, cx: &mut Cx, path: &str) -> Result<(), String> {
        if let Some(mut inner) = self.borrow_mut() {
            inner.load_video(cx, path)
        } else {
            Err("Failed to borrow VideoPlayer".to_string())
        }
    }
}
