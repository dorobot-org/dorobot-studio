//! Shared styles and components

pub mod styles;

use makepad_widgets::{ScriptValue, ScriptVm};

pub fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
    self::styles::script_mod(vm)
}
