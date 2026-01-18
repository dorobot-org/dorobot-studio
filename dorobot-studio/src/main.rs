//! DoRobot Studio - Dataset Viewer with Makepad

pub mod app;
pub mod data;
pub mod lerobot_app;
pub mod home;
pub mod shared;
pub mod widgets;

fn main() {
    lerobot_app::app_main();
}
