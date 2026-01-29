//! Application state passed through Scope to panel content widgets
//!
//! This struct contains all shared state that panels need to access.
//! It is passed via Makepad's Scope mechanism using `Scope::with_data(&mut self.data)`.

use std::collections::HashMap;
use std::path::PathBuf;
use crate::data::{LeRobotDataset, EpisodeFrame};
use crate::widgets::episode_list::EpisodeInfo;

// ============================================================================
// Panel Content Registry
// ============================================================================

/// Type-safe panel identifier
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PanelSlot {
    /// Main video panel (initially panel_0, slot s1_1)
    VideoMain,
    /// 3D robot view panel (initially panel_1, slot s1_2)
    RobotView,
    /// Secondary video panel 1 (initially panel_2, slot s2_1)
    VideoCam1,
    /// Secondary video panel 2 (initially panel_3, slot s2_2)
    VideoCam2,
}

impl PanelSlot {
    /// Get the default panel ID string for this slot
    pub fn default_panel_id(&self) -> &'static str {
        match self {
            Self::VideoMain => "panel_0",
            Self::RobotView => "panel_1",
            Self::VideoCam1 => "panel_2",
            Self::VideoCam2 => "panel_3",
        }
    }

    /// Get all panel slots
    pub fn all() -> &'static [PanelSlot] {
        &[Self::VideoMain, Self::RobotView, Self::VideoCam1, Self::VideoCam2]
    }

    /// Get video panel slots only (excludes RobotView)
    pub fn video_slots() -> &'static [PanelSlot] {
        &[Self::VideoMain, Self::VideoCam1, Self::VideoCam2]
    }

    /// Parse panel_id string to PanelSlot
    pub fn from_panel_id(panel_id: &str) -> Option<Self> {
        match panel_id {
            "panel_0" => Some(Self::VideoMain),
            "panel_1" => Some(Self::RobotView),
            "panel_2" => Some(Self::VideoCam1),
            "panel_3" => Some(Self::VideoCam2),
            _ => None,
        }
    }
}

/// Content assigned to a panel
#[derive(Clone, Debug)]
pub enum PanelContent {
    /// Video content with the video key (e.g., "observation.images.cam_high")
    Video { key: String, display_name: String },
    /// 3D robot viewer
    RobotView { display_name: String },
    /// Empty/placeholder content
    Empty,
}

impl PanelContent {
    /// Get the display name for this content
    pub fn display_name(&self) -> &str {
        match self {
            Self::Video { display_name, .. } => display_name,
            Self::RobotView { display_name } => display_name,
            Self::Empty => "Empty",
        }
    }

    /// Check if this is video content
    pub fn is_video(&self) -> bool {
        matches!(self, Self::Video { .. })
    }

    /// Get video key if this is video content
    pub fn video_key(&self) -> Option<&str> {
        match self {
            Self::Video { key, .. } => Some(key),
            _ => None,
        }
    }
}

/// Registry mapping panel slots to their content
///
/// This registry tracks what content is assigned to each panel slot.
/// When drag-and-drop moves panels, the slot-to-content mapping remains
/// stable - only the visual position changes via LayoutState.
#[derive(Clone, Debug, Default)]
pub struct PanelRegistry {
    /// Maps panel slot to its content
    contents: HashMap<PanelSlot, PanelContent>,
    /// Maps panel_id string to panel slot (for reverse lookup)
    panel_id_to_slot: HashMap<String, PanelSlot>,
}

impl PanelRegistry {
    pub fn new() -> Self {
        let mut registry = Self::default();
        // Initialize with default mappings
        for slot in PanelSlot::all() {
            registry.panel_id_to_slot.insert(slot.default_panel_id().to_string(), *slot);
        }
        registry
    }

    /// Set content for a panel slot
    pub fn set_content(&mut self, slot: PanelSlot, content: PanelContent) {
        self.contents.insert(slot, content);
    }

    /// Get content for a panel slot
    pub fn get_content(&self, slot: PanelSlot) -> Option<&PanelContent> {
        self.contents.get(&slot)
    }

    /// Get panel slot from panel_id string
    pub fn slot_from_panel_id(&self, panel_id: &str) -> Option<PanelSlot> {
        self.panel_id_to_slot.get(panel_id).copied()
    }

    /// Get video key for a panel slot
    pub fn get_video_key(&self, slot: PanelSlot) -> Option<&str> {
        self.contents.get(&slot).and_then(|c| c.video_key())
    }

    /// Clear all content (when loading new dataset)
    pub fn clear(&mut self) {
        self.contents.clear();
    }

    /// Check if a panel slot has video content
    pub fn has_video(&self, slot: PanelSlot) -> bool {
        self.contents.get(&slot).map(|c| c.is_video()).unwrap_or(false)
    }

    /// Get display name for a panel slot
    pub fn get_display_name(&self, slot: PanelSlot) -> &str {
        self.contents.get(&slot)
            .map(|c| c.display_name())
            .unwrap_or("Empty")
    }
}

/// Shared application state accessible by all panels via Scope
pub struct AppData {
    // Dataset state
    pub dataset: Option<LeRobotDataset>,
    pub dataset_name: String,
    pub dataset_info: String,
    pub robot_type: String,
    pub robot_display_name: Option<String>,
    pub episodes: Vec<EpisodeInfo>,

    /// Error message to display in sidebar (cleared on successful load)
    pub error_message: Option<String>,

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

    /// Panel content registry - tracks what content is in each panel
    pub panel_registry: PanelRegistry,

    /// Slot content mapping - which panel_id is at each physical slot
    /// Index 0-3 corresponds to physical slots (s1_1, s1_2, s2_1, s2_2)
    /// Value is the panel_id ("panel_0", "panel_1", etc.)
    pub slot_to_panel: [String; 4],
}

impl Default for AppData {
    fn default() -> Self {
        Self {
            dataset: None,
            dataset_name: String::new(),
            dataset_info: String::new(),
            robot_type: String::new(),
            robot_display_name: None,
            episodes: Vec::new(),
            error_message: None,
            current_episode: None,
            episode_frames: Vec::new(),
            video_paths: HashMap::new(),
            video_frame_offset: 0,
            current_time: 0.0,
            episode_duration: 0.0,
            episode_fps: 30.0,
            is_playing: false,
            playback_speed: 1.0,
            panel_registry: PanelRegistry::new(),
            slot_to_panel: [
                "panel_0".to_string(),
                "panel_1".to_string(),
                "panel_2".to_string(),
                "panel_3".to_string(),
            ],
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
