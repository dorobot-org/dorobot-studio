//! Library screen — datasets, connected hardware, recent sessions.
//!
//! Mirrors `docs/ux/ux-01-library.png`. Content is pushed in from a
//! [`LibraryState`] snapshot by [`LibraryScreenRef::sync`].

use makepad_widgets::*;

use crate::api::{DeviceKind, LibraryState, SessionOutcome, SyncState};
use crate::ui::frame::{apply_light, apply_light_in, theme_chip, theme_panel_head, Themed};

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    use mod.widgets.ux.*

    // A dataset card: preview, name, fact line, quality + sync chips.
    let DatasetCard = mod.widgets.ux.Card{
        width: Fill
        height: Fit
        flow: Down
        cursor: MouseCursor.Hand

        head := mod.widgets.ux.PanelHead{ title +: { text: "dataset" } }

        thumb := View{
            width: Fill
            height: 104
            margin: Inset{left: 9. right: 9. top: 9.}
            show_bg: true
            draw_bg +: {
                light: instance(0.0)
                pixel: fn() {
                    let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                    // placeholder preview: soft vertical gradient + camera glyph
                    let top = mix(#x252C3A, #xEDF0F6, self.light)
                    let bot = mix(#x1B222E, #xE2E7F0, self.light)
                    let g = mix(top, bot, self.pos.y)
                    sdf.box(0.0, 0.0, self.rect_size.x, self.rect_size.y, 4.0)
                    sdf.fill(g)
                    let cx0 = self.rect_size.x * 0.5
                    let cy0 = self.rect_size.y * 0.5
                    sdf.box(cx0 - 15.0, cy0 - 10.0, 30.0, 20.0, 3.0)
                    sdf.stroke(mix(#x49536B, #xC2C9D6, self.light), 1.2)
                    sdf.circle(cx0, cy0, 5.5)
                    sdf.stroke(mix(#x49536B, #xC2C9D6, self.light), 1.2)
                    return sdf.result
                }
            }
        }

        body := View{
            width: Fill
            height: Fit
            flow: Down
            padding: Inset{left: 13. right: 13. top: 9. bottom: 11.}
            spacing: 6.0

            name := Label{
                text: "dataset"
                draw_text +: {
                    light: instance(0.0)
                    text_style: mod.widgets.ux.FONT_MEDIUM{font_size: 13.5}
                    get_color: fn() { return mix(#xE6EAF2, #x1B2231, self.light) }
                }
            }
            meta := Label{
                text: "0 ep"
                draw_text +: {
                    light: instance(0.0)
                    text_style: mod.widgets.ux.TEXT_META{}
                    get_color: fn() { return mix(#x8B95AD, #x6A7387, self.light) }
                }
            }
            chips := View{
                width: Fill
                height: Fit
                flow: Right
                spacing: 7.0
                margin: Inset{top: 3.}
                chip_good := mod.widgets.ux.Chip{
                    draw_bg +: {tone: 1.0}
                    label +: { text: "0 good" draw_text +: {tone: 1.0} }
                }
                chip_bad := mod.widgets.ux.Chip{
                    draw_bg +: {tone: 2.0}
                    label +: { text: "0 bad" draw_text +: {tone: 2.0} }
                }
                chip_sync := mod.widgets.ux.Chip{
                    label +: { text: "synced" }
                }
            }
        }
    }

    // One connected device row in the hardware rail.
    let DeviceRow = View{
        width: Fill
        height: Fit
        flow: Right
        align: Align{y: 0.5}
        spacing: 12.0
        padding: Inset{top: 10. bottom: 10.}

        icon := RoundedView{
            width: 40 height: 40
            align: Align{x: 0.5 y: 0.5}
            show_bg: true
            draw_bg +: {
                kind: instance(0.0)
                light: instance(0.0)
                border_size: 1.0
                border_radius: 6.0
                pixel: fn() {
                    let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                    sdf.box(0.5, 0.5, self.rect_size.x - 1.0, self.rect_size.y - 1.0, 6.0)
                    sdf.fill_keep(mix(#x222836, #xF1F3F8, self.light))
                    sdf.stroke(mix(#x2A3140, #xE0E5EE, self.light), 1.0)
                    let c = mix(#xA9B3C8, #x5C6578, self.light)
                    if self.kind < 0.5 {
                        // robot arm
                        sdf.box(11.0, 27.0, 18.0, 4.0, 1.0) sdf.fill(c)
                        sdf.box(17.0, 14.0, 4.0, 14.0, 1.0) sdf.fill(c)
                        sdf.rotate(0.7, 19.0, 15.0)
                        sdf.box(19.0, 12.0, 12.0, 3.4, 1.0) sdf.fill(c)
                        sdf.rotate(-0.7, 19.0, 15.0)
                    } else {
                        // camera body + lens
                        sdf.box(9.0, 13.0, 22.0, 15.0, 2.5) sdf.stroke(c, 1.4)
                        sdf.circle(20.0, 20.5, 4.2) sdf.stroke(c, 1.4)
                    }
                    return sdf.result
                }
            }
        }
        text_col := View{
            width: Fill height: Fit flow: Down spacing: 3.0
            dev_name := Label{
                text: "device"
                draw_text +: {
                    light: instance(0.0)
                    text_style: mod.widgets.ux.TEXT_BODY{}
                    get_color: fn() { return mix(#xE6EAF2, #x1B2231, self.light) }
                }
            }
            dev_detail := Label{
                text: "-"
                draw_text +: {
                    light: instance(0.0)
                    text_style: mod.widgets.ux.TEXT_META{}
                    get_color: fn() { return mix(#x6C7591, #x848DA0, self.light) }
                }
            }
        }
        status_dot := View{
            width: 10 height: 10
            show_bg: true
            draw_bg +: {
                online: instance(1.0)
                pixel: fn() {
                    let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                    sdf.circle(5.0, 5.0, 4.0)
                    sdf.fill(mix(#x6C7591, #x58BE8A, self.online))
                    return sdf.result
                }
            }
        }
    }

    // One entry in the recent-sessions strip.
    let SessionCell = View{
        width: Fill
        height: Fit
        flow: Right
        spacing: 12.0
        padding: Inset{left: 14. right: 14.}

        sthumb := View{
            width: 74 height: 54
            show_bg: true
            draw_bg +: {
                light: instance(0.0)
                pixel: fn() {
                    let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                    let d = mix(#x2A3242, #x1B222E, self.pos.y)
                    let l = mix(#xEDF0F6, #xE2E7F0, self.pos.y)
                    sdf.box(0.0, 0.0, self.rect_size.x, self.rect_size.y, 4.0)
                    sdf.fill(mix(d, l, self.light))
                    return sdf.result
                }
            }
        }
        scol := View{
            width: Fill height: Fit flow: Down spacing: 5.0
            stitle := Label{
                text: "Session"
                draw_text +: {
                    light: instance(0.0)
                    text_style: mod.widgets.ux.TEXT_BODY{}
                    get_color: fn() { return mix(#xE6EAF2, #x1B2231, self.light) }
                }
            }
            sdataset := Label{
                text: "-"
                draw_text +: {
                    light: instance(0.0)
                    text_style: mod.widgets.ux.TEXT_META{}
                    get_color: fn() { return mix(#x8B95AD, #x6A7387, self.light) }
                }
            }
            sstate := mod.widgets.ux.Chip{
                draw_bg +: {tone: 1.0}
                label +: { text: "COMPLETED" draw_text +: {tone: 1.0} }
            }
        }
    }

    mod.widgets.LibraryScreenBase = #(LibraryScreen::register_widget(vm))
    mod.widgets.LibraryScreen = set_type_default() do mod.widgets.LibraryScreenBase{
        width: Fill
        height: Fill
        flow: Right
        spacing: 14.0
        padding: Inset{left: 16. right: 16. top: 14. bottom: 14.}
        show_bg: true
        draw_bg +: {
            light: instance(0.0)
            pixel: fn() { return mix(#x12151C, #xF4F6FA, self.light) }
        }

        left_col := View{
            width: Fill
            height: Fill
            flow: Down
            spacing: 14.0

            header := View{
                width: Fill height: Fit
                flow: Right
                align: Align{y: 0.5}
                spacing: 12.0
                padding: Inset{left: 4., bottom: 2.}

                page_title := Label{
                    text: "Datasets"
                    draw_text +: {
                        light: instance(0.0)
                        text_style: mod.widgets.ux.TEXT_H1{}
                        get_color: fn() { return mix(#xE6EAF2, #x161C29, self.light) }
                    }
                }
                Filler{}
                btn_new := Button{
                    text: "+  New Recording Session"
                    padding: Inset{left: 18. right: 18. top: 11. bottom: 11.}
                    draw_bg +: {
                        border_radius: 6.0
                        pixel: fn() {
                            let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                            let base = #x3B7BEE
                            let c = mix(base, #x5C93F7, self.hover)
                            sdf.box(0.5, 0.5, self.rect_size.x - 1.0, self.rect_size.y - 1.0, 6.0)
                            sdf.fill(mix(c, #x2E63C9, self.down))
                            return sdf.result
                        }
                    }
                    draw_text +: {
                        color: #xFFFFFF
                        color_hover: #xFFFFFF
                        color_down: #xFFFFFF
                        text_style: mod.widgets.ux.TEXT_BODY{}
                    }
                }
                btn_pull := Button{
                    text: "Pull from Hub"
                    padding: Inset{left: 18. right: 18. top: 11. bottom: 11.}
                    draw_bg +: {
                        light: instance(0.0)
                        border_radius: 6.0
                        pixel: fn() {
                            let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                            sdf.box(0.5, 0.5, self.rect_size.x - 1.0, self.rect_size.y - 1.0, 6.0)
                            let idle = mix(#x161B25, #xFFFFFF, self.light)
                            let hov = mix(#x1E2532, #xF1F4FA, self.light)
                            sdf.fill_keep(mix(idle, hov, self.hover))
                            sdf.stroke(mix(#x333B52, #xD7DCE7, self.light), 1.0)
                            return sdf.result
                        }
                    }
                    draw_text +: {
                        light: instance(0.0)
                        text_style: mod.widgets.ux.TEXT_BODY{}
                        get_color: fn() { return mix(#xC9D2E2, #x333B4D, self.light) }
                    }
                }
            }

            grid := mod.widgets.ux.Card{
                width: Fill
                height: Fit
                flow: Down
                padding: Inset{left: 12. right: 12. top: 12. bottom: 12.}
                spacing: 12.0

                row_a := View{
                    width: Fill height: Fit flow: Right spacing: 12.0
                    card_0 := DatasetCard{}
                    card_1 := DatasetCard{}
                    card_2 := DatasetCard{}
                }
                row_b := View{
                    width: Fill height: Fit flow: Right spacing: 12.0
                    card_3 := DatasetCard{}
                    card_4 := DatasetCard{}
                    card_5 := DatasetCard{}
                }
            }

            Filler{}

            recent := mod.widgets.ux.Card{
                width: Fill
                height: 134
                flow: Down
                rec_head := mod.widgets.ux.PanelHead{
                    title +: { text: "Recent sessions" }
                    btn_max +: { visible: false }
                    btn_close +: { visible: false }
                    view_all := Label{
                        text: "View all"
                        draw_text +: {
                            light: instance(0.0)
                            text_style: mod.widgets.ux.TEXT_BODY{}
                            get_color: fn() { return mix(#x6BA1F8, #x2159C4, self.light) }
                        }
                    }
                }
                rec_body := View{
                    width: Fill height: Fill flow: Right
                    padding: Inset{top: 12. bottom: 12.}
                    sess_0 := SessionCell{}
                    sess_1 := SessionCell{}
                    sess_2 := SessionCell{}
                }
            }
        }

        hardware_rail := mod.widgets.ux.Card{
            width: 306
            height: Fill
            flow: Down
            hw_head := mod.widgets.ux.PanelHead{ title +: { text: "Hardware" } }
            hw_body := View{
                width: Fill height: Fill flow: Down
                padding: Inset{left: 14. right: 14. top: 8.}
                spacing: 2.0
                dev_0 := DeviceRow{}
                dev_1 := DeviceRow{ icon +: { draw_bg +: {kind: 1.0} } }
                // A real control, not a painted one: it reads as a link, so it
                // has to behave like one. A dev View only hit-tests when it
                // declares a cursor.
                link_setup := View{
                    width: Fit height: Fit
                    margin: Inset{top: 14., left: 2.}
                    cursor: MouseCursor.Hand
                    setup_label := Label{
                        text: "+  Set up new robot"
                        draw_text +: {
                            light: instance(0.0)
                            hover: instance(0.0)
                            text_style: mod.widgets.ux.TEXT_BODY{}
                            get_color: fn() {
                                let base = mix(#x6BA1F8, #x2159C4, self.light)
                                return mix(base, mix(#x9CC3FF, #x0E3E96, self.light), self.hover)
                            }
                        }
                    }
                }
                Filler{}
            }
        }
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct LibraryScreen {
    #[deref]
    view: View,
}

impl Widget for LibraryScreen {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
}

const CARD_IDS: [&[LiveId]; 6] = [
    ids!(left_col.grid.row_a.card_0),
    ids!(left_col.grid.row_a.card_1),
    ids!(left_col.grid.row_a.card_2),
    ids!(left_col.grid.row_b.card_3),
    ids!(left_col.grid.row_b.card_4),
    ids!(left_col.grid.row_b.card_5),
];

const SESSION_IDS: [&[LiveId]; 3] = [
    ids!(left_col.recent.rec_body.sess_0),
    ids!(left_col.recent.rec_body.sess_1),
    ids!(left_col.recent.rec_body.sess_2),
];

const DEVICE_IDS: [&[LiveId]; 2] = [
    ids!(hardware_rail.hw_body.dev_0),
    ids!(hardware_rail.hw_body.dev_1),
];

impl LibraryScreenRef {
    /// Dataset id whose card was clicked, if any.
    /// True when "Set up new robot" was clicked. The Hardware screen is that
    /// flow — find the port, check the motors, calibrate, add cameras, save the
    /// profile — so the link goes there rather than opening a dialog of its own.
    pub fn clicked_setup(&self, cx: &mut Cx, actions: &Actions) -> bool {
        let Some(mut inner) = self.borrow_mut() else { return false };
        let link = inner.view.widget(cx, ids!(hardware_rail.hw_body.link_setup));
        !link.is_empty() && crate::ui::frame::view_clicked(actions, link.widget_uid())
    }

    pub fn clicked_dataset(&self, cx: &mut Cx, actions: &Actions, state: &LibraryState) -> Option<String> {
        let mut inner = self.borrow_mut()?;
        for (slot, path) in CARD_IDS.iter().enumerate() {
            let d = state.datasets.get(slot)?;
            let card = inner.view.widget(cx, path);
            if card.is_empty() {
                continue;
            }
            if crate::ui::frame::view_clicked(actions, card.widget_uid()) {
                return Some(d.id.clone());
            }
        }
        None
    }

    /// Push a state snapshot into the view.
    pub fn sync(&self, cx: &mut Cx, state: &LibraryState) {
        let light = crate::ui::frame::light_mode();
        let Some(mut inner) = self.borrow_mut() else { return };
        let root = &mut inner.view;

        // Static chrome: page background, panels, headers, links.
        script_apply_eval!(cx, root, { draw_bg +: { light: #(light) } });
        apply_light_in(cx, root, &[
            (ids!(left_col.header.page_title), Themed::Text),
            (ids!(left_col.header.btn_new), Themed::Both),
            (ids!(left_col.header.btn_pull), Themed::Both),
            (ids!(left_col.grid), Themed::Bg),
            (ids!(left_col.recent), Themed::Bg),
            (ids!(left_col.recent.rec_head.view_all), Themed::Text),
            (ids!(hardware_rail), Themed::Bg),
            (ids!(hardware_rail.hw_body.link_setup.setup_label), Themed::Text),
        ], light);
        for p in [ids!(left_col.recent.rec_head) as &[LiveId], ids!(hardware_rail.hw_head)] {
            let head = root.widget(cx, p);
            theme_panel_head(cx, &head, light);
        }

        for (slot, path) in CARD_IDS.iter().enumerate() {
            let card = root.widget(cx, path);
            if card.is_empty() {
                continue;
            }
            match state.datasets.get(slot) {
                Some(d) => {
                    card.set_visible(cx, true);
                    {
                        let mut c = card.clone();
                        script_apply_eval!(cx, c, { draw_bg +: { light: #(light) } });
                    }
                    let head = card.widget(cx, ids!(head));
                    theme_panel_head(cx, &head, light);
                    apply_light(cx, &card, &[
                        (ids!(thumb), Themed::Bg),
                        (ids!(body.name), Themed::Text),
                        (ids!(body.meta), Themed::Text),
                    ], light);
                    for cp in [ids!(body.chips.chip_good) as &[LiveId],
                               ids!(body.chips.chip_bad), ids!(body.chips.chip_sync)] {
                        let chip = card.widget(cx, cp);
                        theme_chip(cx, &chip, light);
                    }
                    card.label(cx, ids!(head.title)).set_text(cx, &d.name);
                    card.label(cx, ids!(body.name)).set_text(cx, &d.name);
                    card.label(cx, ids!(body.meta)).set_text(cx, &d.meta_line());
                    card.label(cx, ids!(body.chips.chip_good.label))
                        .set_text(cx, &format!("{} good", d.good));
                    card.label(cx, ids!(body.chips.chip_bad.label))
                        .set_text(cx, &format!("{} bad", d.bad));
                    card.label(cx, ids!(body.chips.chip_sync.label))
                        .set_text(cx, d.sync.label());

                    // "local only" reads as an accent chip, "synced" as neutral.
                    let tone = if d.sync == SyncState::LocalOnly { 3.0 } else { 0.0 };
                    let mut chip = card.widget(cx, ids!(body.chips.chip_sync));
                    script_apply_eval!(cx, chip, {
                        draw_bg +: { tone: #(tone) }
                    });
                    let mut chip_label = card.widget(cx, ids!(body.chips.chip_sync.label));
                    script_apply_eval!(cx, chip_label, {
                        draw_text +: { tone: #(tone) }
                    });
                }
                None => card.set_visible(cx, false),
            }
        }

        for (slot, path) in DEVICE_IDS.iter().enumerate() {
            let row = root.widget(cx, path);
            if row.is_empty() {
                continue;
            }
            match state.devices.get(slot) {
                Some(dev) => {
                    row.set_visible(cx, true);
                    apply_light(cx, &row, &[
                        (ids!(icon), Themed::Bg),
                        (ids!(text_col.dev_name), Themed::Text),
                        (ids!(text_col.dev_detail), Themed::Text),
                    ], light);
                    row.label(cx, ids!(text_col.dev_name)).set_text(cx, &dev.name);
                    row.label(cx, ids!(text_col.dev_detail)).set_text(cx, &dev.detail);
                    let on = if dev.online { 1.0 } else { 0.0 };
                    let kind = if dev.kind == DeviceKind::Camera { 1.0 } else { 0.0 };
                    let mut dot = row.widget(cx, ids!(status_dot));
                    script_apply_eval!(cx, dot, {
                        draw_bg +: { online: #(on) }
                    });
                    let mut icon = row.widget(cx, ids!(icon));
                    script_apply_eval!(cx, icon, {
                        draw_bg +: { kind: #(kind) }
                    });
                }
                None => row.set_visible(cx, false),
            }
        }

        for (slot, path) in SESSION_IDS.iter().enumerate() {
            let cell = root.widget(cx, path);
            if cell.is_empty() {
                continue;
            }
            match state.sessions.get(slot) {
                Some(s) => {
                    cell.set_visible(cx, true);
                    apply_light(cx, &cell, &[
                        (ids!(sthumb), Themed::Bg),
                        (ids!(scol.stitle), Themed::Text),
                        (ids!(scol.sdataset), Themed::Text),
                    ], light);
                    let chip = cell.widget(cx, ids!(scol.sstate));
                    theme_chip(cx, &chip, light);
                    cell.label(cx, ids!(scol.stitle))
                        .set_text(cx, &format!("Session {}", s.started));
                    cell.label(cx, ids!(scol.sdataset))
                        .set_text(cx, &format!("{} · {} ep", s.dataset, s.episodes));
                    cell.label(cx, ids!(scol.sstate.label))
                        .set_text(cx, s.outcome.label());
                    // in-progress reads amber (tone 4), completed green (tone 1)
                    let tone = if s.outcome == SessionOutcome::InProgress { 4.0 } else { 1.0 };
                    let mut chip = cell.widget(cx, ids!(scol.sstate));
                    script_apply_eval!(cx, chip, {
                        draw_bg +: { tone: #(tone) }
                    });
                    let mut chip_label = cell.widget(cx, ids!(scol.sstate.label));
                    script_apply_eval!(cx, chip_label, {
                        draw_text +: { tone: #(tone) }
                    });
                }
                None => cell.set_visible(cx, false),
            }
        }
    }
}
