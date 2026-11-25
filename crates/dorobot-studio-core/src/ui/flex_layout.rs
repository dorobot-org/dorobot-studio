//! Stable panel identities: changing geometry never swaps media or widget state.
use makepad_widgets::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PanelLayout {
    pub order: Vec<usize>,
    pub visible: Vec<bool>,
    pub maximized: Option<usize>,
    pub columns: usize,
    pub split_x: f64,
    pub split_y: f64,
}
impl Default for PanelLayout {
    fn default() -> Self {
        Self {
            order: vec![0, 1, 2, 4, 3],
            visible: vec![true; 5],
            maximized: None,
            columns: 2,
            split_x: 0.5,
            split_y: 0.5,
        }
    }
}
impl PanelLayout {
    fn default_for(name: &str) -> Self {
        let mut layout = Self::default();
        if name == "hardware-layout.json" { layout.split_x = 0.6; }
        if name.starts_with("live-") { layout.visible[3] = false; }
        if name.contains("hardware-camera-") {
            layout.order = vec![1, 2, 4, 0, 3];
            layout.visible[3] = true;
            layout.columns = 3; // Hardware: main camera with inset views.
        }
        layout
    }
    // Preserve saved four-panel layouts, including order, visibility and splits.
    fn upgrade_legacy(mut self) -> Self {
        let mut ids = self.order.clone();
        ids.sort_unstable();
        if ids == [0, 1, 2, 3] && self.visible.len() == 4 {
            self.order.push(4);
            self.visible.push(true);
        }
        self
    }
    pub fn validate(&self) -> bool {
        let mut order = self.order.clone();
        order.sort_unstable();
        order == [0, 1, 2, 3, 4]
            && self.visible.len() == 5
            && (1..=5).contains(&self.columns)
            && self.maximized.is_none_or(|i| i < 5 && self.visible[i])
            && self.split_x.is_finite()
            && (0.2..=0.8).contains(&self.split_x)
            && self.split_y.is_finite()
            && (0.2..=0.8).contains(&self.split_y)
    }
    fn path(name: &str) -> Option<std::path::PathBuf> {
        crate::preferences::path().map(|p| p.with_file_name(name))
    }
    fn load(name: &str) -> Self {
        Self::path(name)
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice::<Self>(&b).ok())
            .map(Self::upgrade_legacy)
            .filter(Self::validate)
            .unwrap_or_else(|| Self::default_for(name))
    }
    fn save(&self, name: &str) {
        if let Some(path) = Self::path(name) {
            let result = (|| -> std::io::Result<()> {
                std::fs::create_dir_all(path.parent().unwrap())?;
                let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
                std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
                std::fs::rename(tmp, path)
            })();
            if let Err(e) = result {
                ::log::warn!("Could not save replay layout: {}", e.kind());
            }
        }
    }
    pub fn move_before(&mut self, source: usize, target: usize) {
        if source == target || source >= 5 || target >= 5 {
            return;
        }
        self.order.retain(|i| *i != source);
        let at = self
            .order
            .iter()
            .position(|i| *i == target)
            .unwrap_or(self.order.len());
        self.order.insert(at, source);
    }
    pub fn promote(&mut self, id: usize) {
        if let Some(at) = self.order.iter().position(|i| *i == id) {
            self.order.swap(0, at);
            self.maximized = None;
        }
    }
    pub fn toggle(&mut self, id: usize) {
        if id >= 5 {
            return;
        }
        self.visible[id] = !self.visible[id];
        self.maximized = None;
    }
}

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    mod.widgets.FlexPanelsBase = #(FlexPanels::register_widget(vm))
    mod.widgets.FlexPanels = set_type_default() do mod.widgets.FlexPanelsBase{
        persistence_key: "replay-layout.json"
        panel_count: 5
        width: Fill height: Fill flow: Overlay
        show_bg: false
    }
    // Desktop panel utilities should not inherit the iOS 44px touch minimum.
    mod.widgets.PanelToolButton = mod.widgets.ux.ToolButton{
        width: 24 height: 24 min_height: 24
        padding: 0.0
    }
    mod.widgets.StudioPanel = mod.widgets.ux.Card{
        width: Fill height: Fill flow: Down
        padding: Inset{left: 6. right: 6. top: 0. bottom: 6.}
        head := View{width: Fill height: 28 flow: Right spacing: 4.0 align: Align{y: 0.5}
            cursor: MouseCursor.Hand
            grip := Label{width: 14 text: "⠿" draw_text.color: mod.theme.color_text_disabled}
            title := Label{width: Fill max_lines: 1 text_overflow: Ellipsis
                draw_text +: {color: mod.theme.color_text text_style: mod.widgets.ux.TEXT_NAV{}}}
            maximize := mod.widgets.PanelToolButton{text: "⤢"}
            close := mod.widgets.PanelToolButton{text: "×"}
        }
    }
}

#[derive(Script, ScriptHook, Widget)]
pub struct FlexPanels {
    #[deref]
    view: View,
    #[live]
    persistence_key: String,
    #[live(5usize)]
    panel_count: usize,
    #[rust]
    loaded: bool,
    #[rust(-1.0)]
    themed_light: f64,
    #[rust]
    state: PanelLayout,
    #[rust]
    drag: Option<(usize, DVec2)>,
    #[rust]
    resize: Option<bool>,
    #[rust]
    drop_target: Option<usize>,
    #[rust]
    rect: Rect,
    #[rust]
    panel_rects: [Rect; 5],
    #[rust]
    unavailable: Vec<usize>,
    #[rust]
    handles: [Rect; 2],
    #[live]
    preview: DrawColor,
}
const PANELS: [&[LiveId]; 5] = [
    ids!(robot_panel),
    ids!(camera_a_panel),
    ids!(camera_b_panel),
    ids!(error_panel),
    ids!(camera_c_panel),
];

impl FlexPanels {
    fn changed(&mut self, cx: &mut Cx) {
        self.state.save(&self.persistence_key);
        self.view.redraw(cx);
    }
    fn camera_layout(&self) -> bool {
        self.persistence_key.contains("hardware-camera-") && self.state.columns == 3
    }
    fn place(&mut self, cx: &mut Cx, rect: Rect) {
        self.rect = rect;
        self.panel_rects = [Rect::default(); 5];
        self.handles = [Rect::default(); 2];
        let visible: Vec<_> = self
            .state
            .order
            .iter()
            .copied()
            .filter(|i| {
                *i < self.panel_count
                    && !self.unavailable.contains(i)
                    && self.state.visible[*i]
                    && self.state.maximized.is_none_or(|m| m == *i)
            })
            .collect();
        let cols = self.state.columns.min(visible.len()).max(1);
        let rows = visible.len().div_ceil(cols).max(1);
        let gap = 8.0;
        for (slot, id) in visible.iter().copied().enumerate() {
            let (col, row) = (slot % cols, slot / cols);
            let (x, w) = if cols == 2 {
                let left = (rect.size.x - gap).max(0.0) * self.state.split_x;
                if col == 0 {
                    (0.0, left)
                } else {
                    (left + gap, rect.size.x - left - gap)
                }
            } else {
                let w = (rect.size.x - gap * (cols - 1) as f64) / cols as f64;
                (col as f64 * (w + gap), w)
            };
            let (y, h) = if rows == 2 {
                let top = (rect.size.y - gap).max(0.0) * self.state.split_y;
                if row == 0 {
                    (0.0, top)
                } else {
                    (top + gap, rect.size.y - top - gap)
                }
            } else {
                let h = (rect.size.y - gap * (rows - 1) as f64) / rows as f64;
                (row as f64 * (h + gap), h)
            };
            self.panel_rects[id] = Rect {
                pos: rect.pos + dvec2(x, y),
                size: dvec2(w.max(0.0), h.max(0.0)),
            };
        }
        if cols == 2 {
            self.handles[0] = Rect {
                pos: rect.pos + dvec2((rect.size.x - gap) * self.state.split_x, 0.0),
                size: dvec2(gap, rect.size.y),
            };
        }
        if rows == 2 {
            self.handles[1] = Rect {
                pos: rect.pos + dvec2(0.0, (rect.size.y - gap) * self.state.split_y),
                size: dvec2(rect.size.x, gap),
            };
        }
        if self.camera_layout() && self.state.maximized.is_none() {
            self.handles = [Rect::default(); 2];
            self.panel_rects = camera_rects(rect, &visible);
        }
        // Keep the selected feed first in traversal order.
        // Keep widget identities intact: swapping views never reopens media.
        if self.camera_layout() {
            if let Some(&main) = visible.first() {
                let main_id = PANELS[main][0];
                if let Some(at) = self.view.children.iter().position(|(id, _)| *id == main_id) {
                    let child = self.view.children.remove(at);
                    self.view.children.insert(0, child);
                }
            }
        }
        for (i, path) in PANELS.iter().enumerate() {
            let panel = self.view.widget(cx, path);
            panel.set_visible(cx, visible.contains(&i));
            if let Some(mut v) = panel.borrow_mut::<View>() {
                let r = self.panel_rects[i];
                v.walk = Walk {
                    abs_pos: Some(r.pos),
                    width: Size::Fixed(r.size.x),
                    height: Size::Fixed(r.size.y),
                    ..Walk::default()
                };
            }
            panel.button(cx, ids!(head.maximize)).set_text(
                cx,
                if self.state.maximized == Some(i) {
                    "⤡"
                } else {
                    "⤢"
                },
            );
        }
        self.view
            .widget(cx, ids!(empty))
            .set_visible(cx, visible.is_empty());
    }
}
/// Small views occupy a separate top row; the selected camera gets all of
/// the space below. No part of any feed is covered by another panel.
fn camera_rects(rect: Rect, visible: &[usize]) -> [Rect; 5] {
    let mut panels = [Rect::default(); 5];
    let Some((&main, insets)) = visible.split_first() else { return panels; };
    panels[main] = rect;
    if insets.is_empty() { return panels; }
    let gap = 8.0;
    let count = insets.len() as f64;
    let width = ((rect.size.x - gap * (count - 1.0)) / count).max(0.0);
    let height = if insets.len() >= 4 {
        (rect.size.y * 0.30).min(210.0).max(0.0)
    } else {
        (rect.size.y * 0.25).min(180.0).max(0.0)
    };
    for (slot, id) in insets.iter().copied().enumerate() {
        panels[id] = Rect {
            pos: rect.pos + dvec2(slot as f64 * (width + gap), 0.0),
            size: dvec2(width, height),
        };
    }
    panels[main] = Rect {
        pos: rect.pos + dvec2(0.0, height + gap),
        size: dvec2(rect.size.x, (rect.size.y - height - gap).max(0.0)),
    };
    panels
}

impl Widget for FlexPanels {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        if !self.view.visible { return; }
        // Select an inset before its child handles the press (especially the
        // map's orbit control). Its title bar still supports drag/close/zoom.
        if self.camera_layout() && self.state.maximized.is_none() {
            if let Event::MouseDown(e) = event {
                let visible: Vec<_> = self.state.order.iter().copied()
                    .filter(|i| self.state.visible[*i] && !self.unavailable.contains(i))
                    .collect();
                if let Some(id) = visible.iter().skip(1).copied().find(|id| {
                    let r = self.panel_rects[*id];
                    *id != 3 && r.contains(e.abs) && e.abs.y >= r.pos.y + 28.0
                }) {
                    self.state.promote(id);
                    self.changed(cx);
                    return;
                }
            }
        }
        let actions = cx.capture_actions(|cx| self.view.handle_event(cx, event, scope));
        for (i, path) in PANELS.iter().enumerate() {
            let panel = self.view.widget(cx, path);
            if panel.button(cx, ids!(head.close)).clicked(&actions) {
                self.state.toggle(i);
                self.changed(cx);
            }
            if panel.button(cx, ids!(head.maximize)).clicked(&actions) {
                self.state.maximized = if self.state.maximized == Some(i) {
                    None
                } else {
                    Some(i)
                };
                self.changed(cx);
            }
        }
        cx.extend_actions(actions);
        match event {
            Event::MouseDown(e) if self.rect.contains(e.abs) => {
                for (i, r) in self.handles.iter().enumerate() {
                    if r.contains(e.abs) && r.size.x > 0.0 {
                        self.resize = Some(i == 0);
                        return;
                    }
                }
                for i in self.state.order.iter().copied().rev() {
                    let r = &self.panel_rects[i];
                    if r.contains(e.abs)
                        && e.abs.y < r.pos.y + 28.0
                        && e.abs.x < r.pos.x + r.size.x - 64.0
                    {
                        self.drag = Some((i, e.abs));
                        break;
                    }
                }
            }
            Event::MouseMove(e) => {
                if let Some(horizontal) = self.resize {
                    if horizontal {
                        self.state.split_x =
                            ((e.abs.x - self.rect.pos.x) / self.rect.size.x).clamp(0.2, 0.8);
                    } else {
                        self.state.split_y =
                            ((e.abs.y - self.rect.pos.y) / self.rect.size.y).clamp(0.2, 0.8);
                    }
                    self.view.redraw(cx);
                } else if let Some((source, start)) = self.drag {
                    if (e.abs - start).length() > 8.0 {
                        self.drop_target = self.state.order.iter().copied().rev()
                            .find(|i| *i != source && self.panel_rects[*i].contains(e.abs));
                        self.view.redraw(cx);
                    }
                } else if self.handles[0].contains(e.abs) {
                    cx.set_cursor(MouseCursor::ColResize);
                } else if self.handles[1].contains(e.abs) {
                    cx.set_cursor(MouseCursor::RowResize);
                }
            }
            Event::MouseUp(_) => {
                if let Some((source, _)) = self.drag.take() {
                    if let Some(target) = self.drop_target.take() {
                        self.state.move_before(source, target);
                    }
                    self.changed(cx);
                }
                if self.resize.take().is_some() {
                    self.changed(cx);
                }
            }
            Event::KeyDown(e) if e.key_code == KeyCode::Escape => {
                self.drag = None;
                self.drop_target = None;
                self.resize = None;
                self.state.maximized = None;
                self.changed(cx);
            }
            _ => {}
        }
    }
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        let light = crate::ui::frame::light_mode();
        if self.themed_light != light {
            self.themed_light = light;
            for path in PANELS {
                let mut panel = self.view.widget(cx, path);
                if panel.is_empty() {
                    continue;
                }
                script_apply_eval!(cx, panel, {draw_bg +: {light: #(light)}});
                crate::ui::frame::apply_light(
                    cx,
                    &panel,
                    &[
                        (ids!(head.maximize), crate::ui::frame::Themed::Both),
                        (ids!(head.close), crate::ui::frame::Themed::Both),
                    ],
                    light,
                );
            }
        }
        if !self.loaded {
            self.state = PanelLayout::load(&self.persistence_key);
            for i in self.panel_count.min(5)..5 {
                self.state.visible[i] = false;
            }
            if self
                .state
                .maximized
                .is_some_and(|i| i >= self.panel_count || self.unavailable.contains(&i))
            {
                self.state.maximized = None;
            }
            self.loaded = true;
        }
        let rect = cx.peek_walk_turtle(walk);
        self.place(cx, rect);
        let result = self.view.draw_walk(cx, scope, walk);
        if let Some(id) = self.drop_target {
            self.preview.color = vec4(0.04, 0.48, 1.0, 0.18);
            self.preview.draw_abs(cx, self.panel_rects[id]);
        }
        result
    }
}
impl FlexPanelsRef {
    /// External agents need an orbitable scene, without the gamepad trace map.
    /// Preserve separate layouts when switching back to the built-in controller.
    pub fn set_external_source(&self, cx: &mut Cx, external: bool) {
        if let Some(mut v) = self.borrow_mut() {
            let key = if external { "live-external-camera-layout.json" } else { "live-hardware-camera-layout.json" };
            if v.persistence_key == key { return; }
            if v.loaded { v.state.save(&v.persistence_key); }
            v.persistence_key = key.into();
            v.state = PanelLayout::load(key);
            v.loaded = true;
            v.view.redraw(cx);
        }
    }
    pub fn is_hardware_camera(&self) -> bool {
        self.borrow().is_some_and(|v| v.persistence_key.contains("hardware-camera-"))
    }
    pub fn is_camera_layout(&self) -> bool {
        self.borrow().is_some_and(|v| v.camera_layout())
    }
    /// Live and Replay keep their own layouts, including the monitor window.
    pub fn set_live_mode(&self, cx: &mut Cx, live: bool) {
        if let Some(mut v) = self.borrow_mut() {
            if v.persistence_key.starts_with("live-") == live { return; }
            if v.loaded { v.state.save(&v.persistence_key); }
            v.persistence_key = if live { format!("live-{}", v.persistence_key) } else { v.persistence_key.trim_start_matches("live-").to_owned() };
            v.state = PanelLayout::load(&v.persistence_key);
            v.loaded = true;
            v.view.redraw(cx);
        }
    }
    pub fn set_panel_available(&self, cx: &mut Cx, id: usize, available: bool) {
        if let Some(mut v) = self.borrow_mut() {
            v.unavailable.retain(|i| *i != id);
            if !available {
                v.unavailable.push(id);
                if v.state.maximized == Some(id) {
                    v.state.maximized = None;
                }
            }
            v.view.redraw(cx);
        }
    }

    pub fn snapshot(&self) -> Option<PanelLayout> {
        self.borrow().map(|v| v.state.clone())
    }
    pub fn toggle_panel(&self, cx: &mut Cx, id: usize) {
        if let Some(mut v) = self.borrow_mut() {
            v.state.toggle(id);
            v.changed(cx);
        }
    }
    pub fn reset(&self, cx: &mut Cx) {
        if let Some(mut v) = self.borrow_mut() {
            v.state = PanelLayout::default_for(&v.persistence_key);
            for i in v.panel_count.min(5)..5 {
                v.state.visible[i] = false;
            }
            v.changed(cx);
        }
    }
    pub fn cycle_layout(&self, cx: &mut Cx) {
        if let Some(mut v) = self.borrow_mut() {
            v.state.columns = match v.state.columns {
                3 if v.persistence_key.contains("hardware-camera-") => 2,
                2 => 1,
                1 => v.panel_count.min(5),
                _ if v.persistence_key.contains("hardware-camera-") => 3,
                _ => 2,
            };
            v.state.maximized = None;
            v.changed(cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hardware_camera_swap_preserves_insets_and_other_layouts() {
        let mut state = PanelLayout::default_for("live-hardware-camera-layout.json");
        let rect = Rect { pos: dvec2(200., 100.), size: dvec2(1100., 650.) };
        let visible: Vec<_> = state.order.iter().copied().filter(|i| state.visible[*i]).collect();
        let before = camera_rects(rect, &visible);
        assert_eq!(before[1].size.x, rect.size.x);
        assert!(before[1].size.y > rect.size.y * 0.65);
        for id in [0, 2, 3, 4] {
            assert!(before[id].size.y <= 210.);
            assert!(before[id].pos.y + before[id].size.y < before[1].pos.y);
            assert!(rect.contains(before[id].pos));
            assert!(rect.contains(before[id].pos + before[id].size));
        }
        state.promote(2);
        assert_eq!(state.order, [2, 1, 4, 0, 3]);
        let visible: Vec<_> = state.order.iter().copied().filter(|i| state.visible[*i]).collect();
        assert_eq!(camera_rects(rect, &visible)[2], before[1]);
        state.promote(1);
        assert_eq!(state.order, [1, 2, 4, 0, 3]);
        assert!(state.validate());
        assert_eq!(PanelLayout::default_for("replay-layout.json").columns, 2);
        assert_eq!(PanelLayout::default_for("live-replay-layout.json").columns, 2);
    }
    #[test]
    fn reorder_close_restore_preserves_all_identities() {
        let mut s = PanelLayout::default();
        s.move_before(3, 0);
        s.toggle(1);
        let mut restored: PanelLayout =
            serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(restored.order, [3, 0, 1, 2, 4]);
        assert!(!restored.visible[1]);
        restored.toggle(1);
        assert!(restored.visible[1]);
        assert!(restored.validate());
    }
    #[test]
    fn migrates_four_panel_preferences_without_losing_layout() {
        let old = r#"{"order":[3,0,2,1],"visible":[true,false,true,true],"columns":4,"split_x":0.3,"maximized":2}"#;
        let layout = serde_json::from_str::<PanelLayout>(old)
            .unwrap()
            .upgrade_legacy();
        assert!(layout.validate());
        assert_eq!(layout.order, [3, 0, 2, 1, 4]);
        assert_eq!(layout.visible, [true, false, true, true, true]);
        assert_eq!(layout.maximized, Some(2));
        assert_eq!(layout.split_x, 0.3);
    }
    #[test]
    fn rejects_missing_duplicate_and_invalid_panels() {
        let mut s = PanelLayout::default();
        s.order = vec![0, 0, 2, 3];
        assert!(!s.validate());
        s = PanelLayout::default();
        s.maximized = Some(8);
        assert!(!s.validate());
        s = PanelLayout::default();
        s.split_x = f64::NAN;
        assert!(!s.validate());
    }
}
