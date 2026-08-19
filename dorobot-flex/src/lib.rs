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

/// Still here because `ui`'s Play screen draws with it; the rest of the old
/// viewer's app modules went with its binary.
pub mod playback_controls;
