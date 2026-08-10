//! DoRobot Flex - Entry Point
//!
//! A LeRobot dataset viewer built with Makepad.

pub use makepad_widgets;
pub use makepad_app_shell;

pub mod api;
pub mod shared;
pub mod widgets;
pub mod data;

pub mod app_data;
pub mod sidebar_content;
pub mod episode_info_panel;
pub mod playback_controls;
pub mod footer_stack;
pub mod app;

fn main() {
    env_logger::init();
    app::app_main();
}
