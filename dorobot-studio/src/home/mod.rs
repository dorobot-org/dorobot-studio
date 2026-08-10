//! Home screen with main dashboard layout

pub mod home_screen;

use makepad_widgets::{ScriptValue, ScriptVm};

pub fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
    self::home_screen::script_mod(vm)
}
