//! Custom widgets for LeRobot visualization

pub mod video_player;
pub mod time_series_plot;
pub mod timeline;
pub mod robot_viewer;
pub mod episode_list;

use makepad_widgets::{ScriptValue, ScriptVm};

pub fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
    self::video_player::script_mod(vm);
    self::time_series_plot::script_mod(vm);
    self::timeline::script_mod(vm);
    self::robot_viewer::script_mod(vm);
    self::episode_list::script_mod(vm)
}
