//! LeRobot Viewer Styles and Theme

use makepad_widgets::*;

live_design! {
    use link::theme::*;
    use link::shaders::*;
    use link::widgets::*;

    // ===========================================
    // FONTS - Manrope with Chinese and Emoji support
    // ===========================================

    pub FONT_REGULAR = {
        font_family: {
            latin = font("crate://self/resources/fonts/Manrope-Regular.ttf", 0.0, 0.0),
            chinese = font("crate://makepad-widgets/resources/LXGWWenKaiRegular.ttf", 0.0, 0.0),
            emoji = font("crate://makepad-widgets/resources/NotoColorEmoji.ttf", 0.0, 0.0),
        }
    }
    pub FONT_MEDIUM = {
        font_family: {
            latin = font("crate://self/resources/fonts/Manrope-Medium.ttf", 0.0, 0.0),
            chinese = font("crate://makepad-widgets/resources/LXGWWenKaiRegular.ttf", 0.0, 0.0),
            emoji = font("crate://makepad-widgets/resources/NotoColorEmoji.ttf", 0.0, 0.0),
        }
    }
    pub FONT_SEMIBOLD = {
        font_family: {
            latin = font("crate://self/resources/fonts/Manrope-SemiBold.ttf", 0.0, 0.0),
            chinese = font("crate://makepad-widgets/resources/LXGWWenKaiBold.ttf", 0.0, 0.0),
            emoji = font("crate://makepad-widgets/resources/NotoColorEmoji.ttf", 0.0, 0.0),
        }
    }
    pub FONT_BOLD = {
        font_family: {
            latin = font("crate://self/resources/fonts/Manrope-Bold.ttf", 0.0, 0.0),
            chinese = font("crate://makepad-widgets/resources/LXGWWenKaiBold.ttf", 0.0, 0.0),
            emoji = font("crate://makepad-widgets/resources/NotoColorEmoji.ttf", 0.0, 0.0),
        }
    }

    // ===========================================
    // COLOR PALETTE (Dark Theme - Rerun-inspired)
    // ===========================================

    pub COLOR_BG_APP = #0d0d0f
    pub COLOR_BG_SIDEBAR = #151518
    pub COLOR_BG_PANEL = #1a1a1f
    pub COLOR_BG_HEADER = #222228
    pub COLOR_BG_INPUT = #2a2a32
    pub COLOR_BG_BUTTON = #2a2a32
    pub COLOR_BG_HOVER = #2d2d35

    pub COLOR_ACCENT = #4c8bf5
    pub COLOR_ACCENT_HOVER = #6ba1ff
    pub COLOR_SUCCESS = #4caf50
    pub COLOR_WARNING = #ff9800
    pub COLOR_ERROR = #f44336

    pub COLOR_TEXT_PRIMARY = #e0e0e0
    pub COLOR_TEXT_SECONDARY = #888890
    pub COLOR_TEXT_MUTED = #555560

    pub COLOR_BORDER = #333340
    pub COLOR_BORDER_LIGHT = #444450
    pub COLOR_DIVIDER = #333340

    // Timeline colors
    pub COLOR_PLAYHEAD = #ff4444
    pub COLOR_TIMELINE_TRACK = #2a2a32
    pub COLOR_TIMELINE_TICK = #444450

    // Waveform channel colors
    pub COLOR_CHANNEL_0 = #4c8bf5
    pub COLOR_CHANNEL_1 = #4caf50
    pub COLOR_CHANNEL_2 = #ff9800
    pub COLOR_CHANNEL_3 = #e91e63
    pub COLOR_CHANNEL_4 = #9c27b0
    pub COLOR_CHANNEL_5 = #00bcd4
    pub COLOR_CHANNEL_6 = #8bc34a

    // ===========================================
    // TEXT STYLES (using Manrope)
    // ===========================================

    pub TEXT_TITLE = <FONT_BOLD> {
        font_size: 18.0
    }

    pub TEXT_SUBTITLE = <FONT_SEMIBOLD> {
        font_size: 14.0
    }

    pub TEXT_PANEL_TITLE = <FONT_SEMIBOLD> {
        font_size: 11.0
    }

    pub TEXT_BODY = <FONT_REGULAR> {
        font_size: 12.0
        line_spacing: 1.4
    }

    pub TEXT_SMALL = <FONT_REGULAR> {
        font_size: 10.0
    }

    pub TEXT_MONO = <THEME_FONT_CODE> {
        font_size: 11.0
    }

    pub TEXT_BUTTON = <FONT_SEMIBOLD> {
        font_size: 12.0
    }

    // ===========================================
    // COMMON WIDGET STYLES
    // ===========================================

    // Panel container with rounded corners
    pub Panel = <RoundedView> {
        show_bg: true
        draw_bg: {
            color: (COLOR_BG_PANEL)
            border_radius: 8.0
        }
    }

    // Panel header bar
    pub PanelHeader = <View> {
        width: Fill
        height: 36
        padding: { left: 12, right: 12 }
        align: { y: 0.5 }

        show_bg: true
        draw_bg: {
            color: (COLOR_BG_HEADER)
        }
    }

    // Icon button (for toolbar)
    pub IconButton = <Button> {
        width: 32
        height: 32
        padding: 0
        margin: 0
        align: { x: 0.5, y: 0.5 }

        draw_bg: {
            color: #0000
            color_hover: (COLOR_BG_HOVER)
            border_radius: 4.0
        }

        draw_text: {
            text_style: <TEXT_BODY> {}
            color: (COLOR_TEXT_SECONDARY)
            color_hover: (COLOR_TEXT_PRIMARY)
        }
    }

    // Primary action button
    pub PrimaryButton = <Button> {
        width: Fit
        height: 32
        padding: { left: 16, right: 16 }

        draw_bg: {
            color: (COLOR_ACCENT)
            color_hover: (COLOR_ACCENT_HOVER)
            border_radius: 4.0
        }

        draw_text: {
            text_style: <TEXT_BUTTON> {}
            color: #ffffff
        }
    }

    // Secondary button
    pub SecondaryButton = <Button> {
        width: Fit
        height: 32
        padding: { left: 16, right: 16 }

        draw_bg: {
            color: (COLOR_BG_INPUT)
            color_hover: (COLOR_BG_HOVER)
            border_radius: 4.0
        }

        draw_text: {
            text_style: <TEXT_BUTTON> {}
            color: (COLOR_TEXT_PRIMARY)
        }
    }

    // Sidebar item (selectable list item)
    pub SidebarItem = <View> {
        width: Fill
        height: 40
        padding: { left: 12, right: 12 }
        align: { y: 0.5 }
        cursor: Hand

        show_bg: true
        draw_bg: {
            color: #0000
        }
    }

    // Divider line
    pub Divider = <View> {
        width: Fill
        height: 1
        show_bg: true
        draw_bg: { color: (COLOR_BORDER) }
    }

    // Vertical divider
    pub VDivider = <View> {
        width: 1
        height: Fill
        show_bg: true
        draw_bg: { color: (COLOR_BORDER) }
    }
}
