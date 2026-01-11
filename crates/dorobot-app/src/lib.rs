//! Dorobot Application
//!
//! Two modes available:
//! 1. Original Dorobot dashboard (default) - robotics visualization with Rerun
//! 2. LeRobot Dataset Viewer - visualize LeRobot datasets
//!
//! LeRobot viewer features:
//! - Multi-camera video playback
//! - Time-series waveform plots (observation.state, action)
//! - 3D robot arm visualization
//! - Synchronized timeline scrubbing
//! - Adaptive layout for desktop/tablet

pub mod app;
pub mod data;
pub mod lerobot_app;
pub mod home;
pub mod shared;
pub mod widgets;

// Re-export both app entry points
pub use app::app_main;

// LeRobot app can be accessed via lerobot_app module
