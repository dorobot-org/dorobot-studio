//! App chrome: design tokens, the top app bar, and the left navigation rail.
//!
//! Token values are the design system in `docs/ux/ux-design.html` — they are
//! deliberately separate from `shared::styles` (`mod.widgets.flex.*`), which
//! serves the legacy single-screen player.

use makepad_widgets::*;

use crate::api::Screen;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    use mod.text.*
    use mod.res.*

    // Namespace object for the UX design tokens.
    mod.widgets.ux = {}

    // ---------------------------------------------------------------- color --
    mod.widgets.ux.GROUND      = #x12151C
    mod.widgets.ux.SURFACE     = #x1A1F29
    mod.widgets.ux.ELEVATED    = #x222836
    mod.widgets.ux.LINE        = #x2A3140
    mod.widgets.ux.INK         = #xE6EAF2
    mod.widgets.ux.INK_2       = #x9AA5BC
    mod.widgets.ux.INK_3       = #x6C7591
    mod.widgets.ux.ACCENT      = #x4C8BF5
    mod.widgets.ux.ACCENT_SOFT = #x1B2740
    mod.widgets.ux.OK          = #x58BE8A
    mod.widgets.ux.WARN        = #xD9A24E
    mod.widgets.ux.STOP        = #xE5484D

    // ----------------------------------------------------------------- type --
    // Roboto for Latin, Noto Sans SC for Chinese. ("Open Sans SC" is not a
    // released family; Noto Sans SC is the standard open Simplified-Chinese
    // face and pairs cleanly with Roboto's humanist proportions.)
    mod.widgets.ux.FONT_REGULAR = TextStyle{
        font_family: FontFamily{
            latin := FontMember{res: crate_resource("self:resources/fonts/Roboto-Regular.ttf") asc: 0.0 desc: 0.0}
            chinese := FontMember{res: crate_resource("self:resources/fonts/NotoSansSC-Regular.ttf") asc: 0.0 desc: 0.0}
        }
        line_spacing: 1.35
    }
    mod.widgets.ux.FONT_MEDIUM = TextStyle{
        font_family: FontFamily{
            latin := FontMember{res: crate_resource("self:resources/fonts/Roboto-Medium.ttf") asc: 0.0 desc: 0.0}
            chinese := FontMember{res: crate_resource("self:resources/fonts/NotoSansSC-Regular.ttf") asc: 0.0 desc: 0.0}
        }
        line_spacing: 1.35
    }
    mod.widgets.ux.FONT_BOLD = TextStyle{
        font_family: FontFamily{
            latin := FontMember{res: crate_resource("self:resources/fonts/Roboto-Bold.ttf") asc: 0.0 desc: 0.0}
            chinese := FontMember{res: crate_resource("self:resources/fonts/NotoSansSC-Regular.ttf") asc: 0.0 desc: 0.0}
        }
        line_spacing: 1.35
    }

    // Lighter, smaller, airier than before — the previous scale read as heavy.
    mod.widgets.ux.TEXT_H1    = mod.widgets.ux.FONT_MEDIUM{font_size: 19.0}
    mod.widgets.ux.TEXT_TITLE = mod.widgets.ux.FONT_MEDIUM{font_size: 13.0}
    mod.widgets.ux.TEXT_BODY  = mod.widgets.ux.FONT_REGULAR{font_size: 11.5}
    mod.widgets.ux.TEXT_META  = mod.widgets.ux.FONT_REGULAR{font_size: 10.5}
    mod.widgets.ux.TEXT_CHIP  = mod.widgets.ux.FONT_MEDIUM{font_size: 9.5}
    mod.widgets.ux.TEXT_NAV   = mod.widgets.ux.FONT_REGULAR{font_size: 10.5}

    // -------------------------------------------------------------- surfaces --
    // Panel with a hairline border, the base for every card and rail.
    // `light` = 0 dark, 1 light. Every surface carries it so a theme switch is
    // one value pushed through the tree rather than a second widget set.
    mod.widgets.ux.Card = RoundedView{
        width: Fill
        height: Fit
        show_bg: true
        draw_bg +: {
            light: instance(0.0)
            border_size: 1.0
            border_radius: 7.0
            pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                let fill = mix(#x1A1F29, #xFFFFFF, self.light)
                let edge = mix(#x262D3B, #xE3E7EF, self.light)
                sdf.box(0.5, 0.5, self.rect_size.x - 1.0, self.rect_size.y - 1.0, 7.0)
                sdf.fill_keep(fill)
                sdf.stroke(edge, 1.0)
                return sdf.result
            }
        }
    }

    // Standard panel title bar: drag dots, title, maximize, close.
    mod.widgets.ux.PanelHead = View{
        width: Fill
        height: 34
        flow: Right
        align: Align{y: 0.5}
        padding: Inset{left: 12. right: 10.}
        spacing: 9.0
        show_bg: true
        draw_bg +: {
            light: instance(0.0)
            pixel: fn() {
                // hairline along the bottom edge only
                let base = mix(#x1A1F29, #xFFFFFF, self.light)
                let border = mix(#x262D3B, #xE7EBF2, self.light)
                let t = step(self.rect_size.y - 1.0, self.pos.y * self.rect_size.y)
                return mix(base, border, t)
            }
        }

        grip := View{
            width: 10 height: 14
            show_bg: true
            draw_bg +: {
                light: instance(0.0)
                pixel: fn() {
                    let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                    let c = mix(#x4B5468, #xB6BCCB, self.light)
                    let r = 1.0
                    sdf.circle(2.0, 3.0, r)  sdf.fill(c)
                    sdf.circle(7.0, 3.0, r)  sdf.fill(c)
                    sdf.circle(2.0, 7.0, r)  sdf.fill(c)
                    sdf.circle(7.0, 7.0, r)  sdf.fill(c)
                    sdf.circle(2.0, 11.0, r) sdf.fill(c)
                    sdf.circle(7.0, 11.0, r) sdf.fill(c)
                    return sdf.result
                }
            }
        }
        title := Label{
            text: "Panel"
            draw_text +: {
                light: instance(0.0)
                text_style: mod.widgets.ux.TEXT_TITLE{}
                get_color: fn() { return mix(#xD7DDEA, #x2A3244, self.light) }
            }
        }
        Filler{}
        btn_max := View{
            width: 13 height: 13
            show_bg: true
            draw_bg +: {
                light: instance(0.0)
                pixel: fn() {
                    let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                    sdf.box(2.0, 2.0, 9.0, 9.0, 1.5)
                    sdf.stroke(mix(#x79839A, #x99A1B3, self.light), 1.1)
                    return sdf.result
                }
            }
        }
        btn_close := View{
            width: 13 height: 13
            margin: Inset{left: 6.}
            show_bg: true
            draw_bg +: {
                light: instance(0.0)
                pixel: fn() {
                    let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                    sdf.move_to(3.0, 3.0)  sdf.line_to(10.0, 10.0)
                    sdf.stroke(mix(#x79839A, #x99A1B3, self.light), 1.1)
                    sdf.move_to(10.0, 3.0) sdf.line_to(3.0, 10.0)
                    sdf.stroke(mix(#x79839A, #x99A1B3, self.light), 1.1)
                    return sdf.result
                }
            }
        }
    }

    // ---------------------------------------------------------------- chips --
    // Status pill. `tone` selects the palette: 0 neutral, 1 ok, 2 stop, 3 accent.
    mod.widgets.ux.Chip = RoundedView{
        width: Fit
        // Intrinsic height: a fixed box clipped descenders at this type size.
        height: Fit
        padding: Inset{left: 8. right: 8. top: 4. bottom: 4.}
        align: Align{y: 0.5}
        show_bg: true
        draw_bg +: {
            tone: instance(0.0)
            light: instance(0.0)
            pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                // Shader `let` is immutable, so select branchlessly on tone.
                let f_ok = mix(#x222836, #x14301F, step(0.5, self.tone))
                let f_stop = mix(f_ok, #x33191B, step(1.5, self.tone))
                let f_accent = mix(f_stop, #x14243E, step(2.5, self.tone))
                let fill = mix(f_accent, #x332813, step(3.5, self.tone))
                let l_ok = mix(#x394153, #x2C6B48, step(0.5, self.tone))
                let l_stop = mix(l_ok, #x7A2F32, step(1.5, self.tone))
                let l_accent = mix(l_stop, #x2E538F, step(2.5, self.tone))
                let line = mix(l_accent, #x7A5A22, step(3.5, self.tone))
                // light palette: tinted washes instead of dark fills
                let lf_ok = mix(#xF1F3F7, #xE6F5EC, step(0.5, self.tone))
                let lf_stop = mix(lf_ok, #xFCEAE9, step(1.5, self.tone))
                let lf_accent = mix(lf_stop, #xE8F0FE, step(2.5, self.tone))
                let lfill = mix(lf_accent, #xFBF1DF, step(3.5, self.tone))
                let ll_ok = mix(#xDFE3EC, #xB6DEC7, step(0.5, self.tone))
                let ll_stop = mix(ll_ok, #xF0BFBD, step(1.5, self.tone))
                let ll_accent = mix(ll_stop, #xBBD3FA, step(2.5, self.tone))
                let lline = mix(ll_accent, #xE6CE9C, step(3.5, self.tone))
                sdf.box(0.5, 0.5, self.rect_size.x - 1.0, self.rect_size.y - 1.0, 4.0)
                sdf.fill_keep(mix(fill, lfill, self.light))
                sdf.stroke(mix(line, lline, self.light), 1.0)
                return sdf.result
            }
        }
        label := Label{
            text: "chip"
            draw_text +: {
                tone: instance(0.0)
                light: instance(0.0)
                text_style: mod.widgets.ux.TEXT_CHIP{}
                get_color: fn() {
                    let c_ok = mix(#x9AA5BC, #x58BE8A, step(0.5, self.tone))
                    let c_stop = mix(c_ok, #xE5484D, step(1.5, self.tone))
                    let c_accent = mix(c_stop, #x6BA1F8, step(2.5, self.tone))
                    let dark_c = mix(c_accent, #xD9A24E, step(3.5, self.tone))
                    let l_ok = mix(#x606A7E, #x1F7A4C, step(0.5, self.tone))
                    let l_stop = mix(l_ok, #xB3312F, step(1.5, self.tone))
                    let l_accent = mix(l_stop, #x2159C4, step(2.5, self.tone))
                    let light_c = mix(l_accent, #x8A6414, step(3.5, self.tone))
                    return mix(dark_c, light_c, self.light)
                }
            }
        }
    }

    // ------------------------------------------------------------- app bar --
    mod.widgets.ux.AppBar = View{
        width: Fill
        height: 58
        flow: Right
        align: Align{y: 0.5}
        padding: Inset{left: 18. right: 20.}
        spacing: 14.0
        show_bg: true
        draw_bg +: {
            light: instance(0.0)
            pixel: fn() {
                let base = mix(#x0F131B, #xFFFFFF, self.light)
                let border = mix(#x262D3B, #xE3E7EF, self.light)
                let t = step(self.rect_size.y - 1.0, self.pos.y * self.rect_size.y)
                return mix(base, border, t)
            }
        }

        hamburger := View{
            width: 20 height: 20
            show_bg: true
            draw_bg +: {
                light: instance(0.0)
                pixel: fn() {
                    let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                    let c = mix(#xB8C1D2, #x4A5266, self.light)
                    sdf.move_to(2.0, 5.0)  sdf.line_to(18.0, 5.0)  sdf.stroke(c, 1.3)
                    sdf.move_to(2.0, 10.0) sdf.line_to(18.0, 10.0) sdf.stroke(c, 1.3)
                    sdf.move_to(2.0, 15.0) sdf.line_to(18.0, 15.0) sdf.stroke(c, 1.3)
                    return sdf.result
                }
            }
        }
        logo := View{
            width: 26 height: 26
            margin: Inset{left: 8.}
            show_bg: true
            draw_bg +: {
                pixel: fn() {
                    let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                    let c = #x4C8BF5
                    // stylised arm: base, upper link, forearm, gripper dot
                    sdf.box(3.0, 19.0, 13.0, 4.0, 1.2)
                    sdf.fill(c)
                    sdf.rotate(-0.6, 8.0, 19.0)
                    sdf.box(6.0, 9.0, 4.0, 11.0, 1.2)
                    sdf.fill(c)
                    sdf.rotate(0.6, 8.0, 19.0)
                    sdf.rotate(0.5, 12.0, 10.0)
                    sdf.box(10.0, 6.0, 11.0, 3.6, 1.2)
                    sdf.fill(c)
                    sdf.rotate(-0.5, 12.0, 10.0)
                    sdf.circle(20.0, 6.0, 2.6)
                    sdf.fill(#xCED5E4)
                    return sdf.result
                }
            }
        }
        product := Label{
            text: "DoRobot Studio"
            draw_text +: {
                light: instance(0.0)
                text_style: mod.widgets.ux.FONT_MEDIUM{font_size: 14.0}
                get_color: fn() { return mix(#xE6EAF2, #x1C2231, self.light) }
            }
        }
        Filler{}
        actions := View{
            width: Fit height: Fit
            flow: Right
            spacing: 18.0
            align: Align{y: 0.5}
            bell := View{
                width: 16 height: 16
                show_bg: true
                draw_bg +: {
                    light: instance(0.0)
                    pixel: fn() {
                        let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                        sdf.circle(8.5, 7.0, 5.4)
                        sdf.stroke(mix(#x8B95AD, #x6B7488, self.light), 1.2)
                        sdf.move_to(5.5, 13.0) sdf.line_to(11.5, 13.0)
                        sdf.stroke(mix(#x8B95AD, #x6B7488, self.light), 1.2)
                        return sdf.result
                    }
                }
            }
            help := View{
                width: 16 height: 16
                show_bg: true
                draw_bg +: {
                    light: instance(0.0)
                    pixel: fn() {
                        let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                        sdf.circle(8.5, 8.5, 7.2)
                        sdf.stroke(mix(#x8B95AD, #x6B7488, self.light), 1.2)
                        sdf.circle(8.5, 12.0, 0.9)
                        sdf.fill(mix(#x8B95AD, #x6B7488, self.light))
                        return sdf.result
                    }
                }
            }
            gear := View{
                width: 16 height: 16
                cursor: MouseCursor.Hand
                capture_overload: true
                show_bg: true
                draw_bg +: {
                    light: instance(0.0)
                    pixel: fn() {
                        let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                        sdf.circle(8.5, 8.5, 6.6)
                        sdf.stroke(mix(#x8B95AD, #x6B7488, self.light), 1.2)
                        sdf.circle(8.5, 8.5, 2.4)
                        sdf.stroke(mix(#x8B95AD, #x6B7488, self.light), 1.2)
                        return sdf.result
                    }
                }
            }
            avatar := RoundedView{
                width: 28 height: 28
                align: Align{x: 0.5 y: 0.5}
                show_bg: true
                draw_bg +: {
                    color: #x2B60C6
                    border_radius: 14.0
                }
                Label{
                    text: "AK"
                    draw_text +: {
                        color: #xFFFFFF
                        text_style: mod.widgets.ux.TEXT_CHIP{}
                    }
                }
            }
        }
    }

    // ------------------------------------------------------------- nav rail --
    // One rail entry. `icon` picks the glyph, `active` drives the pill + tint.
    mod.widgets.ux.NavItem = View{
        width: Fill
        height: 58
        flow: Down
        align: Align{x: 0.5 y: 0.5}
        spacing: 6.0
        cursor: MouseCursor.Hand
        show_bg: true
        draw_bg +: {
            active: instance(0.0)
            light: instance(0.0)
            pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                let well = mix(#x1B2740, #xE8EFFC, self.light)
                let idle = mix(#x0F131B00, #xF7F8FB00, self.light)
                sdf.box(9.0, 4.0, self.rect_size.x - 18.0, self.rect_size.y - 8.0, 6.0)
                sdf.fill(mix(idle, well, self.active))
                // slim accent marker on the leading edge
                sdf.box(2.0, 12.0, 2.5, self.rect_size.y - 24.0, 1.25)
                sdf.fill(mix(idle, #x4C8BF5, self.active))
                return sdf.result
            }
        }

        glyph := View{
            width: 19 height: 19
            show_bg: true
            draw_bg +: {
                icon: instance(0.0)
                active: instance(0.0)
                light: instance(0.0)
                pixel: fn() {
                    let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                    let idle_c = mix(#x77819A, #x757E93, self.light)
                    let on_c = mix(#xA9C6FB, #x2F62C9, self.light)
                    let c = mix(idle_c, on_c, self.active)
                    if self.icon < 0.5 {
                        // library: folder
                        sdf.move_to(2.0, 6.0)
                        sdf.line_to(8.0, 6.0)
                        sdf.line_to(10.0, 8.5)
                        sdf.line_to(20.0, 8.5)
                        sdf.line_to(20.0, 18.0)
                        sdf.line_to(2.0, 18.0)
                        sdf.close_path()
                        sdf.stroke(c, 1.5)
                    } else if self.icon < 1.5 {
                        // record: ring with filled core
                        sdf.circle(11.0, 11.0, 8.0)
                        sdf.stroke(c, 1.5)
                        sdf.circle(11.0, 11.0, 3.6)
                        sdf.fill(c)
                    } else if self.icon < 2.5 {
                        // play: triangle
                        sdf.move_to(5.0, 3.5)
                        sdf.line_to(18.0, 11.0)
                        sdf.line_to(5.0, 18.5)
                        sdf.close_path()
                        sdf.stroke(c, 1.5)
                    } else if self.icon < 3.5 {
                        // hardware: chip
                        sdf.box(5.0, 5.0, 12.0, 12.0, 1.5)
                        sdf.stroke(c, 1.5)
                        sdf.box(9.0, 9.0, 4.0, 4.0, 0.8)
                        sdf.fill(c)
                        sdf.move_to(2.0, 8.0)  sdf.line_to(5.0, 8.0)  sdf.stroke(c, 1.3)
                        sdf.move_to(2.0, 14.0) sdf.line_to(5.0, 14.0) sdf.stroke(c, 1.3)
                        sdf.move_to(17.0, 8.0)  sdf.line_to(20.0, 8.0)  sdf.stroke(c, 1.3)
                        sdf.move_to(17.0, 14.0) sdf.line_to(20.0, 14.0) sdf.stroke(c, 1.3)
                    } else {
                        // eval: bar chart
                        sdf.box(3.0, 12.0, 3.6, 7.0, 0.8)  sdf.fill(c)
                        sdf.box(9.0, 7.0, 3.6, 12.0, 0.8)  sdf.fill(c)
                        sdf.box(15.0, 3.0, 3.6, 16.0, 0.8) sdf.fill(c)
                    }
                    return sdf.result
                }
            }
        }
        caption := Label{
            text: "Item"
            draw_text +: {
                active: instance(0.0)
                light: instance(0.0)
                text_style: mod.widgets.ux.TEXT_NAV{}
                get_color: fn() {
                    let idle_c = mix(#x77819A, #x757E93, self.light)
                    let on_c = mix(#xA9C6FB, #x2F62C9, self.light)
                    return mix(idle_c, on_c, self.active)
                }
            }
        }
    }

    mod.widgets.ux.NavRail = View{
        width: 88
        height: Fill
        flow: Down
        padding: Inset{top: 14. left: 2. right: 2.}
        spacing: 4.0
        show_bg: true
        draw_bg +: {
            light: instance(0.0)
            pixel: fn() {
                let base = mix(#x0F131B, #xF7F8FB, self.light)
                let border = mix(#x262D3B, #xE3E7EF, self.light)
                let t = step(self.rect_size.x - 1.0, self.pos.x * self.rect_size.x)
                return mix(base, border, t)
            }
        }

        nav_library  := mod.widgets.ux.NavItem{ caption +: { text: "Library" }  glyph +: { draw_bg +: {icon: 0.0} } }
        nav_record   := mod.widgets.ux.NavItem{ caption +: { text: "Record" }   glyph +: { draw_bg +: {icon: 1.0} } }
        nav_play     := mod.widgets.ux.NavItem{ caption +: { text: "Play" }     glyph +: { draw_bg +: {icon: 2.0} } }
        nav_hardware := mod.widgets.ux.NavItem{ caption +: { text: "Hardware" } glyph +: { draw_bg +: {icon: 3.0} } }
        nav_eval     := mod.widgets.ux.NavItem{ caption +: { text: "Eval" }     glyph +: { draw_bg +: {icon: 4.0} } }
    }
}

/// Nav rail ids in display order, paired with the screen each selects.
pub const NAV_ITEMS: [(&[LiveId], Screen); 5] = [
    (ids!(nav_library), Screen::Library),
    (ids!(nav_record), Screen::Record),
    (ids!(nav_play), Screen::Play),
    (ids!(nav_hardware), Screen::Hardware),
    (ids!(nav_eval), Screen::Eval),
];

/// Paint the active pill and label tint for the current screen.
pub fn sync_nav(cx: &mut Cx, rail: &WidgetRef, current: Screen) {
    for (path, screen) in NAV_ITEMS {
        let on = if screen == current { 1.0 } else { 0.0 };
        let mut item = rail.widget(cx, path);
        if item.is_empty() {
            continue;
        }
        script_apply_eval!(cx, item, {
            draw_bg +: { active: #(on) }
        });
        let mut glyph = rail.widget(cx, path).widget(cx, ids!(glyph));
        if !glyph.is_empty() {
            script_apply_eval!(cx, glyph, {
                draw_bg +: { active: #(on) }
            });
        }
        let mut cap = rail.widget(cx, path).widget(cx, ids!(caption));
        if !cap.is_empty() {
            script_apply_eval!(cx, cap, {
                draw_text +: { active: #(on) }
            });
        }
    }
}

// ============================================================================
// Theme
// ============================================================================

use std::cell::Cell;

thread_local! {
    /// 0.0 = dark, 1.0 = light. Read by screens when they repaint.
    static LIGHT: Cell<f64> = const { Cell::new(0.0) };
}

pub fn light_mode() -> f64 {
    LIGHT.with(|l| l.get())
}

pub fn set_light_mode(v: f64) {
    LIGHT.with(|l| l.set(v));
}

pub fn toggle_light_mode() -> f64 {
    let next = if light_mode() > 0.5 { 0.0 } else { 1.0 };
    set_light_mode(next);
    next
}

/// True when a View-like widget was released under the pointer.
///
/// `find_widget_action` returns only the *first* action for a uid, and one
/// press delivers FingerDown and FingerUp in the same batch — so matching on
/// that first action never sees the release. Scan them all instead.
pub fn view_clicked(actions: &Actions, uid: WidgetUid) -> bool {
    actions.filter_widget_actions(uid).any(|a| {
        matches!(a.cast::<ViewAction>(), ViewAction::FingerUp(fe) if fe.is_over)
    })
}

/// Which draws on a widget carry the `light` instance.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Themed {
    Bg,
    Text,
    Both,
}

/// Push the theme value into a set of widgets.
///
/// Widgets are listed explicitly rather than walked: applying a field a widget
/// does not declare logs a script-VM error, and composite widgets have no
/// `draw_bg` of their own.
pub fn apply_light(cx: &mut Cx, root: &WidgetRef, items: &[(&[LiveId], Themed)], light: f64) {
    for (path, kind) in items {
        let mut w = root.widget(cx, path);
        if w.is_empty() {
            continue;
        }
        match kind {
            Themed::Bg => script_apply_eval!(cx, w, { draw_bg +: { light: #(light) } }),
            Themed::Text => script_apply_eval!(cx, w, { draw_text +: { light: #(light) } }),
            Themed::Both => script_apply_eval!(cx, w, {
                draw_bg +: { light: #(light) }
                draw_text +: { light: #(light) }
            }),
        }
    }
}

/// Same as [`apply_light`] but rooted at a `View` (what a widget's `#[deref]`
/// field is), rather than a `WidgetRef`.
pub fn apply_light_in(cx: &mut Cx, root: &mut View, items: &[(&[LiveId], Themed)], light: f64) {
    for (path, kind) in items {
        let mut w = root.widget(cx, path);
        if w.is_empty() {
            continue;
        }
        match kind {
            Themed::Bg => script_apply_eval!(cx, w, { draw_bg +: { light: #(light) } }),
            Themed::Text => script_apply_eval!(cx, w, { draw_text +: { light: #(light) } }),
            Themed::Both => script_apply_eval!(cx, w, {
                draw_bg +: { light: #(light) }
                draw_text +: { light: #(light) }
            }),
        }
    }
}

/// App-bar and nav-rail chrome.
pub fn theme_chrome(cx: &mut Cx, ui: &WidgetRef, light: f64) {
    let mut items: Vec<(&[LiveId], Themed)> = vec![
        (ids!(app_bar), Themed::Bg),
        (ids!(app_bar.hamburger), Themed::Bg),
        (ids!(app_bar.product), Themed::Text),
        (ids!(app_bar.actions.bell), Themed::Bg),
        (ids!(app_bar.actions.help), Themed::Bg),
        (ids!(app_bar.actions.gear), Themed::Bg),
        (ids!(nav), Themed::Bg),
    ];
    for (path, _) in NAV_ITEMS {
        items.push((path, Themed::Bg));
    }
    apply_light(cx, ui, &items, light);
    for (path, _) in NAV_ITEMS {
        let item = ui.widget(cx, path);
        if item.is_empty() {
            continue;
        }
        apply_light(cx, &item, &[(ids!(glyph), Themed::Bg), (ids!(caption), Themed::Text)], light);
    }
}

/// Every panel head shares the same themed parts.
pub fn theme_panel_head(cx: &mut Cx, head: &WidgetRef, light: f64) {
    let mut h = head.clone();
    if !h.is_empty() {
        script_apply_eval!(cx, h, { draw_bg +: { light: #(light) } });
    }
    apply_light(
        cx,
        head,
        &[
            (ids!(grip), Themed::Bg),
            (ids!(title), Themed::Text),
            (ids!(btn_max), Themed::Bg),
            (ids!(btn_close), Themed::Bg),
        ],
        light,
    );
}

/// A chip carries the value on both its fill and its label.
pub fn theme_chip(cx: &mut Cx, chip: &WidgetRef, light: f64) {
    let mut c = chip.clone();
    if !c.is_empty() {
        script_apply_eval!(cx, c, { draw_bg +: { light: #(light) } });
    }
    apply_light(cx, chip, &[(ids!(label), Themed::Text)], light);
}
