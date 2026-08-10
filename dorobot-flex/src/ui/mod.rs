//! Makepad implementation of the UX design in `docs/ux/`.
//!
//! Screens are built against [`crate::api::Backend`] so they can be rendered
//! and visually validated (see `tools/vqa/`) with [`crate::api::mock::MockBackend`]
//! before any real hardware, writer, or policy backend exists.

use makepad_widgets::*;

pub mod frame;
pub mod hardware;
pub mod play;
pub mod record;
pub mod eval;
pub mod library;

/// Register every UI module. Tokens must come first — screens reference them.
pub fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
    frame::script_mod(vm);
    library::script_mod(vm);
    hardware::script_mod(vm);
    record::script_mod(vm);
    play::script_mod(vm);
    eval::script_mod(vm)
}
