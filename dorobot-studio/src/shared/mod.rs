//! Shared styles and components

pub mod shell_theme;
pub mod styles;
pub mod theme;

use makepad_widgets::{ScriptValue, ScriptVm};

pub fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
    // The theme constant first: every colour token below reads it.
    self::theme::script_mod(vm);
    self::styles::script_mod(vm)
}
