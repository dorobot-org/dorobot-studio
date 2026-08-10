//! DoRobot Flex - Library
//!
//! A LeRobot dataset viewer built with Makepad.
//!
//! This library provides:
//! - Flex layout with resizable sidebars and footer
//! - Dataset loading for LeRobot v2.0/v3.0 formats
//! - Video playback (with optional FFmpeg support)
//! - Time series visualization
//! - 3D robot viewer

pub use makepad_widgets;
pub use makepad_app_shell;

pub mod api;
pub mod ui;
pub mod shared;
pub mod widgets;
pub mod data;

pub mod app_data;
pub mod sidebar_content;
pub mod episode_info_panel;
pub mod playback_controls;
pub mod footer_stack;
pub mod app;
