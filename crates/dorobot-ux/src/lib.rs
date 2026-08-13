//! The visual language every DoRobot app draws with.
//!
//! The products have long claimed to "share a visual language and two widget
//! crates". Until this crate existed that was aspirational: the tokens and the
//! custom widgets lived inside one application, private to it, so the others
//! could only agree by copy-paste — and copies drift. This is that shared
//! crate.
//!
//! **It is deliberately lineage-neutral.** Both product lines depend on it —
//! the RL training console and the teleop/dataset tools — and they live in
//! separate repositories. Nothing specific to either belongs here: no engine
//! wiring, no dataset types, no screen that only one of them has. A token or a
//! widget earns its place here by being wanted by both; anything else belongs
//! in the application that wants it.
//!
//! Two modules, deliberately:
//!
//! * [`tokens`] — the factory-industrial palette, type scale and DSL
//!   prototypes (`mod.widgets.nx.*`), plus [`tokens::pal`], the Rust-side
//!   mirror for colours pushed through `script_apply_eval!`.
//! * [`kit`] — the custom-drawn widgets that no makepad primitive covers:
//!   sparkline, segmented bar, heatmap, scrub bar.
//!
//! The fonts live here rather than in either app, because
//! `crate_resource("self:resources/fonts/…")` resolves relative to the crate
//! that *defines* the DSL. A consumer needs no font files of its own.
//!
//! Register both in one call, before any widget that uses them mounts:
//!
//! ```ignore
//! fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
//!     makepad_widgets::script_mod(vm);
//!     dorobot_ux::script_mod(vm);
//!     // …then this app's own screens
//! }
//! ```

use makepad_widgets::*;

pub mod kit;
pub mod tokens;

/// Register the tokens and the widget kit, in dependency order: `kit`'s
/// prototypes reference `nx` type styles, so the tokens must land first.
pub fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
    crate::tokens::script_mod(vm);
    crate::kit::script_mod(vm)
}
