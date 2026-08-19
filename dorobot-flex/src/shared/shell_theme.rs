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
//!
//! **Both halves of every pair are set.** app-shell mixes `_L` toward `_D` on
//! its `dark_mode` uniform, and this app's own widgets already carry the same
//! dorobot-ux pairs, so with both halves filled the whole window — chrome and
//! content — moves together on one switch instead of one of them being pinned.

use makepad_widgets::*;

script_mod! {
    use mod.prelude.widgets.*

    // dorobot-ux surfaces. The ladder is void (ground) -> deep (surface) ->
    // lift (raised), with edge for hairlines; see `dorobot_ux::tokens::pal`.

    // Ground everything sits on — ux void.
    mod.widgets.shell.BG_CONTENT_L = #xF2F0ED
    mod.widgets.shell.BG_CONTENT_D = #x0A0908

    // Chrome surfaces — ux deep.
    mod.widgets.shell.BG_HEADER_L = #xFBFAF8
    mod.widgets.shell.BG_HEADER_D = #x141312
    mod.widgets.shell.BG_PANEL_L = #xFBFAF8
    mod.widgets.shell.BG_PANEL_D = #x141312
    mod.widgets.shell.BG_FOOTER_L = #xFBFAF8
    mod.widgets.shell.BG_FOOTER_D = #x141312

    // Raised surfaces — ux lift. The sidebar and a panel's title bar sit a
    // step above the surface they belong to.
    mod.widgets.shell.BG_SIDEBAR_L = #xFFFFFF
    mod.widgets.shell.BG_SIDEBAR_D = #x1B1917
    mod.widgets.shell.BG_PANEL_TITLE_L = #xFFFFFF
    mod.widgets.shell.BG_PANEL_TITLE_D = #x1B1917

    // Hairlines — ux edge.
    mod.widgets.shell.BORDER_L = #xD8D4CF
    mod.widgets.shell.BORDER_D = #x2A2725

    // Type — ux ink and dim.
    mod.widgets.shell.TEXT_L = #x161413
    mod.widgets.shell.TEXT_D = #xF2F0EC
    mod.widgets.shell.TEXT_DIM_L = #x5E564E
    mod.widgets.shell.TEXT_DIM_D = #x94877F

    // Accent — ux vio, the one warm signal colour.
    mod.widgets.shell.ACCENT_L = #xD15010
    mod.widgets.shell.ACCENT_D = #xEF6F2E
}
