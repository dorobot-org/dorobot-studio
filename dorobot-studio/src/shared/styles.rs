//! DoRobot Studio styles — an alias layer onto the shared `nx` design system.
//!
//! Every name here is the one this app's screens already use, so the restyle is
//! one reviewable file rather than a diff across every screen. What changed is
//! what the names resolve to: the same palette, faces and series colours that
//! dorobot-nexus draws with, so the two apps read as one product.
//!
//! Colours are `mix(dark, light, LIGHT)` resolved at DSL load — see
//! `shared/theme.rs` for why that beats pushing a float per widget.

use makepad_widgets::*;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    use mod.text.*
    use mod.res.*

    // ===========================================
    // FONTS - Manrope with Chinese and Emoji support
    // ===========================================

    // Namespace object for this app's shared style constants
    mod.widgets.studio = {}

    mod.widgets.studio.FONT_REGULAR  = mod.widgets.nx.FONT_R{}
    mod.widgets.studio.FONT_MEDIUM   = mod.widgets.nx.FONT_M{}
    mod.widgets.studio.FONT_SEMIBOLD = mod.widgets.nx.FONT_B{}
    mod.widgets.studio.FONT_BOLD     = mod.widgets.nx.FONT_B{}

    // ===========================================
    // COLOR PALETTE (Dark Theme - Rerun-inspired)
    // ===========================================

    mod.widgets.studio.COLOR_BG_APP = mix(#x0A0908, #xF2F0ED, mod.widgets.studio_theme.LIGHT)
    mod.widgets.studio.COLOR_BG_SIDEBAR = mix(#x141312, #xFBFAF8, mod.widgets.studio_theme.LIGHT)
    mod.widgets.studio.COLOR_BG_PANEL = mix(#x141312, #xFBFAF8, mod.widgets.studio_theme.LIGHT)
    mod.widgets.studio.COLOR_BG_HEADER = mix(#x1B1917, #xFFFFFF, mod.widgets.studio_theme.LIGHT)
    mod.widgets.studio.COLOR_BG_INPUT = mix(#x060505, #xE8E5E1, mod.widgets.studio_theme.LIGHT)
    mod.widgets.studio.COLOR_BG_HOVER = mix(#x2A2725, #xD8D4CF, mod.widgets.studio_theme.LIGHT)

    mod.widgets.studio.COLOR_ACCENT = mix(#xEF6F2E, #xD15010, mod.widgets.studio_theme.LIGHT)
    mod.widgets.studio.COLOR_ACCENT_HOVER = mix(#xF5854A, #xE0631E, mod.widgets.studio_theme.LIGHT)
    mod.widgets.studio.COLOR_SUCCESS = mix(#x6FAB78, #x3E7A4A, mod.widgets.studio_theme.LIGHT)
    mod.widgets.studio.COLOR_WARNING = mix(#xF0A330, #xB07514, mod.widgets.studio_theme.LIGHT)
    mod.widgets.studio.COLOR_ERROR = mix(#xE54048, #xC43B36, mod.widgets.studio_theme.LIGHT)

    mod.widgets.studio.COLOR_TEXT_PRIMARY = mix(#xF2F0EC, #x161413, mod.widgets.studio_theme.LIGHT)
    mod.widgets.studio.COLOR_TEXT_SECONDARY = mix(#x948781, #x5E564E, mod.widgets.studio_theme.LIGHT)
    mod.widgets.studio.COLOR_TEXT_MUTED = mix(#x6B625B, #x7A7169, mod.widgets.studio_theme.LIGHT)

    mod.widgets.studio.COLOR_BORDER = mix(#x2A2725, #xD8D4CF, mod.widgets.studio_theme.LIGHT)
    mod.widgets.studio.COLOR_BORDER_LIGHT = mix(#x3A3633, #xC4BFB9, mod.widgets.studio_theme.LIGHT)
    mod.widgets.studio.COLOR_PLAYHEAD = mix(#xEF6F2E, #xD15010, mod.widgets.studio_theme.LIGHT)
    mod.widgets.studio.COLOR_TIMELINE_TRACK = mix(#x060505, #xE8E5E1, mod.widgets.studio_theme.LIGHT)
    mod.widgets.studio.COLOR_TIMELINE_TICK = mix(#x2A2725, #xD8D4CF, mod.widgets.studio_theme.LIGHT)
    mod.widgets.studio.COLOR_CHANNEL_0 = mix(#x01A2C5, #x0098B9, mod.widgets.studio_theme.LIGHT)
    mod.widgets.studio.COLOR_CHANNEL_1 = mix(#xAB3D09, #x9A3404, mod.widgets.studio_theme.LIGHT)
    mod.widgets.studio.COLOR_CHANNEL_2 = mix(#xDF6791, #xE15E8E, mod.widgets.studio_theme.LIGHT)
    mod.widgets.studio.COLOR_CHANNEL_3 = mix(#xB5900A, #xB28D00, mod.widgets.studio_theme.LIGHT)
    mod.widgets.studio.COLOR_CHANNEL_4 = mix(#x754CB0, #x60309B, mod.widgets.studio_theme.LIGHT)
    mod.widgets.studio.COLOR_CHANNEL_5 = mix(#x01A2C5, #x0098B9, mod.widgets.studio_theme.LIGHT)
    mod.widgets.studio.COLOR_CHANNEL_6 = mix(#xAB3D09, #x9A3404, mod.widgets.studio_theme.LIGHT)
    // TEXT STYLES (using Manrope)
    // ===========================================

    mod.widgets.studio.TEXT_TITLE = mod.widgets.studio.FONT_BOLD{
        font_size: 18.0
    }

    mod.widgets.studio.TEXT_SUBTITLE = mod.widgets.studio.FONT_SEMIBOLD{
        font_size: 14.0
    }

    mod.widgets.studio.TEXT_PANEL_TITLE = mod.widgets.studio.FONT_SEMIBOLD{
        font_size: 11.0
    }

    mod.widgets.studio.TEXT_BODY = mod.widgets.studio.FONT_REGULAR{
        font_size: 12.0
        line_spacing: 1.4
    }

    mod.widgets.studio.TEXT_SMALL = mod.widgets.studio.FONT_REGULAR{
        font_size: 10.0
    }

    mod.widgets.studio.TEXT_MONO = mod.widgets.nx.FONT_MONO{
        font_size: 11.0
    }

    mod.widgets.studio.TEXT_BUTTON = mod.widgets.studio.FONT_SEMIBOLD{
        font_size: 12.0
    }

    // ===========================================
    // COMMON WIDGET STYLES
    // ===========================================

    // Panel container with rounded corners
    mod.widgets.studio.Panel = RoundedView{
        show_bg: true
        draw_bg +: {
            color: mod.widgets.studio.COLOR_BG_PANEL
            border_radius: 0.0
        }
    }

    // Panel header bar
    mod.widgets.studio.PanelHeader = View{
        width: Fill
        height: 36
        padding: Inset{left: 12. right: 12.}
        align: Align{y: 0.5}

        show_bg: true
        draw_bg.color: mod.widgets.studio.COLOR_BG_HEADER
    }

    // Icon button (for toolbar)
    mod.widgets.studio.IconButton = Button{
        width: 32
        height: 32
        padding: 0
        margin: 0
        align: Align{x: 0.5 y: 0.5}

        draw_bg +: {
            color: #x00000000
            color_hover: mod.widgets.studio.COLOR_BG_HOVER
            border_radius: 0.0
        }

        draw_text +: {
            text_style: mod.widgets.studio.TEXT_BODY{}
            color: mod.widgets.studio.COLOR_TEXT_SECONDARY
            color_hover: mod.widgets.studio.COLOR_TEXT_PRIMARY
        }
    }

    // Primary action button
    mod.widgets.studio.PrimaryButton = Button{
        width: Fit
        height: 32
        padding: Inset{left: 16. right: 16.}

        draw_bg +: {
            color: mod.widgets.studio.COLOR_ACCENT
            color_hover: mod.widgets.studio.COLOR_ACCENT_HOVER
            border_radius: 0.0
            // Setting `color` alone is not enough: makepad's stock Button draws
            // a bevelled outset over it, which is why this button rendered grey
            // while its accent token was correct all along. Flattening `pixel`
            // makes the accent the button.
            pixel: fn() {
                let base = mix(self.color, self.color_hover, self.hover)
                return mix(base, self.color_hover, self.down)
            }
        }

        draw_text +: {
            text_style: mod.widgets.studio.TEXT_BUTTON{}
            color: #xffffff
        }
    }

    // Secondary button
    mod.widgets.studio.SecondaryButton = Button{
        width: Fit
        height: 32
        padding: Inset{left: 16. right: 16.}

        draw_bg +: {
            color: mod.widgets.studio.COLOR_BG_INPUT
            color_hover: mod.widgets.studio.COLOR_BG_HOVER
            border_radius: 0.0
        }

        draw_text +: {
            text_style: mod.widgets.studio.TEXT_BUTTON{}
            color: mod.widgets.studio.COLOR_TEXT_PRIMARY
        }
    }

    // Sidebar item (selectable list item)
    mod.widgets.studio.SidebarItem = View{
        width: Fill
        height: 40
        padding: Inset{left: 12. right: 12.}
        align: Align{y: 0.5}
        cursor: MouseCursor.Hand

        show_bg: true
        draw_bg.color: #x00000000
    }

    // Divider line
    mod.widgets.studio.Divider = View{
        width: Fill
        height: 1
        show_bg: true
        draw_bg.color: mod.widgets.studio.COLOR_BORDER
    }

    // Vertical divider
    mod.widgets.studio.VDivider = View{
        width: 1
        height: Fill
        show_bg: true
        draw_bg.color: mod.widgets.studio.COLOR_BORDER
    }
}
