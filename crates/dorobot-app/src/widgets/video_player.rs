//! Video Player Widget for LeRobot camera feeds

use makepad_widgets::*;
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
        draw_bg: { color: #000000 }

        flow: Overlay

        // Video frame display
        video_area = <View> {
            width: Fill
            height: Fill
            align: { x: 0.5, y: 0.5 }

            // Image widget for frame display
            frame_image = <Image> {
                width: Fill
                height: Fill
                fit: Smallest
                visible: false
            }

            // No video placeholder
            placeholder = <View> {
                width: Fit
                height: Fit
                align: { x: 0.5, y: 0.5 }

                placeholder_label = <Label> {
                    draw_text: {
                        text_style: <TEXT_BODY> {}
                        color: (COLOR_TEXT_MUTED)
                        wrap: Word
                    }
                    text: "No video loaded"
                }
            }
        }

        // Camera label overlay (top left)
        camera_overlay = <View> {
            width: Fill
            height: Fill
            padding: 8

            camera_label = <RoundedView> {
                width: Fit
                height: Fit
                padding: { left: 8, right: 8, top: 4, bottom: 4 }

                draw_bg: {
                    color: #00000080
                }

                label = <Label> {
                    draw_text: {
                        text_style: <TEXT_SMALL> {}
                        color: #ffffff
                    }
                    text: "Camera"
                }
            }
        }

        // Frame info overlay (bottom right)
        info_overlay = <View> {
            width: Fill
            height: Fill
            padding: 8
            align: { x: 1.0, y: 1.0 }

            frame_info = <RoundedView> {
                width: Fit
                height: Fit
                padding: { left: 8, right: 8, top: 4, bottom: 4 }

                draw_bg: {
                    color: #00000080
                }

                label = <Label> {
                    draw_text: {
                        text_style: <TEXT_MONO> {}
                        color: #ffffff
                    }
                    text: "Frame: 0 / 0"
                }
            }
        }
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
    camera_name: String,
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
}

impl Widget for VideoPlayer {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
}

impl VideoPlayer {
    /// Set the camera name label
    pub fn set_camera_name(&mut self, cx: &mut Cx, name: &str) {
        self.camera_name = name.to_string();
        self.view.label(id!(camera_overlay.camera_label.label)).set_text(cx, name);
    }

    /// Update frame info display
    pub fn set_frame_info(&mut self, cx: &mut Cx, current: u64, total: u64) {
        self.current_frame = current;
        self.total_frames = total;
        let text = format!("Frame: {} / {}", current, total);
        self.view.label(id!(info_overlay.frame_info.label)).set_text(cx, &text);
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
        // Convert RGB24 to the format expected by the Image widget
        let bgra_data = Self::rgb_to_bgra_u32(data);

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

        // Debug: write to file (only log occasionally to avoid spam)
        if episode_frame == 0 || episode_frame % 100 == 0 {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/dorobot_debug.log") {
                let _ = writeln!(f, "[VideoPlayer] show_frame_at_time: time={:.2}s, episode_frame={}, offset={}, absolute_frame={}, absolute_time={:.2}s",
                    time, episode_frame, self.episode_frame_offset, absolute_frame, absolute_time);
            }
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
                    ::log::warn!("Failed to decode frame at time {} (absolute: {}): {}", time, absolute_time, e);
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
        // Debug: write to file
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/dorobot_debug.log") {
            let _ = writeln!(f, "[VideoPlayer] load_video called with path: {}", path);
        }

        match VideoDecoder::open(path) {
            Ok(decoder) => {
                if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/dorobot_debug.log") {
                    let _ = writeln!(f, "[VideoPlayer] Successfully opened video: {} frames, {} fps", decoder.frame_count(), decoder.fps());
                }
                self.total_frames = decoder.frame_count();
                self.fps = decoder.fps();
                self.has_video = true;
                self.video_decoder = Some(decoder);
                self.placeholder_decoder = None;

                // Show first frame
                self.show_frame_at_time(cx, 0.0);
                Ok(())
            }
            Err(e) => {
                if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("/tmp/dorobot_debug.log") {
                    let _ = writeln!(f, "[VideoPlayer] Failed to load video {}: {}", path, e);
                }
                Err(format!("Failed to load video: {}", e))
            }
        }
    }

    /// Load video - stub when video feature not enabled
    #[cfg(not(feature = "video"))]
    pub fn load_video(&mut self, _cx: &mut Cx, _path: &str) -> Result<(), String> {
        Err("Video feature not enabled. Build with --features video".to_string())
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

impl VideoPlayerRef {
    pub fn set_camera_name(&self, cx: &mut Cx, name: &str) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_camera_name(cx, name);
        }
    }

    pub fn set_frame_info(&self, cx: &mut Cx, current: u64, total: u64) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_frame_info(cx, current, total);
        }
    }

    pub fn set_placeholder_text(&self, cx: &mut Cx, text: &str) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_placeholder_text(cx, text);
        }
    }

    pub fn set_frame(&self, cx: &mut Cx, width: usize, height: usize, data: &[u8]) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_frame(cx, width, height, data);
        }
    }

    pub fn clear(&self, cx: &mut Cx) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.clear(cx);
        }
    }

    pub fn init_placeholder(&self, width: u32, height: u32, fps: f64, frame_count: u64) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.init_placeholder(width, height, fps, frame_count);
        }
    }

    pub fn show_frame_at_time(&self, cx: &mut Cx, time: f64) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.show_frame_at_time(cx, time);
        }
    }

    pub fn set_has_video(&self, cx: &mut Cx, has_video: bool) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_has_video(cx, has_video);
        }
    }

    pub fn load_video(&self, cx: &mut Cx, path: &str) -> Result<(), String> {
        if let Some(mut inner) = self.borrow_mut() {
            inner.load_video(cx, path)
        } else {
            Err("Failed to borrow VideoPlayer".to_string())
        }
    }

    pub fn set_episode_info(&self, frame_offset: u64, frame_count: u64) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_episode_info(frame_offset, frame_count);
        }
    }
}
