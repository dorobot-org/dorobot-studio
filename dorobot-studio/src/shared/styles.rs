//! LeRobot Viewer Styles and Theme

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

    mod.widgets.studio.FONT_REGULAR = TextStyle{
        font_family: FontFamily{
            latin := FontMember{res: crate_resource("self:resources/fonts/Manrope-Regular.ttf") asc: 0.0 desc: 0.0}
            chinese := FontMember{res: crate_resource("makepad-widgets:resources/LXGWWenKaiRegular.ttf") asc: 0.0 desc: 0.0}
            emoji := FontMember{res: crate_resource("makepad-widgets:resources/NotoColorEmoji.ttf") asc: 0.0 desc: 0.0}
        }
    }
    mod.widgets.studio.FONT_MEDIUM = TextStyle{
        font_family: FontFamily{
            latin := FontMember{res: crate_resource("self:resources/fonts/Manrope-Medium.ttf") asc: 0.0 desc: 0.0}
            chinese := FontMember{res: crate_resource("makepad-widgets:resources/LXGWWenKaiRegular.ttf") asc: 0.0 desc: 0.0}
            emoji := FontMember{res: crate_resource("makepad-widgets:resources/NotoColorEmoji.ttf") asc: 0.0 desc: 0.0}
        }
    }
    mod.widgets.studio.FONT_SEMIBOLD = TextStyle{
        font_family: FontFamily{
            latin := FontMember{res: crate_resource("self:resources/fonts/Manrope-SemiBold.ttf") asc: 0.0 desc: 0.0}
            chinese := FontMember{res: crate_resource("makepad-widgets:resources/LXGWWenKaiBold.ttf") asc: 0.0 desc: 0.0}
            emoji := FontMember{res: crate_resource("makepad-widgets:resources/NotoColorEmoji.ttf") asc: 0.0 desc: 0.0}
        }
    }
    mod.widgets.studio.FONT_BOLD = TextStyle{
        font_family: FontFamily{
            latin := FontMember{res: crate_resource("self:resources/fonts/Manrope-Bold.ttf") asc: 0.0 desc: 0.0}
            chinese := FontMember{res: crate_resource("makepad-widgets:resources/LXGWWenKaiBold.ttf") asc: 0.0 desc: 0.0}
            emoji := FontMember{res: crate_resource("makepad-widgets:resources/NotoColorEmoji.ttf") asc: 0.0 desc: 0.0}
        }
    }

    // ===========================================
    // COLOR PALETTE (Dark Theme - Rerun-inspired)
    // ===========================================

    mod.widgets.studio.COLOR_BG_APP = #x0d0d0f
    mod.widgets.studio.COLOR_BG_SIDEBAR = #x151518
    mod.widgets.studio.COLOR_BG_PANEL = #x1a1a1f
    mod.widgets.studio.COLOR_BG_HEADER = #x222228
    mod.widgets.studio.COLOR_BG_INPUT = #x2a2a32
    mod.widgets.studio.COLOR_BG_HOVER = #x2d2d35

    mod.widgets.studio.COLOR_ACCENT = #x4c8bf5
    mod.widgets.studio.COLOR_ACCENT_HOVER = #x6ba1ff
    mod.widgets.studio.COLOR_SUCCESS = #x4caf50
    mod.widgets.studio.COLOR_WARNING = #xff9800
    mod.widgets.studio.COLOR_ERROR = #xf44336

    mod.widgets.studio.COLOR_TEXT_PRIMARY = #xe0e0e0
    mod.widgets.studio.COLOR_TEXT_SECONDARY = #x888890
    mod.widgets.studio.COLOR_TEXT_MUTED = #x555560

    mod.widgets.studio.COLOR_BORDER = #x333340
    mod.widgets.studio.COLOR_BORDER_LIGHT = #x444450

    // Timeline colors
    mod.widgets.studio.COLOR_PLAYHEAD = #xff4444
    mod.widgets.studio.COLOR_TIMELINE_TRACK = #x2a2a32
    mod.widgets.studio.COLOR_TIMELINE_TICK = #x444450

    // Waveform channel colors
    mod.widgets.studio.COLOR_CHANNEL_0 = #x4c8bf5
    mod.widgets.studio.COLOR_CHANNEL_1 = #x4caf50
    mod.widgets.studio.COLOR_CHANNEL_2 = #xff9800
    mod.widgets.studio.COLOR_CHANNEL_3 = #xe91e63
    mod.widgets.studio.COLOR_CHANNEL_4 = #x9c27b0
    mod.widgets.studio.COLOR_CHANNEL_5 = #x00bcd4
    mod.widgets.studio.COLOR_CHANNEL_6 = #x8bc34a

    // ===========================================
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

    mod.widgets.studio.TEXT_MONO = theme.font_code{
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
            border_radius: 8.0
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
            border_radius: 4.0
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
            border_radius: 4.0
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
            border_radius: 4.0
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
