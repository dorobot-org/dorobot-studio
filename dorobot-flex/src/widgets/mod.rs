//! Custom widgets for LeRobot visualization

pub mod video_player;
pub mod time_series_plot;
pub mod timeline;
pub mod robot_viewer;
pub mod episode_list;

use makepad_widgets::Cx;

pub fn live_design(cx: &mut Cx) {
    self::video_player::live_design(cx);
    self::time_series_plot::live_design(cx);
    self::timeline::live_design(cx);
    self::robot_viewer::live_design(cx);
    self::episode_list::live_design(cx);
}
