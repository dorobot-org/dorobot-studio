//! The theme selector: one DSL constant, chosen before the styles load.
//!
//! makepad gives a shader no global uniform, so the usual way to theme a tree
//! is to push a `light` float into every widget that draws — dozens of call
//! sites, and it silently misses any widget nobody remembered to list. This
//! takes the other route: every colour token is a `mix(dark, light, LIGHT)`
//! resolved when the DSL loads, where `LIGHT` is the one constant registered
//! below. One choice themes everything, including the widgets nobody listed.
//!
//! The trade is that the choice is made at startup rather than live. For a
//! dataset viewer that is the honest trade: a restart is cheap, and a toggle
//! that silently misses half the tree is worse than no toggle.
//!
//! Set `DOROBOT_THEME=light` to start in light mode.

use makepad_widgets::*;

mod dark {
    use makepad_widgets::*;
    script_mod! {
        mod.widgets.studio_theme = {}
        mod.widgets.studio_theme.LIGHT = 0.0
        // makepad binds `mod.theme = mod.themes.dark` itself; restated so both
        // arms of the choice are visible in one place.
        mod.theme = mod.themes.dark
    }
}

mod light {
    use makepad_widgets::*;
    script_mod! {
        mod.widgets.studio_theme = {}
        mod.widgets.studio_theme.LIGHT = 1.0
        // Stock widgets carry their own palette — a View's default background,
        // scrollbars, the text input. Without rebinding this they stay dark and
        // show through wherever this app's surfaces are transparent.
        mod.theme = mod.themes.light
    }
}

/// Which theme the environment asks for. Anything but `light` is dark,
/// including an unset variable — the industrial palette is dark-first.
pub fn wants_light() -> bool {
    std::env::var("DOROBOT_THEME")
        .map(|v| v.eq_ignore_ascii_case("light"))
        .unwrap_or(false)
}

/// Register the selected theme constant. Must run before [`crate::shared`].
pub fn script_mod(vm: &mut ScriptVm) -> ScriptValue {
    if wants_light() {
        self::light::script_mod(vm)
    } else {
        self::dark::script_mod(vm)
    }
}
