//! The dataflow graph: a layered layout (pure functions) and the custom-drawn
//! `DvizGraph` widget that draws it and reports clicks on node boxes.
//!
//! Left-to-right layering and status-tinted boxes follow dviz's
//! `dviz-widgets/src/dataflow_graph.rs` (dorobot-org/dviz, Apache-2.0); the
//! code here is new.

use dorobot_studio_plugin::*;
use dorobot_ux::tokens::pal;
use dviz_protocol::{GraphEdge, GraphNode, GraphNodeStatus};
use std::collections::{HashMap, HashSet, VecDeque};

/// Box size and gaps when there is room; with less, all shrink together.
const BOX_W: f64 = 140.0;
const BOX_H: f64 = 30.0;
const GAP_X: f64 = 56.0;
const GAP_Y: f64 = 14.0;
/// Margin around the graph.
const PAD: f64 = 12.0;
/// Arrowhead length at the target end of a link.
const ARROW: f64 = 6.0;
/// Space between a box's left edge and its label.
const TEXT_INSET: f64 = 8.0;

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*

    // A rounded node box; fill, border and border width are set per box.
    let DrawGraphBox = set_type_default() do #(DrawGraphBox::script_shader(vm)){
        ..mod.draw.DrawQuad
        pixel: fn() {
            let sdf = Sdf2d.viewport(self.pos * self.rect_size)
            sdf.box(1.0, 1.0, self.rect_size.x - 2.0, self.rect_size.y - 2.0, 4.0)
            sdf.fill_keep(self.box_fill)
            sdf.stroke(self.box_edge, self.edge_width)
            return sdf.result
        }
    }

    // An anti-aliased segment from seg_a to seg_b inside its bounding quad,
    // after the segment shader of Makepad's charts (MIT OR Apache-2.0).
    let DrawGraphLine = set_type_default() do #(DrawGraphLine::script_shader(vm)){
        ..mod.draw.DrawQuad
        pixel: fn() {
            let p = self.pos * self.rect_size
            let pa = p - self.seg_a
            let ba = self.seg_b - self.seg_a
            let h = clamp(dot(pa, ba) / max(dot(ba, ba), 0.0001), 0.0, 1.0)
            let d = length(pa - ba * h)
            let aa = 1.0 - smoothstep(self.thickness * 0.5 - 0.75, self.thickness * 0.5 + 0.75, d)
            return vec4(self.color.rgb * self.color.a * aa, self.color.a * aa)
        }
    }

    mod.widgets.DvizGraphBase = #(DvizGraph::register_widget(vm))
    mod.widgets.DvizGraph = set_type_default() do mod.widgets.DvizGraphBase{
        width: Fill
        height: Fill
        draw_box: DrawGraphBox{}
        draw_line: DrawGraphLine{}
        draw_text +: {text_style: mod.widgets.ux.TEXT_META{}}
    }
}

/// A position in the graph's own coordinates: pixels from its top-left.
pub type Point = Vec2d;

/// Where each node box and link goes, in the graph's own coordinates.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GraphLayout {
    /// One box per distinct node id, in the order the nodes were given.
    pub boxes: Vec<(String, Rect)>,
    /// One link per connected pair of distinct, known nodes, from the right
    /// edge of the source box to the left edge of the target box.
    pub links: Vec<(Point, Point)>,
}

/// What a click on the graph reports.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum DvizGraphAction {
    #[default]
    None,
    /// The id of the node whose box was clicked.
    Selected(String),
}

/// Lays the graph out in layers, left to right. Nodes without inputs from
/// other known nodes form the first layer; every other node sits one layer
/// right of its furthest input. A layer is ordered by the mean height of its
/// nodes' inputs (the first keeps the given order), and the whole grid is
/// centred in `size`, shrinking to fit (to empty boxes if there is no room).
///
/// Edges from or to unknown nodes (such as dora timers) and self-loops get
/// no link. A cycle is entered at a node that a placed node feeds, else at
/// its first node; its other edges then run back to the left.
pub fn layout(nodes: &[GraphNode], edges: &[GraphEdge], size: (f64, f64)) -> GraphLayout {
    let mut index = HashMap::new();
    let mut ids: Vec<&str> = Vec::new();
    for node in nodes {
        index.entry(node.id.as_str()).or_insert_with(|| {
            ids.push(&node.id);
            ids.len() - 1
        });
    }
    if ids.is_empty() {
        return GraphLayout::default();
    }
    let mut linked = HashSet::new();
    let pairs: Vec<(usize, usize)> = edges
        .iter()
        .filter_map(|edge| {
            Some((
                *index.get(edge.from_node.as_str())?,
                *index.get(edge.to_node.as_str())?,
            ))
        })
        .filter(|&(from, to)| from != to && linked.insert((from, to)))
        .collect();
    let rects = place(&columns(ids.len(), &pairs), ids.len(), size);
    GraphLayout {
        boxes: ids
            .iter()
            .zip(&rects)
            .map(|(id, rect)| (id.to_string(), *rect))
            .collect(),
        links: pairs
            .iter()
            .map(|&(from, to)| {
                let (a, b) = (rects[from], rects[to]);
                (
                    dvec2(a.pos.x + a.size.x, a.pos.y + a.size.y / 2.0),
                    dvec2(b.pos.x, b.pos.y + b.size.y / 2.0),
                )
            })
            .collect(),
    }
}

/// The node whose (non-empty) box contains `p`.
pub fn hit_test(layout: &GraphLayout, p: Point) -> Option<String> {
    layout
        .boxes
        .iter()
        .find(|(_, rect)| rect.size.x > 0.0 && rect.size.y > 0.0 && rect.contains(p))
        .map(|(id, _)| id.clone())
}

/// Node indices per layer, top to bottom. `pairs` are distinct links
/// between distinct nodes `0..n`.
fn columns(n: usize, pairs: &[(usize, usize)]) -> Vec<Vec<usize>> {
    let mut inputs = vec![Vec::new(); n];
    let mut outputs = vec![Vec::new(); n];
    for &(from, to) in pairs {
        inputs[to].push(from);
        outputs[from].push(to);
    }
    let layer = layers(&inputs, &outputs);
    let mut columns = vec![Vec::new(); layer.iter().max().map_or(0, |last| last + 1)];
    for (v, &l) in layer.iter().enumerate() {
        columns[l].push(v);
    }
    let rows = columns.iter().map(Vec::len).max().unwrap_or(0);
    // Each placed node's height, in rows from the top of the grid.
    let mut height = vec![0.0; n];
    for c in 0..columns.len() {
        if c > 0 {
            let mut keyed: Vec<(f64, usize)> = columns[c]
                .iter()
                .map(|&v| {
                    let left: Vec<f64> = inputs[v]
                        .iter()
                        .filter(|&&p| layer[p] < c)
                        .map(|&p| height[p])
                        .collect();
                    let mean = if left.is_empty() {
                        f64::MAX
                    } else {
                        left.iter().sum::<f64>() / left.len() as f64
                    };
                    (mean, v)
                })
                .collect();
            keyed.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
            columns[c] = keyed.into_iter().map(|(_, v)| v).collect();
        }
        let first_row = first_row(rows, columns[c].len());
        for (row, &v) in columns[c].iter().enumerate() {
            height[v] = first_row + row as f64;
        }
    }
    columns
}

/// The layer of each node, in topological (Kahn) order: a node lands one
/// layer right of its furthest placed input. When only cycles are left, the
/// first node that a placed node feeds is placed next, else the first
/// unplaced node, so every node is placed exactly once.
fn layers(inputs: &[Vec<usize>], outputs: &[Vec<usize>]) -> Vec<usize> {
    let n = inputs.len();
    // Inputs of each node that are not placed yet.
    let mut waiting: Vec<usize> = inputs.iter().map(Vec::len).collect();
    let mut layer: Vec<Option<usize>> = vec![None; n];
    let mut ready: VecDeque<usize> = (0..n).filter(|&v| waiting[v] == 0).collect();
    for _ in 0..n {
        let next = ready.pop_front().or_else(|| {
            let unplaced = || (0..n).filter(|&v| layer[v].is_none());
            unplaced()
                .find(|&v| inputs[v].iter().any(|&p| layer[p].is_some()))
                .or_else(|| unplaced().next())
        });
        let Some(v) = next else { break };
        layer[v] = Some(
            inputs[v]
                .iter()
                .filter_map(|&p| layer[p])
                .map(|l| l + 1)
                .max()
                .unwrap_or(0),
        );
        for &to in &outputs[v] {
            waiting[to] = waiting[to].saturating_sub(1);
            if waiting[to] == 0 && layer[to].is_none() {
                ready.push_back(to);
            }
        }
    }
    layer.into_iter().map(|l| l.unwrap_or(0)).collect()
}

/// The row a column of `len` boxes starts at, centring it among `rows`.
fn first_row(rows: usize, len: usize) -> f64 {
    rows.saturating_sub(len) as f64 / 2.0
}

/// A box per node: one column per layer, each column centred vertically,
/// the whole grid centred in `size`.
fn place(columns: &[Vec<usize>], n: usize, size: (f64, f64)) -> Vec<Rect> {
    let room = |v: f64| if v.is_finite() { v.max(0.0) } else { 0.0 };
    let (w, h) = (room(size.0), room(size.1));
    let rows = columns.iter().map(Vec::len).max().unwrap_or(0).max(1);
    let cols = columns.len().max(1) as f64;
    let pitch_x = ((w - 2.0 * PAD).max(0.0) / cols).min(BOX_W + GAP_X);
    let pitch_y = ((h - 2.0 * PAD).max(0.0) / rows as f64).min(BOX_H + GAP_Y);
    let box_size = dvec2(
        pitch_x * BOX_W / (BOX_W + GAP_X),
        pitch_y * BOX_H / (BOX_H + GAP_Y),
    );
    let left = (w - pitch_x * cols) / 2.0;
    let top = (h - pitch_y * rows as f64) / 2.0;
    let mut rects = vec![Rect::default(); n];
    for (c, column) in columns.iter().enumerate() {
        let first_row = first_row(rows, column.len());
        for (row, &v) in column.iter().enumerate() {
            rects[v] = Rect {
                pos: dvec2(
                    left + c as f64 * pitch_x + (pitch_x - box_size.x) / 2.0,
                    top + (first_row + row as f64) * pitch_y + (pitch_y - box_size.y) / 2.0,
                ),
                size: box_size,
            };
        }
    }
    rects
}

/// `text` cut to at most `max_chars` characters, ending in an ellipsis when cut.
fn fit_label(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(max_chars.saturating_sub(1)).collect();
    if max_chars > 0 {
        cut.push('…');
    }
    cut
}

fn status_color(t: &pal::Th, status: GraphNodeStatus) -> Vec4f {
    match status {
        GraphNodeStatus::Active => t.ok(),
        GraphNodeStatus::Idle => t.faint(),
        GraphNodeStatus::Error => t.hot(),
    }
}

/// Only what is drawn counts: ids and status, not `last_seen`.
fn same_nodes(a: &[GraphNode], b: &[GraphNode]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(a, b)| a.id == b.id && a.status == b.status)
}

fn same_edges(a: &[GraphEdge], b: &[GraphEdge]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(a, b)| {
            a.from_node == b.from_node
                && a.from_port == b.from_port
                && a.to_node == b.to_node
                && a.to_port == b.to_port
        })
}

#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawGraphBox {
    #[deref]
    draw_super: DrawQuad,
    #[live]
    box_fill: Vec4f,
    #[live]
    box_edge: Vec4f,
    #[live(1.0)]
    edge_width: f32,
}

#[derive(Script, ScriptHook)]
#[repr(C)]
pub struct DrawGraphLine {
    #[deref]
    draw_super: DrawQuad,
    #[live]
    color: Vec4f,
    #[live]
    seg_a: Vec2f,
    #[live]
    seg_b: Vec2f,
    #[live(1.5)]
    thickness: f32,
}

impl DrawGraphLine {
    /// A segment from `a` to `b` (absolute), drawn in its padded bounding box.
    fn segment(&mut self, cx: &mut Cx2d, a: Vec2d, b: Vec2d) {
        let pad = self.thickness as f64 + 1.0;
        let pos = dvec2(a.x.min(b.x) - pad, a.y.min(b.y) - pad);
        let size = dvec2((a.x - b.x).abs() + 2.0 * pad, (a.y - b.y).abs() + 2.0 * pad);
        self.seg_a = Vec2f {
            x: (a.x - pos.x) as f32,
            y: (a.y - pos.y) as f32,
        };
        self.seg_b = Vec2f {
            x: (b.x - pos.x) as f32,
            y: (b.y - pos.y) as f32,
        };
        self.draw_abs(cx, Rect { pos, size });
    }

    /// A link with an arrowhead at `b`.
    fn link(&mut self, cx: &mut Cx2d, a: Vec2d, b: Vec2d) {
        self.segment(cx, a, b);
        let length = (b - a).length();
        if length > 2.0 * ARROW {
            let along = (b - a) / length;
            let across = dvec2(-along.y, along.x) * (ARROW / 2.0);
            let back = b - along * ARROW;
            self.segment(cx, b, back + across);
            self.segment(cx, b, back - across);
        }
    }
}

/// Draws the dataflow graph and reports a click on a node box as
/// [`DvizGraphAction::Selected`].
#[derive(Script, ScriptHook, Widget)]
pub struct DvizGraph {
    #[deref]
    view: View,
    #[live]
    draw_box: DrawGraphBox,
    #[live]
    draw_line: DrawGraphLine,
    #[live]
    draw_text: DrawText,
    #[rust]
    nodes: Vec<GraphNode>,
    #[rust]
    edges: Vec<GraphEdge>,
    #[rust]
    selected: Option<String>,
    /// 0 dark, 1 light.
    #[rust]
    light: f32,
    /// The layout last drawn, in the widget's own coordinates; clicks are
    /// matched against it.
    #[rust]
    drawn: GraphLayout,
}

impl Widget for DvizGraph {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
        let Hit::FingerUp(fe) = event.hits(cx, self.view.area()) else {
            return;
        };
        if !fe.is_over {
            return;
        }
        let origin = self.view.area().rect(cx).pos;
        if let Some(id) = hit_test(&self.drawn, fe.abs - origin) {
            if self.selected.as_deref() != Some(id.as_str()) {
                self.selected = Some(id.clone());
                self.view.redraw(cx);
            }
            cx.widget_action(self.widget_uid(), DvizGraphAction::Selected(id));
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        if !self.view.visible() {
            return DrawStep::done();
        }
        while self.view.draw_walk(cx, scope, walk).step().is_some() {}
        let rect = self.view.area().rect(cx);
        self.drawn = layout(&self.nodes, &self.edges, (rect.size.x, rect.size.y));
        let t = pal::Th { l: self.light };
        if self.drawn.boxes.is_empty() {
            self.draw_text.color = t.dim();
            self.draw_text
                .draw_abs(cx, rect.pos + dvec2(PAD, PAD), "No nodes");
            return DrawStep::done();
        }

        self.draw_line.color = t.faint();
        for &(a, b) in &self.drawn.links {
            self.draw_line.link(cx, rect.pos + a, rect.pos + b);
        }

        let mut status = HashMap::new();
        for node in &self.nodes {
            status.entry(node.id.as_str()).or_insert(node.status);
        }
        for (id, at) in &self.drawn.boxes {
            let tone = status_color(&t, status.get(id.as_str()).copied().unwrap_or_default());
            let selected = self.selected.as_deref() == Some(id.as_str());
            self.draw_box.box_fill = pal::mixv(t.deep(), tone, 0.16);
            self.draw_box.box_edge = if selected { t.vio() } else { tone };
            self.draw_box.edge_width = if selected { 2.0 } else { 1.0 };
            self.draw_box.draw_abs(cx, at.translate(rect.pos));
        }

        // Labels only where they can be read.
        let font = self.draw_text.text_style.font_size as f64;
        self.draw_text.color = t.ink();
        for (id, at) in &self.drawn.boxes {
            let chars = ((at.size.x - 2.0 * TEXT_INSET) / (font * 0.6)).floor();
            if at.size.y < font * 1.4 || chars < 1.0 {
                continue;
            }
            let label = fit_label(id, chars as usize);
            let pos = rect.pos + at.pos + dvec2(TEXT_INSET, (at.size.y - font * 1.3) / 2.0);
            self.draw_text.draw_abs(cx, pos, &label);
        }
        DrawStep::done()
    }
}

impl DvizGraphRef {
    /// Shows `nodes` and `edges` with `selected` outlined, in the given theme
    /// (`light` 0 dark, 1 light). Redraws only when something drawn changed.
    pub fn set(
        &self,
        cx: &mut Cx,
        nodes: &[GraphNode],
        edges: &[GraphEdge],
        selected: Option<&str>,
        light: f32,
    ) {
        let Some(mut graph) = self.borrow_mut() else {
            return;
        };
        if same_nodes(&graph.nodes, nodes)
            && same_edges(&graph.edges, edges)
            && graph.selected.as_deref() == selected
            && graph.light == light
        {
            return;
        }
        graph.nodes = nodes.to_vec();
        graph.edges = edges.to_vec();
        graph.selected = selected.map(str::to_string);
        graph.light = light;
        graph.view.redraw(cx);
    }

    /// The node whose box was clicked, if `actions` hold such a click.
    pub fn clicked_node(&self, actions: &Actions) -> Option<String> {
        match actions.find_widget_action(self.widget_uid())?.cast() {
            DvizGraphAction::Selected(id) => Some(id),
            DvizGraphAction::None => None,
        }
    }

    /// The ids of the nodes shown, in the order given.
    pub fn node_ids(&self) -> Vec<String> {
        self.borrow()
            .map(|graph| graph.nodes.iter().map(|node| node.id.clone()).collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dviz_protocol::GraphNodeStatus;

    fn node(id: &str) -> GraphNode {
        GraphNode {
            id: id.into(),
            status: GraphNodeStatus::Active,
            last_seen: 0.0,
        }
    }

    fn edge(from: &str, to: &str) -> GraphEdge {
        GraphEdge {
            from_node: from.into(),
            from_port: "out".into(),
            to_node: to.into(),
            to_port: "in".into(),
        }
    }

    fn nodes(ids: &[&str]) -> Vec<GraphNode> {
        ids.iter().map(|id| node(id)).collect()
    }

    fn rect_of<'a>(layout: &'a GraphLayout, id: &str) -> &'a Rect {
        &layout
            .boxes
            .iter()
            .find(|(box_id, _)| box_id == id)
            .unwrap_or_else(|| panic!("no box for {id}: {layout:?}"))
            .1
    }

    /// Every box is inside `size` and no two boxes overlap.
    fn assert_fits_without_overlap(layout: &GraphLayout, size: (f64, f64)) {
        let area = Rect {
            pos: dvec2(0.0, 0.0),
            size: dvec2(size.0, size.1),
        };
        for (i, (id, rect)) in layout.boxes.iter().enumerate() {
            assert!(rect.is_inside_of(area), "{id} {rect:?} is outside {size:?}");
            for (other, other_rect) in &layout.boxes[i + 1..] {
                assert!(
                    !rect.intersects(*other_rect),
                    "{id} {rect:?} overlaps {other} {other_rect:?}"
                );
            }
        }
    }

    #[test]
    fn layout_is_deterministic_and_non_overlapping() {
        // Given out of order on purpose: the chain is a → b → c → d → e.
        let nodes = nodes(&["c", "a", "e", "b", "d"]);
        let edges = [
            edge("a", "b"),
            edge("b", "c"),
            edge("c", "d"),
            edge("d", "e"),
        ];
        let size = (800.0, 300.0);
        let first = layout(&nodes, &edges, size);
        let second = layout(&nodes, &edges, size);
        assert_eq!(first, second);
        assert_eq!(first.boxes.len(), 5);
        assert_eq!(first.links.len(), 4);
        assert_fits_without_overlap(&first, size);
        // A chain reads left to right.
        let x = |id| rect_of(&first, id).pos.x;
        assert!(x("a") < x("b") && x("b") < x("c") && x("c") < x("d") && x("d") < x("e"));
    }

    #[test]
    fn sources_are_left_of_sinks() {
        let layout = layout(&nodes(&["b", "a"]), &[edge("a", "b")], (400.0, 200.0));
        let (a, b) = (rect_of(&layout, "a"), rect_of(&layout, "b"));
        assert!(a.pos.x + a.size.x < b.pos.x, "{a:?} {b:?}");
        // The link runs from a's right edge to b's left edge.
        let (from, to) = layout.links[0];
        assert_eq!(from, dvec2(a.pos.x + a.size.x, a.pos.y + a.size.y / 2.0));
        assert_eq!(to, dvec2(b.pos.x, b.pos.y + b.size.y / 2.0));
    }

    #[test]
    fn hit_test_finds_the_node_under_the_point() {
        let layout = layout(
            &nodes(&["camera", "detector"]),
            &[edge("camera", "detector")],
            (500.0, 200.0),
        );
        for (id, rect) in &layout.boxes {
            assert_eq!(
                hit_test(&layout, rect.center()).as_deref(),
                Some(id.as_str())
            );
        }
        assert_eq!(hit_test(&layout, dvec2(1.0, 1.0)), None);
        assert_eq!(hit_test(&layout, dvec2(-5.0, 100.0)), None);
        let camera = rect_of(&layout, "camera");
        let right_of_camera = dvec2(camera.pos.x + camera.size.x + 1.0, camera.center().y);
        assert_eq!(hit_test(&layout, right_of_camera), None);
    }

    #[test]
    fn a_node_is_one_layer_right_of_its_furthest_input() {
        // a → b → c and a → c: c sits right of b, not beside it.
        let layout = layout(
            &nodes(&["a", "b", "c"]),
            &[edge("a", "b"), edge("b", "c"), edge("a", "c")],
            (600.0, 200.0),
        );
        let x = |id| rect_of(&layout, id).pos.x;
        assert!(x("a") < x("b") && x("b") < x("c"));
        // Nodes without inputs share the first layer.
        let layout = layout_of(&["s1", "s2", "sink"], &[("s1", "sink"), ("s2", "sink")]);
        assert_eq!(rect_of(&layout, "s1").pos.x, rect_of(&layout, "s2").pos.x);
    }

    fn layout_of(ids: &[&str], links: &[(&str, &str)]) -> GraphLayout {
        let edges: Vec<_> = links.iter().map(|(from, to)| edge(from, to)).collect();
        layout(&nodes(ids), &edges, (600.0, 300.0))
    }

    #[test]
    fn a_cycle_fed_by_a_source_stays_right_of_the_source() {
        // a → b → c → b: b and c are downstream of a whatever the loop.
        let layout = layout_of(&["c", "b", "a"], &[("a", "b"), ("b", "c"), ("c", "b")]);
        let x = |id| rect_of(&layout, id).pos.x;
        assert!(x("a") < x("b") && x("b") < x("c"), "{layout:?}");
    }

    #[test]
    fn odd_graphs_lay_out_without_panicking() {
        let size = (600.0, 300.0);
        // Empty.
        assert_eq!(layout(&[], &[], size), GraphLayout::default());
        // Edges only, all to unknown nodes: nothing to draw.
        assert_eq!(
            layout(&[], &[edge("ghost", "phantom")], size),
            GraphLayout::default()
        );
        // A closed loop with no source, a self-loop, an edge from an unknown
        // node (a dora timer), a duplicate node and a repeated edge.
        let mut odd = nodes(&["x", "y", "z", "x"]);
        odd.push(node("lonely"));
        let edges = [
            edge("x", "y"),
            edge("y", "x"),
            edge("z", "z"),
            edge("dora", "z"),
            edge("x", "y"),
        ];
        let drawn = layout(&odd, &edges, size);
        let ids: Vec<&str> = drawn.boxes.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, ["x", "y", "z", "lonely"]);
        assert_eq!(drawn.links.len(), 2, "x→y and y→x only: {drawn:?}");
        assert_fits_without_overlap(&drawn, size);
        // Tiny, zero and non-finite sizes give empty boxes, never a panic.
        for size in [
            (0.0, 0.0),
            (5.0, 5.0),
            (f64::NAN, 100.0),
            (f64::INFINITY, -1.0),
        ] {
            let drawn = layout(&odd, &edges, size);
            assert_eq!(drawn.boxes.len(), 4);
            assert_eq!(hit_test(&drawn, dvec2(0.0, 0.0)), None);
        }
    }

    /// The box and line shaders compile for the desktop backends (no GPU is
    /// needed: this is the fork's own shader compiler).
    #[test]
    fn graph_shaders_compile() {
        let mut cx = Cx::new(Box::new(|_, _| {}));
        cx.init_cx_os();
        cx.with_vm(|vm| {
            dorobot_studio_core::register_ui(vm);
            script_mod(vm);
            assert!(vm.take_errors().is_empty());
            let sources = [
                ("box glsl", script_eval!(vm, {mod.shader.test_compile_draw_source(mod.widgets.DvizGraph.draw_box, "glsl", false)})),
                ("box metal", script_eval!(vm, {mod.shader.test_compile_draw_source(mod.widgets.DvizGraph.draw_box, "metal", false)})),
                ("line glsl", script_eval!(vm, {mod.shader.test_compile_draw_source(mod.widgets.DvizGraph.draw_line, "glsl", false)})),
                ("line metal", script_eval!(vm, {mod.shader.test_compile_draw_source(mod.widgets.DvizGraph.draw_line, "metal", false)})),
            ];
            for (name, value) in sources {
                let source = vm
                    .heap()
                    .string_with(value, |_, text| text.to_string())
                    .unwrap_or_else(|| panic!("{name}: no source"));
                assert!(
                    !source.is_empty() && !source.starts_with("ERRORS"),
                    "{name}: {source}"
                );
            }
            assert!(vm.take_errors().is_empty());
        });
    }

    #[test]
    fn long_labels_are_cut_with_an_ellipsis() {
        assert_eq!(fit_label("planner", 7), "planner");
        assert_eq!(fit_label("planner", 5), "plan…");
        assert_eq!(fit_label("ünïcode", 3), "ün…");
        assert_eq!(fit_label("planner", 1), "…");
        assert_eq!(fit_label("planner", 0), "");
    }

    #[test]
    fn many_nodes_still_fit() {
        let ids: Vec<String> = (0..300).map(|i| format!("n{i:03}")).collect();
        let nodes: Vec<_> = ids.iter().map(|id| node(id)).collect();
        // Thirty chains of ten.
        let edges: Vec<_> = (0..300)
            .filter(|i| i % 10 != 9)
            .map(|i| edge(&ids[i], &ids[i + 1]))
            .collect();
        let size = (1200.0, 700.0);
        let layout = layout(&nodes, &edges, size);
        assert_eq!(layout.boxes.len(), 300);
        assert_eq!(layout.links.len(), 270);
        assert_fits_without_overlap(&layout, size);
    }
}
