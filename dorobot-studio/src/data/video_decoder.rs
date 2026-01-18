//! Video Decoder for LeRobot MP4 files
//!
//! Extracts frames from MP4 video files for display in the UI.
//!
//! Enable the `video` feature to use FFmpeg-based decoding:
//! ```toml
//! [dependencies]
//! dorobot-app = { features = ["video"] }
//! ```

use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum VideoError {
    #[error("FFmpeg error: {0}")]
    Ffmpeg(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Video file not found: {0}")]
    NotFound(String),
    #[error("Invalid frame index: {0}")]
    InvalidFrame(u64),
    #[error("Video feature not enabled")]
    FeatureNotEnabled,
}

/// RGB frame data
#[derive(Clone)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,  // RGB24 format
    pub frame_index: u64,
    pub timestamp: f64,
}

/// Video decoder for extracting frames from MP4 files
///
/// Optimized for sequential playback - avoids seeking when reading consecutive frames.
#[cfg(feature = "video")]
pub struct VideoDecoder {
    path: std::path::PathBuf,
    width: u32,
    height: u32,
    fps: f64,
    frame_count: u64,
    duration: f64,
    decoder: Option<DecoderState>,
    /// Last successfully decoded frame index (for sequential optimization)
    last_frame_index: Option<u64>,
}

#[cfg(feature = "video")]
struct DecoderState {
    input_ctx: ffmpeg_next::format::context::Input,
    decoder: ffmpeg_next::decoder::Video,
    video_stream_index: usize,
    scaler: ffmpeg_next::software::scaling::Context,
}

#[cfg(feature = "video")]
impl VideoDecoder {
    /// Open a video file for decoding
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, VideoError> {
        let path = path.as_ref().to_path_buf();

        if !path.exists() {
            return Err(VideoError::NotFound(path.display().to_string()));
        }

        // Initialize FFmpeg
        ffmpeg_next::init().map_err(|e| VideoError::Ffmpeg(e.to_string()))?;

        // Open input
        let input_ctx = ffmpeg_next::format::input(&path)
            .map_err(|e| VideoError::Ffmpeg(e.to_string()))?;

        // Find video stream
        let video_stream = input_ctx.streams()
            .best(ffmpeg_next::media::Type::Video)
            .ok_or_else(|| VideoError::Ffmpeg("No video stream found".to_string()))?;

        let video_stream_index = video_stream.index();

        // Get video parameters
        let codec_params = video_stream.parameters();
        let decoder_ctx = ffmpeg_next::codec::context::Context::from_parameters(codec_params)
            .map_err(|e| VideoError::Ffmpeg(e.to_string()))?;

        let decoder = decoder_ctx.decoder().video()
            .map_err(|e| VideoError::Ffmpeg(e.to_string()))?;

        let width = decoder.width();
        let height = decoder.height();

        // Get frame rate
        let fps = video_stream.avg_frame_rate();
        let fps = if fps.1 != 0 { fps.0 as f64 / fps.1 as f64 } else { 30.0 };

        // Get duration
        let duration = if video_stream.duration() > 0 {
            video_stream.duration() as f64 * f64::from(video_stream.time_base())
        } else {
            input_ctx.duration() as f64 / 1_000_000.0
        };

        let frame_count = (duration * fps) as u64;

        // Create scaler for RGB conversion
        let scaler = ffmpeg_next::software::scaling::Context::get(
            decoder.format(),
            width,
            height,
            ffmpeg_next::format::Pixel::RGB24,
            width,
            height,
            ffmpeg_next::software::scaling::Flags::BILINEAR,
        ).map_err(|e| VideoError::Ffmpeg(e.to_string()))?;

        // Re-open for seeking
        let input_ctx = ffmpeg_next::format::input(&path)
            .map_err(|e| VideoError::Ffmpeg(e.to_string()))?;

        let video_stream = input_ctx.streams()
            .best(ffmpeg_next::media::Type::Video)
            .ok_or_else(|| VideoError::Ffmpeg("No video stream found".to_string()))?;

        let codec_params = video_stream.parameters();
        let decoder_ctx = ffmpeg_next::codec::context::Context::from_parameters(codec_params)
            .map_err(|e| VideoError::Ffmpeg(e.to_string()))?;

        let decoder = decoder_ctx.decoder().video()
            .map_err(|e| VideoError::Ffmpeg(e.to_string()))?;

        Ok(Self {
            path,
            width,
            height,
            fps,
            frame_count,
            duration,
            decoder: Some(DecoderState {
                input_ctx,
                decoder,
                video_stream_index,
                scaler,
            }),
            last_frame_index: None,
        })
    }

    pub fn width(&self) -> u32 { self.width }
    pub fn height(&self) -> u32 { self.height }
    pub fn fps(&self) -> f64 { self.fps }
    pub fn frame_count(&self) -> u64 { self.frame_count }
    pub fn duration(&self) -> f64 { self.duration }

    pub fn get_frame_at_time(&mut self, time: f64) -> Result<VideoFrame, VideoError> {
        let frame_index = (time * self.fps) as u64;
        self.get_frame(frame_index)
    }

    pub fn get_frame(&mut self, frame_index: u64) -> Result<VideoFrame, VideoError> {
        if frame_index >= self.frame_count {
            return Err(VideoError::InvalidFrame(frame_index));
        }

        let state = self.decoder.as_mut()
            .ok_or_else(|| VideoError::Ffmpeg("Decoder not initialized".to_string()))?;

        // Check if we need to seek or can continue sequentially
        let need_seek = match self.last_frame_index {
            // First frame or jumping backward always needs seek
            None => true,
            Some(last) if frame_index <= last => true,
            // Small forward jump (1-3 frames) - continue sequential read
            Some(last) if frame_index <= last + 3 => false,
            // Large forward jump - seek is more efficient
            Some(_) => true,
        };

        if need_seek {
            let timestamp = (frame_index as f64 / self.fps * 1_000_000.0) as i64;
            state.input_ctx.seek(timestamp, timestamp..)
                .map_err(|e| VideoError::Ffmpeg(e.to_string()))?;

            // Flush decoder after seek
            state.decoder.flush();
        }

        let mut decoded_frame = ffmpeg_next::frame::Video::empty();
        let mut rgb_frame = ffmpeg_next::frame::Video::empty();

        for (stream, packet) in state.input_ctx.packets() {
            if stream.index() != state.video_stream_index {
                continue;
            }

            state.decoder.send_packet(&packet)
                .map_err(|e| VideoError::Ffmpeg(e.to_string()))?;

            while state.decoder.receive_frame(&mut decoded_frame).is_ok() {
                let frame_pts = decoded_frame.timestamp().unwrap_or(0);
                let time_base = f64::from(stream.time_base());
                let frame_time = frame_pts as f64 * time_base;

                // Calculate which frame index this decoded frame corresponds to
                let decoded_index = (frame_time * self.fps) as u64;

                // Skip frames until we reach or pass the target
                if decoded_index < frame_index && !need_seek {
                    continue;
                }

                state.scaler.run(&decoded_frame, &mut rgb_frame)
                    .map_err(|e| VideoError::Ffmpeg(e.to_string()))?;

                // Update last frame index for sequential optimization
                self.last_frame_index = Some(frame_index);

                return Ok(VideoFrame {
                    width: self.width,
                    height: self.height,
                    data: rgb_frame.data(0).to_vec(),
                    frame_index,
                    timestamp: frame_time,
                });
            }
        }

        Err(VideoError::Ffmpeg("Failed to decode frame".to_string()))
    }

    /// Reset sequential state (call when switching episodes or videos)
    pub fn reset_sequential_state(&mut self) {
        self.last_frame_index = None;
    }
}

/// Stub decoder when video feature is not enabled
#[cfg(not(feature = "video"))]
pub struct VideoDecoder;

#[cfg(not(feature = "video"))]
impl VideoDecoder {
    pub fn open<P: AsRef<Path>>(_path: P) -> Result<Self, VideoError> {
        Err(VideoError::FeatureNotEnabled)
    }
}

/// Placeholder video decoder for testing without FFmpeg
pub struct PlaceholderDecoder {
    width: u32,
    height: u32,
    fps: f64,
    frame_count: u64,
}

impl PlaceholderDecoder {
    pub fn new(width: u32, height: u32, fps: f64, frame_count: u64) -> Self {
        Self { width, height, fps, frame_count }
    }

    pub fn width(&self) -> u32 { self.width }
    pub fn height(&self) -> u32 { self.height }
    pub fn fps(&self) -> f64 { self.fps }
    pub fn frame_count(&self) -> u64 { self.frame_count }

    pub fn get_frame(&self, frame_index: u64) -> VideoFrame {
        let mut data = vec![0u8; (self.width * self.height * 3) as usize];

        // Generate a simple gradient pattern based on frame
        let hue = (frame_index as f32 / self.frame_count.max(1) as f32) * 360.0;
        let (r, g, b) = hsv_to_rgb(hue, 0.5, 0.5);

        for y in 0..self.height {
            for x in 0..self.width {
                let idx = ((y * self.width + x) * 3) as usize;
                let brightness = ((x + y) % 32) as f32 / 32.0 * 0.3 + 0.7;
                data[idx] = (r * brightness * 255.0) as u8;
                data[idx + 1] = (g * brightness * 255.0) as u8;
                data[idx + 2] = (b * brightness * 255.0) as u8;
            }
        }

        VideoFrame {
            width: self.width,
            height: self.height,
            data,
            frame_index,
            timestamp: frame_index as f64 / self.fps,
        }
    }

    pub fn get_frame_at_time(&self, time: f64) -> VideoFrame {
        let frame_index = (time * self.fps) as u64;
        self.get_frame(frame_index)
    }
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> (f32, f32, f32) {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;

    let (r, g, b) = if h < 60.0 {
        (c, x, 0.0)
    } else if h < 120.0 {
        (x, c, 0.0)
    } else if h < 180.0 {
        (0.0, c, x)
    } else if h < 240.0 {
        (0.0, x, c)
    } else if h < 300.0 {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };

    (r + m, g + m, b + m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_placeholder_decoder() {
        let decoder = PlaceholderDecoder::new(640, 480, 30.0, 100);
        let frame = decoder.get_frame(50);
        assert_eq!(frame.width, 640);
        assert_eq!(frame.height, 480);
        assert_eq!(frame.data.len(), 640 * 480 * 3);
    }
}
