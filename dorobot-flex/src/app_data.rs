//! Application state passed through Scope to panel content widgets
//!
//! This struct contains all shared state that panels need to access.
//! It is passed via Makepad's Scope mechanism using `Scope::with_data(&mut self.data)`.

use std::collections::HashMap;
use std::path::PathBuf;
use crate::data::{LeRobotDataset, EpisodeFrame};
use crate::widgets::episode_list::EpisodeInfo;

/// Shared application state accessible by all panels via Scope
pub struct AppData {
    // Dataset state
    pub dataset: Option<LeRobotDataset>,
    pub dataset_name: String,
    pub dataset_info: String,
    pub episodes: Vec<EpisodeInfo>,

    // Current episode state
    pub current_episode: Option<u64>,
    pub episode_frames: Vec<EpisodeFrame>,
    pub video_paths: HashMap<String, PathBuf>,
    pub video_frame_offset: u64,

    // Playback state
    pub current_time: f64,
    pub episode_duration: f64,
    pub episode_fps: f64,
    pub is_playing: bool,
    pub playback_speed: f64,
}

impl Default for AppData {
    fn default() -> Self {
        Self {
            dataset: None,
            dataset_name: String::new(),
            dataset_info: String::new(),
            episodes: Vec::new(),
            current_episode: None,
            episode_frames: Vec::new(),
            video_paths: HashMap::new(),
            video_frame_offset: 0,
            current_time: 0.0,
            episode_duration: 0.0,
            episode_fps: 30.0,
            is_playing: false,
            playback_speed: 1.0,
        }
    }
}

impl AppData {
    /// Get the current frame index based on current_time
    pub fn current_frame_index(&self) -> u64 {
        (self.current_time * self.episode_fps) as u64
    }

    /// Get total frame count for current episode
    pub fn total_frames(&self) -> u64 {
        (self.episode_duration * self.episode_fps) as u64
    }

    /// Get current episode info if available
    pub fn current_episode_info(&self) -> Option<&EpisodeInfo> {
        self.current_episode
            .and_then(|idx| self.episodes.get(idx as usize))
    }

    /// Get current frame data if available
    pub fn current_frame(&self) -> Option<&EpisodeFrame> {
        if self.episode_frames.is_empty() {
            return None;
        }

        let frame_idx = self.episode_frames
            .iter()
            .position(|f| f.timestamp >= self.current_time)
            .unwrap_or(self.episode_frames.len() - 1);

        self.episode_frames.get(frame_idx)
    }

    /// Format time as M:SS.CC
    pub fn format_time(seconds: f64) -> String {
        let mins = (seconds / 60.0) as u32;
        let secs = seconds % 60.0;
        format!("{}:{:05.2}", mins, secs)
    }
}
