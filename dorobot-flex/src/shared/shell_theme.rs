//! The nexus palette, handed to `makepad-app-shell` through its theme hook.
//!
//! app-shell draws its chrome — header, sidebar, panel bodies and title bars,
//! footer — from `mod.widgets.shell.*` tokens bound as uniform defaults. This
//! module replaces those tokens with dorobot-ux's surfaces, so the shell and
//! the screens inside it read as one product.
//!
//! It must be registered inside `script_mod_with_theme`'s hook: after
//! app-shell's defaults, before its widgets bind them. Registering it anywhere
//! else compiles and silently does nothing.

use makepad_widgets::*;

script_mod! {
    use mod.prelude.widgets.*

    // Surfaces, dark ladder: void -> deep -> lift.
    mod.widgets.shell.BG_CONTENT_L = #x0A0908
    mod.widgets.shell.BG_HEADER_L = #x141312
    mod.widgets.shell.BG_PANEL_L = #x141312
    mod.widgets.shell.BG_SIDEBAR_L = #x1B1917
    mod.widgets.shell.BG_PANEL_TITLE_L = #x1B1917
    mod.widgets.shell.BG_FOOTER_L = #x141312

    mod.widgets.shell.BORDER_L = #x2A2725

    mod.widgets.shell.TEXT_L = #xF2F0EC
    mod.widgets.shell.TEXT_DIM_L = #x94877F
    mod.widgets.shell.ACCENT_L = #xEF6F2E
}
