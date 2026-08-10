//! LeRobot Viewer Styles and Theme
//!
//! Supports dark/light theme via `dark_mode: instance(0.0)` shader variable.
//! Use `get_global_dark_mode()` from makepad-app-shell to get current theme.

use makepad_widgets::*;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    use mod.text.*
    use mod.res.*

    // Namespace object for this app's shared style constants
    mod.widgets.flex = {}

    // ===========================================
    // FONTS - Manrope with Chinese and Emoji support
    // ===========================================

    mod.widgets.flex.FONT_REGULAR = TextStyle{
        font_family: FontFamily{
            latin := FontMember{res: crate_resource("self:resources/fonts/Manrope-Regular.ttf") asc: 0.0 desc: 0.0}
            chinese := FontMember{res: crate_resource("makepad-widgets:resources/LXGWWenKaiRegular.ttf") asc: 0.0 desc: 0.0}
            emoji := FontMember{res: crate_resource("makepad-widgets:resources/NotoColorEmoji.ttf") asc: 0.0 desc: 0.0}
        }
    }
    mod.widgets.flex.FONT_MEDIUM = TextStyle{
        font_family: FontFamily{
            latin := FontMember{res: crate_resource("self:resources/fonts/Manrope-Medium.ttf") asc: 0.0 desc: 0.0}
            chinese := FontMember{res: crate_resource("makepad-widgets:resources/LXGWWenKaiRegular.ttf") asc: 0.0 desc: 0.0}
            emoji := FontMember{res: crate_resource("makepad-widgets:resources/NotoColorEmoji.ttf") asc: 0.0 desc: 0.0}
        }
    }
    mod.widgets.flex.FONT_SEMIBOLD = TextStyle{
        font_family: FontFamily{
            latin := FontMember{res: crate_resource("self:resources/fonts/Manrope-SemiBold.ttf") asc: 0.0 desc: 0.0}
            chinese := FontMember{res: crate_resource("makepad-widgets:resources/LXGWWenKaiBold.ttf") asc: 0.0 desc: 0.0}
            emoji := FontMember{res: crate_resource("makepad-widgets:resources/NotoColorEmoji.ttf") asc: 0.0 desc: 0.0}
        }
    }
    mod.widgets.flex.FONT_BOLD = TextStyle{
        font_family: FontFamily{
            latin := FontMember{res: crate_resource("self:resources/fonts/Manrope-Bold.ttf") asc: 0.0 desc: 0.0}
            chinese := FontMember{res: crate_resource("makepad-widgets:resources/LXGWWenKaiBold.ttf") asc: 0.0 desc: 0.0}
            emoji := FontMember{res: crate_resource("makepad-widgets:resources/NotoColorEmoji.ttf") asc: 0.0 desc: 0.0}
        }
    }

    // ===========================================
    // TEXT STYLES (using Manrope)
    // ===========================================

    mod.widgets.flex.TEXT_TITLE = mod.widgets.flex.FONT_BOLD{
        font_size: 18.0
    }

    mod.widgets.flex.TEXT_SUBTITLE = mod.widgets.flex.FONT_SEMIBOLD{
        font_size: 14.0
    }

    mod.widgets.flex.TEXT_PANEL_TITLE = mod.widgets.flex.FONT_SEMIBOLD{
        font_size: 11.0
    }

    mod.widgets.flex.TEXT_BODY = mod.widgets.flex.FONT_REGULAR{
        font_size: 12.0
        line_spacing: 1.4
    }

    mod.widgets.flex.TEXT_SMALL = mod.widgets.flex.FONT_REGULAR{
        font_size: 10.0
    }

    mod.widgets.flex.TEXT_MONO = theme.font_code{
        font_size: 11.0
    }

    mod.widgets.flex.TEXT_BUTTON = mod.widgets.flex.FONT_SEMIBOLD{
        font_size: 12.0
    }

    // ===========================================
    // THEMED BACKGROUNDS (dark_mode responsive)
    // ===========================================

    // Sidebar background - responds to dark_mode
    mod.widgets.flex.ThemedSidebarBg = {
        dark_mode: instance(0.0)
        pixel: fn() {
            // Light: slate-50, Dark: #151518
            let light = vec4(0.973, 0.980, 0.988, 1.0)
            let dark = vec4(0.082, 0.082, 0.094, 1.0)
            return mix(light, dark, self.dark_mode)
        }
    }

    // Header background - responds to dark_mode
    mod.widgets.flex.ThemedHeaderBg = {
        dark_mode: instance(0.0)
        pixel: fn() {
            // Light: slate-100, Dark: #222228
            let light = vec4(0.945, 0.961, 0.976, 1.0)
            let dark = vec4(0.133, 0.133, 0.157, 1.0)
            return mix(light, dark, self.dark_mode)
        }
    }

    // Panel background - responds to dark_mode
    mod.widgets.flex.ThemedPanelBg = {
        dark_mode: instance(0.0)
        pixel: fn() {
            // Light: white, Dark: #1a1a1f
            let light = vec4(1.0, 1.0, 1.0, 1.0)
            let dark = vec4(0.102, 0.102, 0.122, 1.0)
            return mix(light, dark, self.dark_mode)
        }
    }

    // Divider - responds to dark_mode
    mod.widgets.flex.ThemedDividerBg = {
        dark_mode: instance(0.0)
        pixel: fn() {
            // Light: slate-200, Dark: #333340
            let light = vec4(0.886, 0.910, 0.941, 1.0)
            let dark = vec4(0.200, 0.200, 0.251, 1.0)
            return mix(light, dark, self.dark_mode)
        }
    }

    // Button background - responds to dark_mode
    // (applied to Button draw_bg; the base hover/down instances drive the states)
    mod.widgets.flex.ThemedButtonBg = {
        dark_mode: instance(0.0)
        pixel: fn() {
            // Light mode colors
            let light_normal = vec4(0.945, 0.961, 0.976, 1.0)  // slate-100
            let light_hover = vec4(0.886, 0.910, 0.941, 1.0)   // slate-200
            let light_pressed = vec4(0.800, 0.839, 0.886, 1.0) // slate-300
            // Dark mode colors
            let dark_normal = vec4(0.165, 0.165, 0.196, 1.0)   // #2a2a32
            let dark_hover = vec4(0.176, 0.176, 0.208, 1.0)    // #2d2d35
            let dark_pressed = vec4(0.200, 0.200, 0.235, 1.0)  // slightly lighter

            let normal = mix(light_normal, dark_normal, self.dark_mode)
            let hover_color = mix(light_hover, dark_hover, self.dark_mode)
            let pressed_color = mix(light_pressed, dark_pressed, self.dark_mode)

            let color = mix(normal, hover_color, self.hover)
            return mix(color, pressed_color, self.down)
        }
    }

    // Accent button (primary action)
    mod.widgets.flex.ThemedAccentButtonBg = {
        pixel: fn() {
            // Accent color stays consistent
            let normal = vec4(0.298, 0.545, 0.961, 1.0)    // #4c8bf5
            let hover_color = vec4(0.420, 0.631, 1.0, 1.0) // #6ba1ff
            let pressed_color = vec4(0.231, 0.510, 0.906, 1.0)

            let color = mix(normal, hover_color, self.hover)
            return mix(color, pressed_color, self.down)
        }
    }

    // ===========================================
    // THEMED TEXT DRAW SHADERS
    // ===========================================

    // Primary text - dark in light mode, light in dark mode
    mod.widgets.flex.ThemedTextPrimary = {
        dark_mode: instance(0.0)
        get_color: fn() {
            // Light mode: dark gray text, Dark mode: light gray text
            let light_text = vec4(0.1, 0.1, 0.12, 1.0)   // near black
            let dark_text = vec4(0.878, 0.878, 0.878, 1.0) // #e0e0e0
            return mix(light_text, dark_text, self.dark_mode)
        }
    }

    // Secondary text - medium gray in both modes
    mod.widgets.flex.ThemedTextSecondary = {
        dark_mode: instance(0.0)
        get_color: fn() {
            // Light mode: medium dark gray, Dark mode: medium light gray
            let light_text = vec4(0.35, 0.35, 0.4, 1.0)   // dark gray
            let dark_text = vec4(0.533, 0.533, 0.565, 1.0) // #888890
            return mix(light_text, dark_text, self.dark_mode)
        }
    }

    // Muted text - lighter labels/headers
    mod.widgets.flex.ThemedTextMuted = {
        dark_mode: instance(0.0)
        get_color: fn() {
            // Light mode: medium gray, Dark mode: dim gray
            let light_text = vec4(0.45, 0.45, 0.5, 1.0)   // gray
            let dark_text = vec4(0.333, 0.333, 0.376, 1.0) // #555560
            return mix(light_text, dark_text, self.dark_mode)
        }
    }

    // ===========================================
    // STATIC COLORS (for reference, non-themed)
    // ===========================================

    mod.widgets.flex.COLOR_ACCENT = #x4c8bf5
    mod.widgets.flex.COLOR_ACCENT_HOVER = #x6ba1ff
    mod.widgets.flex.COLOR_SUCCESS = #x4caf50
    mod.widgets.flex.COLOR_WARNING = #xff9800
    mod.widgets.flex.COLOR_ERROR = #xf44336
    mod.widgets.flex.COLOR_PLAYHEAD = #xff4444

    // Waveform channel colors (consistent across themes)
    mod.widgets.flex.COLOR_CHANNEL_0 = #x4c8bf5
    mod.widgets.flex.COLOR_CHANNEL_1 = #x4caf50
    mod.widgets.flex.COLOR_CHANNEL_2 = #xff9800
    mod.widgets.flex.COLOR_CHANNEL_3 = #xe91e63
    mod.widgets.flex.COLOR_CHANNEL_4 = #x9c27b0
    mod.widgets.flex.COLOR_CHANNEL_5 = #x00bcd4
    mod.widgets.flex.COLOR_CHANNEL_6 = #x8bc34a

    // ===========================================
    // THEMED WIDGET TEMPLATES
    // ===========================================

    // Themed sidebar View
    mod.widgets.flex.ThemedSidebar = View{
        width: Fill
        height: Fill
        flow: Down
        show_bg: true
        draw_bg +: {
            dark_mode: instance(0.0)
            pixel: fn() {
                // Light: slate-50, Dark: #151518
                let light = vec4(0.973, 0.980, 0.988, 1.0)
                let dark = vec4(0.082, 0.082, 0.094, 1.0)
                return mix(light, dark, self.dark_mode)
            }
        }
    }

    // Themed header View
    mod.widgets.flex.ThemedHeader = View{
        width: Fill
        height: 40
        padding: Inset{left: 16.}
        align: Align{y: 0.5}
        show_bg: true
        draw_bg +: {
            dark_mode: instance(0.0)
            pixel: fn() {
                // Light: slate-100, Dark: #222228
                let light = vec4(0.945, 0.961, 0.976, 1.0)
                let dark = vec4(0.133, 0.133, 0.157, 1.0)
                return mix(light, dark, self.dark_mode)
            }
        }
    }

    // Themed panel container
    mod.widgets.flex.ThemedPanel = View{
        show_bg: true
        draw_bg +: {
            dark_mode: instance(0.0)
            pixel: fn() {
                // Light: white, Dark: #1a1a1f
                let light = vec4(1.0, 1.0, 1.0, 1.0)
                let dark = vec4(0.102, 0.102, 0.122, 1.0)
                return mix(light, dark, self.dark_mode)
            }
        }
    }

    // Themed divider
    mod.widgets.flex.ThemedDivider = View{
        width: Fill
        height: 1
        show_bg: true
        draw_bg +: {
            dark_mode: instance(0.0)
            pixel: fn() {
                // Light: slate-200, Dark: #333340
                let light = vec4(0.886, 0.910, 0.941, 1.0)
                let dark = vec4(0.200, 0.200, 0.251, 1.0)
                return mix(light, dark, self.dark_mode)
            }
        }
    }

    // Themed label with primary text color (dark in light mode, light in dark mode)
    mod.widgets.flex.ThemedLabel = Label{
        draw_text +: {
            text_style: mod.widgets.flex.FONT_SEMIBOLD{font_size: 12.0}
            dark_mode: instance(0.0)
            get_color: fn() {
                let light_text = vec4(0.1, 0.1, 0.12, 1.0)
                let dark_text = vec4(0.878, 0.878, 0.878, 1.0)
                return mix(light_text, dark_text, self.dark_mode)
            }
        }
    }

    // Themed secondary label
    mod.widgets.flex.ThemedLabelSecondary = Label{
        draw_text +: {
            text_style: mod.widgets.flex.FONT_REGULAR{font_size: 11.0}
            dark_mode: instance(0.0)
            get_color: fn() {
                let light_text = vec4(0.35, 0.35, 0.4, 1.0)
                let dark_text = vec4(0.533, 0.533, 0.565, 1.0)
                return mix(light_text, dark_text, self.dark_mode)
            }
        }
    }

    // Themed muted label (for section headers)
    mod.widgets.flex.ThemedLabelMuted = Label{
        draw_text +: {
            text_style: mod.widgets.flex.FONT_REGULAR{font_size: 10.0}
            dark_mode: instance(0.0)
            get_color: fn() {
                let light_text = vec4(0.45, 0.45, 0.5, 1.0)
                let dark_text = vec4(0.333, 0.333, 0.376, 1.0)
                return mix(light_text, dark_text, self.dark_mode)
            }
        }
    }

    // Primary action button (accent color)
    mod.widgets.flex.ThemedPrimaryButton = Button{
        width: Fit
        height: 32
        padding: Inset{left: 16. right: 16.}
        draw_bg +: {
            pixel: fn() {
                // Accent color stays consistent
                let normal = vec4(0.298, 0.545, 0.961, 1.0)    // #4c8bf5
                let hover_color = vec4(0.420, 0.631, 1.0, 1.0) // #6ba1ff
                let pressed_color = vec4(0.231, 0.510, 0.906, 1.0)

                let color = mix(normal, hover_color, self.hover)
                return mix(color, pressed_color, self.down)
            }
        }
        draw_text +: {
            text_style: mod.widgets.flex.TEXT_BUTTON{}
            color: #xffffff
        }
    }

    // Secondary button
    mod.widgets.flex.ThemedSecondaryButton = Button{
        width: Fit
        height: 32
        padding: Inset{left: 16. right: 16.}
        draw_bg +: {
            dark_mode: instance(0.0)
            pixel: fn() {
                // Light mode colors
                let light_normal = vec4(0.945, 0.961, 0.976, 1.0)  // slate-100
                let light_hover = vec4(0.886, 0.910, 0.941, 1.0)   // slate-200
                let light_pressed = vec4(0.800, 0.839, 0.886, 1.0) // slate-300
                // Dark mode colors
                let dark_normal = vec4(0.165, 0.165, 0.196, 1.0)   // #2a2a32
                let dark_hover = vec4(0.176, 0.176, 0.208, 1.0)    // #2d2d35
                let dark_pressed = vec4(0.200, 0.200, 0.235, 1.0)  // slightly lighter

                let normal = mix(light_normal, dark_normal, self.dark_mode)
                let hover_color = mix(light_hover, dark_hover, self.dark_mode)
                let pressed_color = mix(light_pressed, dark_pressed, self.dark_mode)

                let color = mix(normal, hover_color, self.hover)
                return mix(color, pressed_color, self.down)
            }
        }
        draw_text +: {
            text_style: mod.widgets.flex.TEXT_BUTTON{}
            dark_mode: instance(0.0)
            get_color: fn() {
                let light_text = vec4(0.1, 0.1, 0.12, 1.0)
                let dark_text = vec4(0.878, 0.878, 0.878, 1.0)
                return mix(light_text, dark_text, self.dark_mode)
            }
        }
    }
}
