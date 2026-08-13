//! Factory-industrial design tokens and shared DSL prototypes.
//! Palette is the frozen web mockup's: warm charcoal / bone, signal orange,
//! steel cyan; sharp corners everywhere. `light` = 0 dark, 1 light on every
//! shader — one float pushed through the tree switches the theme.

use makepad_widgets::*;

/// Rust-side palette mirror for values pushed via script_apply_eval.
pub mod pal {
    use makepad_widgets::*;
    pub fn mixv(a: Vec4f, b: Vec4f, t: f32) -> Vec4f {
        a + (b - a) * t
    }
    pub const fn rgb(r: u8, g: u8, b: u8) -> Vec4f {
        Vec4f { x: r as f32 / 255.0, y: g as f32 / 255.0, z: b as f32 / 255.0, w: 1.0 }
    }
    // dark, light pairs
    pub const VOID_D: Vec4f = rgb(0x0A, 0x09, 0x08);
    pub const VOID_L: Vec4f = rgb(0xF2, 0xF0, 0xED);
    pub const DEEP_D: Vec4f = rgb(0x14, 0x13, 0x12);
    pub const DEEP_L: Vec4f = rgb(0xFB, 0xFA, 0xF8);
    pub const LIFT_D: Vec4f = rgb(0x1B, 0x19, 0x17);
    pub const LIFT_L: Vec4f = rgb(0xFF, 0xFF, 0xFF);
    pub const SINK_D: Vec4f = rgb(0x06, 0x05, 0x05);
    pub const SINK_L: Vec4f = rgb(0xE8, 0xE5, 0xE1);
    pub const EDGE_D: Vec4f = rgb(0x2A, 0x27, 0x25);
    pub const EDGE_L: Vec4f = rgb(0xD8, 0xD4, 0xCF);
    pub const INK_D: Vec4f = rgb(0xF2, 0xF0, 0xEC);
    pub const INK_L: Vec4f = rgb(0x16, 0x14, 0x13);
    pub const INK2_D: Vec4f = rgb(0xCF, 0xC9, 0xC2);
    pub const INK2_L: Vec4f = rgb(0x3E, 0x39, 0x35);
    pub const DIM_D: Vec4f = rgb(0x94, 0x87, 0x81);
    pub const DIM_L: Vec4f = rgb(0x5E, 0x56, 0x4E);
    pub const FAINT_D: Vec4f = rgb(0x6B, 0x62, 0x5B);
    pub const FAINT_L: Vec4f = rgb(0x7A, 0x71, 0x69);
    pub const VIO_D: Vec4f = rgb(0xEF, 0x6F, 0x2E);
    pub const VIO_L: Vec4f = rgb(0xD1, 0x50, 0x10);
    pub const VIOG_D: Vec4f = rgb(0x2A, 0x17, 0x08);
    pub const VIOG_L: Vec4f = rgb(0xF7, 0xE4, 0xD8);
    pub const CY_D: Vec4f = rgb(0x6F, 0xA8, 0xB8);
    pub const CY_L: Vec4f = rgb(0x3E, 0x7A, 0x8C);
    pub const OK_D: Vec4f = rgb(0x6F, 0xAB, 0x78);
    pub const OK_L: Vec4f = rgb(0x3E, 0x7A, 0x4A);
    pub const HOT_D: Vec4f = rgb(0xE5, 0x40, 0x48);
    pub const HOT_L: Vec4f = rgb(0xC4, 0x3B, 0x36);
    pub const AMB_D: Vec4f = rgb(0xF0, 0xA3, 0x30);
    pub const AMB_L: Vec4f = rgb(0xB0, 0x75, 0x14);

    // ----------------------------------------------------------- data series --
    // Categorical colours for plot series. Deliberately NOT the UI accents
    // above, which is a computed result rather than a preference: measured in
    // OKLCH, CY's chroma is 0.064 — below the 0.10 floor at which a mark stops
    // reading as a colour and reads as grey — and VIO, AMB and OK all sit above
    // the dark lightness band (0.686–0.773 against a 0.67 ceiling). A UI accent
    // is judged against a surface it sits on; a series colour is judged against
    // the other series. These share the accents' hue angles and are stepped per
    // mode so they pass.
    //
    // FIVE SLOTS IS THE CEILING, and that is measured too. Checking every pair
    // (not just adjacent ones — a time-series plot shows all series at once, so
    // any two may be compared) under protan and deutan simulation: six hues
    // pass on dark but fail the normal-vision floor on light by 0.1 (14.9 vs
    // 15.0), and seven fail both (13.9). A sixth series therefore gets a dash
    // pattern or its own facet — never a sixth hue. Cycling hues past this
    // point produces a chart that cannot be read, only decorated.
    //
    // Validated all-pairs in both modes: worst CVD separation 8.6, worst
    // normal-vision 19.2, contrast >= 3.0 against #141312 and #FBFAF8.
    pub const S1_D: Vec4f = rgb(0x01, 0xA2, 0xC5); // cyan   217°
    pub const S1_L: Vec4f = rgb(0x00, 0x98, 0xB9);
    pub const S2_D: Vec4f = rgb(0xAB, 0x3D, 0x09); // orange  44°
    pub const S2_L: Vec4f = rgb(0x9A, 0x34, 0x04);
    pub const S3_D: Vec4f = rgb(0xDF, 0x67, 0x91); // rose     0°
    pub const S3_L: Vec4f = rgb(0xE1, 0x5E, 0x8E);
    pub const S4_D: Vec4f = rgb(0xB5, 0x90, 0x0A); // gold    90°
    pub const S4_L: Vec4f = rgb(0xB2, 0x8D, 0x00);
    pub const S5_D: Vec4f = rgb(0x75, 0x4C, 0xB0); // violet 300°
    pub const S5_L: Vec4f = rgb(0x60, 0x30, 0x9B);

    /// How many series may be told apart by colour alone. Past this, encode
    /// with a dash pattern or facet into small multiples.
    pub const SERIES_MAX: usize = 5;

    pub struct Th {
        pub l: f32,
    }
    impl Th {
        pub fn void(&self) -> Vec4f { mixv(VOID_D, VOID_L, self.l) }
        pub fn deep(&self) -> Vec4f { mixv(DEEP_D, DEEP_L, self.l) }
        pub fn lift(&self) -> Vec4f { mixv(LIFT_D, LIFT_L, self.l) }
        pub fn sink(&self) -> Vec4f { mixv(SINK_D, SINK_L, self.l) }
        pub fn edge(&self) -> Vec4f { mixv(EDGE_D, EDGE_L, self.l) }
        pub fn ink(&self) -> Vec4f { mixv(INK_D, INK_L, self.l) }
        pub fn ink2(&self) -> Vec4f { mixv(INK2_D, INK2_L, self.l) }
        pub fn dim(&self) -> Vec4f { mixv(DIM_D, DIM_L, self.l) }
        pub fn faint(&self) -> Vec4f { mixv(FAINT_D, FAINT_L, self.l) }
        pub fn vio(&self) -> Vec4f { mixv(VIO_D, VIO_L, self.l) }
        pub fn viog(&self) -> Vec4f { mixv(VIOG_D, VIOG_L, self.l) }
        pub fn cy(&self) -> Vec4f { mixv(CY_D, CY_L, self.l) }
        pub fn ok(&self) -> Vec4f { mixv(OK_D, OK_L, self.l) }
        pub fn hot(&self) -> Vec4f { mixv(HOT_D, HOT_L, self.l) }
        pub fn amb(&self) -> Vec4f { mixv(AMB_D, AMB_L, self.l) }

        /// Categorical series colour by index, theme-stepped. Indices past
        /// [`SERIES_MAX`] wrap, which is a bug at the call site rather than
        /// here: two series would share a hue with nothing to separate them.
        /// Use [`Th::series_needs_dash`] to encode the overflow instead.
        pub fn series(&self, i: usize) -> Vec4f {
            let (d, l) = match i % SERIES_MAX {
                0 => (S1_D, S1_L),
                1 => (S2_D, S2_L),
                2 => (S3_D, S3_L),
                3 => (S4_D, S4_L),
                _ => (S5_D, S5_L),
            };
            mixv(d, l, self.l)
        }

        /// True when series `i` reuses an earlier hue and must carry a second
        /// channel of identity — a dash pattern — to stay readable.
        pub fn series_needs_dash(&self, i: usize) -> bool {
            i >= SERIES_MAX
        }
    }
}

script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    use mod.text.*
    use mod.res.*

    mod.widgets.nx = {}

    // ----------------------------------------------------------------- type --
    mod.widgets.nx.FONT_R = TextStyle{
        font_family: FontFamily{
            latin := FontMember{res: crate_resource("self:resources/fonts/Roboto-Regular.ttf") asc: 0.0 desc: 0.0}
            chinese := FontMember{res: crate_resource("self:resources/fonts/NotoSansSC-Regular.ttf") asc: 0.0 desc: 0.0}
            symbols := FontMember{res: crate_resource("self:resources/fonts/DejaVuSans.ttf") asc: 0.0 desc: 0.0}
        }
        line_spacing: 1.4
    }
    mod.widgets.nx.FONT_M = TextStyle{
        font_family: FontFamily{
            latin := FontMember{res: crate_resource("self:resources/fonts/Roboto-Medium.ttf") asc: 0.0 desc: 0.0}
            chinese := FontMember{res: crate_resource("self:resources/fonts/NotoSansSC-Regular.ttf") asc: 0.0 desc: 0.0}
            symbols := FontMember{res: crate_resource("self:resources/fonts/DejaVuSans.ttf") asc: 0.0 desc: 0.0}
        }
        line_spacing: 1.4
    }
    mod.widgets.nx.FONT_B = TextStyle{
        font_family: FontFamily{
            latin := FontMember{res: crate_resource("self:resources/fonts/Roboto-Bold.ttf") asc: 0.0 desc: 0.0}
            chinese := FontMember{res: crate_resource("self:resources/fonts/NotoSansSC-Regular.ttf") asc: 0.0 desc: 0.0}
            symbols := FontMember{res: crate_resource("self:resources/fonts/DejaVuSans.ttf") asc: 0.0 desc: 0.0}
        }
        line_spacing: 1.4
    }
    mod.widgets.nx.FONT_MONO = TextStyle{
        font_family: FontFamily{
            latin := FontMember{res: crate_resource("self:resources/fonts/RobotoMono-Regular.ttf") asc: 0.0 desc: 0.0}
            chinese := FontMember{res: crate_resource("self:resources/fonts/NotoSansSC-Regular.ttf") asc: 0.0 desc: 0.0}
            symbols := FontMember{res: crate_resource("self:resources/fonts/DejaVuSans.ttf") asc: 0.0 desc: 0.0}
        }
        line_spacing: 1.35
    }

    mod.widgets.nx.T_TITLE = mod.widgets.nx.FONT_M{font_size: 13.0}
    mod.widgets.nx.T_BODY  = mod.widgets.nx.FONT_R{font_size: 10.5}
    mod.widgets.nx.T_META  = mod.widgets.nx.FONT_R{font_size: 9.5}
    mod.widgets.nx.T_CAP   = mod.widgets.nx.FONT_M{font_size: 8.5}
    mod.widgets.nx.T_MONO  = mod.widgets.nx.FONT_MONO{font_size: 10.0}
    mod.widgets.nx.T_MONO_S= mod.widgets.nx.FONT_MONO{font_size: 8.5}
    mod.widgets.nx.T_BIG   = mod.widgets.nx.FONT_M{font_size: 20.0}

    // ------------------------------------------------------- makepad fixes --
    // Two defects in makepad's own widgets, both verified still present on the
    // dev branch (d0fe5f2b, 2026-08-13) — so they are worked around here once,
    // for every app that draws with these tokens, rather than rediscovered.
    //
    // 1. `TextInput` declares no `visible` property, and `Widget::set_visible`
    //    has an empty default body — so hiding a bare input compiles, runs and
    //    does nothing at all. The wrapper is a View, which does honour it.
    // 2. Its `empty_text` defaults to the literal string "Your text here"
    //    (widgets/src/text_input.rs:43), which ships to users as UI copy unless
    //    every site overrides it. Blank here; give it a real hint per field.
    //
    // Toggle `nx.Field` itself, and read/write the `input` inside it.
    mod.widgets.nx.Field = View{
        width: Fill height: Fit
        visible: false
        input := TextInput{ width: Fill text: "" empty_text: "" }
    }

    // Series colours for DSL-side marks. Kept as explicit dark/light pairs
    // because a shader mixes them itself with `light`; see pal::Th::series for
    // the Rust mirror and the note there on why five is the ceiling.
    mod.widgets.nx.S1_D = #x01A2C5
    mod.widgets.nx.S1_L = #x0098B9
    mod.widgets.nx.S2_D = #xAB3D09
    mod.widgets.nx.S2_L = #x9A3404
    mod.widgets.nx.S3_D = #xDF6791
    mod.widgets.nx.S3_L = #xE15E8E
    mod.widgets.nx.S4_D = #xB5900A
    mod.widgets.nx.S4_L = #xB28D00
    mod.widgets.nx.S5_D = #x754CB0
    mod.widgets.nx.S5_L = #x60309B

    // -------------------------------------------------------------- surfaces --
    // Root ground.
    mod.widgets.nx.Ground = View{
        width: Fill height: Fill
        show_bg: true
        draw_bg +: {
            light: instance(0.0)
            pixel: fn() { return mix(#x0A0908, #xF2F0ED, self.light) }
        }
    }

    // Rail / panel surface with a hairline outer border, sharp corners.
    mod.widgets.nx.Panel = View{
        width: Fill height: Fill
        flow: Down
        show_bg: true
        draw_bg +: {
            light: instance(0.0)
            pixel: fn() {
                let base = mix(#x141312, #xFBFAF8, self.light)
                let edge = mix(#x2A2725, #xD8D4CF, self.light)
                let bx = self.pos.x * self.rect_size.x
                let by = self.pos.y * self.rect_size.y
                let on_edge = max(
                    max(step(bx, 1.0), step(self.rect_size.x - 1.0, bx)),
                    max(step(by, 1.0), step(self.rect_size.y - 1.0, by)))
                return mix(base, edge, on_edge)
            }
        }
    }

    // Small uppercase section caption (uppercase applied in Rust).
    mod.widgets.nx.Cap = Label{
        text: "CAP"
        draw_text +: {
            light: instance(0.0)
            text_style: mod.widgets.nx.T_CAP{}
            get_color: fn() { return mix(#x948781, #x5E564E, self.light) }
        }
    }

    mod.widgets.nx.BodyLbl = Label{
        width: Fill
        text: ""
        draw_text +: {
            light: instance(0.0)
            wrap: Words
            text_style: mod.widgets.nx.T_META{}
            get_color: fn() { return mix(#x948781, #x5E564E, self.light) }
        }
    }

    mod.widgets.nx.InkLbl = Label{
        text: ""
        draw_text +: {
            light: instance(0.0)
            text_style: mod.widgets.nx.T_BODY{}
            get_color: fn() { return mix(#xF2F0EC, #x161413, self.light) }
        }
    }

    mod.widgets.nx.MonoLbl = Label{
        text: ""
        draw_text +: {
            light: instance(0.0)
            text_style: mod.widgets.nx.T_MONO_S{}
            get_color: fn() { return mix(#x948781, #x5E564E, self.light) }
        }
    }

    // --------------------------------------------------------------- buttons --
    // Standard button: transparent fill, hairline edge; hover/active → orange.
    mod.widgets.nx.NxBtn = Button{
        width: Fit height: Fit
        padding: Inset{left: 11. right: 11. top: 4. bottom: 4.}
        draw_text +: {
            light: instance(0.0)
            on: instance(0.0)
            text_style: mod.widgets.nx.T_BODY{}
            get_color: fn() {
                let idle = mix(#xCFC9C2, #x3E3935, self.light)
                let vio = mix(#xEF6F2E, #xD15010, self.light)
                let c = mix(idle, vio, max(self.on, self.hover))
                return mix(c, mix(#x6B625B, #x7A7169, self.light), self.disabled)
            }
        }
        draw_bg +: {
            light: instance(0.0)
            on: instance(0.0)
            pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                let fill = mix(#x1B1917, #xFFFFFF, self.light)
                let edge0 = mix(#x2A2725, #xD8D4CF, self.light)
                let vio = mix(#xEF6F2E, #xD15010, self.light)
                let edge = mix(edge0, vio, max(self.on, self.hover))
                sdf.rect(0.5, 0.5, self.rect_size.x - 1.0, self.rect_size.y - 1.0)
                sdf.fill_keep(mix(fill, mix(#x2A1708, #xF7E4D8, self.light), self.on * 0.6))
                sdf.stroke(mix(edge, edge0, self.disabled), 1.0)
                return sdf.result
            }
        }
    }

    // Small inline action button.
    mod.widgets.nx.Mini = mod.widgets.nx.NxBtn{
        padding: Inset{left: 7. right: 7. top: 2. bottom: 2.}
        draw_text +: { text_style: mod.widgets.nx.T_META{} }
    }

    // Destructive variant: red text/edge on hover.
    mod.widgets.nx.MiniHot = mod.widgets.nx.Mini{
        draw_text +: {
            get_color: fn() {
                let idle = mix(#xCFC9C2, #x3E3935, self.light)
                let hot = mix(#xE54048, #xC43B36, self.light)
                return mix(idle, hot, max(self.on, self.hover))
            }
        }
        draw_bg +: {
            pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                let fill = mix(#x1B1917, #xFFFFFF, self.light)
                let edge0 = mix(#x2A2725, #xD8D4CF, self.light)
                let hot = mix(#xE54048, #xC43B36, self.light)
                let edge = mix(edge0, hot, max(self.on, self.hover))
                sdf.rect(0.5, 0.5, self.rect_size.x - 1.0, self.rect_size.y - 1.0)
                sdf.fill_keep(fill)
                sdf.stroke(edge, 1.0)
                return sdf.result
            }
        }
    }

    // Big destructive button (E-STOP).
    mod.widgets.nx.HotBtn = mod.widgets.nx.NxBtn{
        draw_text +: {
            text_style: mod.widgets.nx.FONT_B{font_size: 10.5}
            get_color: fn() {
                let hot = mix(#xE54048, #xC43B36, self.light)
                return hot
            }
        }
        draw_bg +: {
            pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                let fill = mix(#x1B1917, #xFFFFFF, self.light)
                let hot = mix(#xE54048, #xC43B36, self.light)
                sdf.rect(0.5, 0.5, self.rect_size.x - 1.0, self.rect_size.y - 1.0)
                sdf.fill_keep(fill)
                sdf.stroke(hot, 1.0)
                return sdf.result
            }
        }
    }

    // ----------------------------------------------------------------- pills --
    // tone: 0 run(cyan) 1 ok 2 warn 3 bad 4 arch(grey)
    mod.widgets.nx.Pill = View{
        width: Fit height: Fit
        padding: Inset{left: 6. right: 6. top: 1. bottom: 1.}
        show_bg: true
        draw_bg +: {
            light: instance(0.0)
            tone: instance(0.0)
            pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                let c_ok = mix(#x6FA8B8, #x6FAB78, step(0.5, self.tone))
                let c_warn = mix(c_ok, #xF0A330, step(1.5, self.tone))
                let c_bad = mix(c_warn, #xE54048, step(2.5, self.tone))
                let cd = mix(c_bad, #x6B625B, step(3.5, self.tone))
                let l_ok = mix(#x3E7A8C, #x3E7A4A, step(0.5, self.tone))
                let l_warn = mix(l_ok, #xB07514, step(1.5, self.tone))
                let l_bad = mix(l_warn, #xC43B36, step(2.5, self.tone))
                let cl = mix(l_bad, #x7A7169, step(3.5, self.tone))
                let c = mix(cd, cl, self.light)
                sdf.rect(0.5, 0.5, self.rect_size.x - 1.0, self.rect_size.y - 1.0)
                sdf.fill_keep(mix(c, mix(#x141312, #xFBFAF8, self.light), 0.82))
                sdf.stroke(c, 1.0)
                return sdf.result
            }
        }
        lbl := Label{
            text: "pill"
            draw_text +: {
                light: instance(0.0)
                tone: instance(0.0)
                text_style: mod.widgets.nx.T_CAP{}
                get_color: fn() {
                    let c_ok = mix(#x6FA8B8, #x6FAB78, step(0.5, self.tone))
                    let c_warn = mix(c_ok, #xF0A330, step(1.5, self.tone))
                    let c_bad = mix(c_warn, #xE54048, step(2.5, self.tone))
                    let cd = mix(c_bad, #x948781, step(3.5, self.tone))
                    let l_ok = mix(#x3E7A8C, #x3E7A4A, step(0.5, self.tone))
                    let l_warn = mix(l_ok, #xB07514, step(1.5, self.tone))
                    let l_bad = mix(l_warn, #xC43B36, step(2.5, self.tone))
                    let cl = mix(l_bad, #x5E564E, step(3.5, self.tone))
                    return mix(cd, cl, self.light)
                }
            }
        }
    }

    // Chrome chip: bordered, optional pulse dot (dot: 1 shows it).
    mod.widgets.nx.ChipV = View{
        width: Fit height: Fit
        flow: Right
        align: Align{y: 0.5}
        spacing: 5.0
        padding: Inset{left: 8. right: 8. top: 3. bottom: 3.}
        cursor: MouseCursor.Hand
        show_bg: true
        draw_bg +: {
            light: instance(0.0)
            pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                let fill = mix(#x1B1917, #xFFFFFF, self.light)
                let edge = mix(#x2A2725, #xD8D4CF, self.light)
                sdf.rect(0.5, 0.5, self.rect_size.x - 1.0, self.rect_size.y - 1.0)
                sdf.fill_keep(fill)
                sdf.stroke(edge, 1.0)
                return sdf.result
            }
        }
        dot := View{
            width: 7 height: 7
            visible: false
            show_bg: true
            draw_bg +: {
                tone: instance(0.0)
                time_on: instance(0.0)
                pixel: fn() {
                    let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                    let c_run = #x6FA8B8
                    let c_warn = mix(c_run, #xF0A330, step(0.5, self.tone))
                    let c = mix(c_warn, #xE54048, step(1.5, self.tone))
                    sdf.circle(3.5, 3.5, 2.6)
                    sdf.fill(c)
                    return sdf.result
                }
            }
        }
        lbl := Label{
            text: ""
            draw_text +: {
                light: instance(0.0)
                text_style: mod.widgets.nx.T_META{}
                get_color: fn() { return mix(#xCFC9C2, #x3E3935, self.light) }
            }
        }
    }

    // ------------------------------------------------------------------ rows --
    // Selectable list row: name + right-aligned tag; active = orange left edge.
    mod.widgets.nx.NxRow = View{
        width: Fill height: Fit
        flow: Right
        align: Align{y: 0.5}
        padding: Inset{left: 9. right: 7. top: 5. bottom: 5.}
        spacing: 6.0
        cursor: MouseCursor.Hand
        show_bg: true
        draw_bg +: {
            light: instance(0.0)
            on: instance(0.0)
            hover: instance(0.0)
            dimmed: instance(0.0)
            frac: instance(0.0)
            pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                let base = mix(#x141312, #xFBFAF8, self.light)
                let liftc = mix(#x1B1917, #xFFFFFF, self.light)
                let fill = mix(base, liftc, max(self.on, self.hover * 0.6))
                sdf.rect(0.0, 0.0, self.rect_size.x, self.rect_size.y)
                sdf.fill(mix(fill, base, self.dimmed * 0.5))
                let viog = mix(#x2A1708, #xF7E4D8, self.light)
                sdf.rect(0.0, 0.0, self.rect_size.x * self.frac, self.rect_size.y)
                sdf.fill(mix(vec4(0.0,0.0,0.0,0.0), viog, step(0.001, self.frac)))
                let vio = mix(#xEF6F2E, #xD15010, self.light)
                sdf.rect(0.0, 0.0, 2.0, self.rect_size.y)
                sdf.fill(mix(vec4(0.0,0.0,0.0,0.0), vio, self.on))
                return sdf.result
            }
        }
        nm := Label{
            text: ""
            draw_text +: {
                light: instance(0.0)
                dimmed: instance(0.0)
                text_style: mod.widgets.nx.T_BODY{}
                get_color: fn() {
                    let c = mix(#xF2F0EC, #x161413, self.light)
                    let d = mix(#x6B625B, #x7A7169, self.light)
                    return mix(c, d, self.dimmed)
                }
            }
        }
        pill := mod.widgets.nx.Pill{ visible: false }
        Filler{}
        tg := Label{
            text: ""
            draw_text +: {
                light: instance(0.0)
                text_style: mod.widgets.nx.T_MONO_S{}
                get_color: fn() { return mix(#x948781, #x5E564E, self.light) }
            }
        }
    }

    // Group header ("SET · crouch ladder").
    mod.widgets.nx.Grp = View{
        width: Fill height: Fit
        padding: Inset{left: 9. right: 7. top: 8. bottom: 3.}
        show_bg: true
        draw_bg +: {
            light: instance(0.0)
            pixel: fn() {
                let base = mix(#x141312, #xFBFAF8, self.light)
                let hair = mix(#x1F1D1C, #xE2DED9, self.light)
                let t = step(self.rect_size.y - 1.0, self.pos.y * self.rect_size.y)
                return mix(base, hair, t)
            }
        }
        lbl := mod.widgets.nx.Cap{}
    }

    // ----------------------------------------------------------------- tiles --
    mod.widgets.nx.Tile = View{
        width: Fit height: Fit
        flow: Down
        padding: Inset{left: 8. right: 8. top: 4. bottom: 4.}
        spacing: 1.0
        show_bg: true
        draw_bg +: {
            light: instance(0.0)
            pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                let fill = mix(#x1B1917, #xFFFFFF, self.light)
                let edge = mix(#x2A2725, #xD8D4CF, self.light)
                sdf.rect(0.5, 0.5, self.rect_size.x - 1.0, self.rect_size.y - 1.0)
                sdf.fill_keep(fill)
                sdf.stroke(edge, 1.0)
                return sdf.result
            }
        }
        k := mod.widgets.nx.Cap{ text: "K" }
        v := Label{
            text: "—"
            draw_text +: {
                light: instance(0.0)
                accent: instance(0.0) // 0 ink, 1 cyan, 2 orange
                text_style: mod.widgets.nx.T_MONO{}
                get_color: fn() {
                    let ink = mix(#xF2F0EC, #x161413, self.light)
                    let cy = mix(#x6FA8B8, #x3E7A8C, self.light)
                    let vio = mix(#xEF6F2E, #xD15010, self.light)
                    let a = mix(ink, cy, step(0.5, self.accent))
                    return mix(a, vio, step(1.5, self.accent))
                }
            }
        }
    }

    // -------------------------------------------------------------- verdicts --
    // tone: 0 neutral 1 warn 2 bad
    mod.widgets.nx.Verdict = View{
        width: Fill height: Fit
        padding: Inset{left: 10. right: 10. top: 7. bottom: 7.}
        show_bg: true
        draw_bg +: {
            light: instance(0.0)
            tone: instance(0.0)
            pixel: fn() {
                let sdf = Sdf2d.viewport(self.pos * self.rect_size)
                let fill = mix(#x1B1917, #xFFFFFF, self.light)
                let n = mix(#x6FA8B8, #x3E7A8C, self.light)
                let w = mix(#xF0A330, #xB07514, self.light)
                let b = mix(#xE54048, #xC43B36, self.light)
                let edge = mix(mix(n, w, step(0.5, self.tone)), b, step(1.5, self.tone))
                sdf.rect(0.0, 0.0, self.rect_size.x, self.rect_size.y)
                sdf.fill(fill)
                sdf.rect(0.0, 0.0, 2.0, self.rect_size.y)
                sdf.fill(edge)
                return sdf.result
            }
        }
        lbl := Label{
            width: Fill
            text: ""
            draw_text +: {
                light: instance(0.0)
                wrap: Words
                text_style: mod.widgets.nx.T_META{}
                get_color: fn() { return mix(#xCFC9C2, #x3E3935, self.light) }
            }
        }
    }

    // Check row: state icon + name + detail. st: 0 pass 1 warn/run 2 fail 3 idle
    mod.widgets.nx.ChkRow = View{
        width: Fill height: Fit
        flow: Right
        spacing: 7.0
        padding: Inset{left: 2. top: 4. bottom: 4. right: 2.}
        ico := Label{
            text: "✓"
            draw_text +: {
                light: instance(0.0)
                st: instance(0.0)
                text_style: mod.widgets.nx.T_BODY{}
                get_color: fn() {
                    let p = mix(#x6FAB78, #x3E7A4A, self.light)
                    let w = mix(#xF0A330, #xB07514, self.light)
                    let f = mix(#xE54048, #xC43B36, self.light)
                    let idle = mix(#x6B625B, #x7A7169, self.light)
                    let c = mix(mix(p, w, step(0.5, self.st)), f, step(1.5, self.st))
                    return mix(c, idle, step(2.5, self.st))
                }
            }
        }
        body := View{
            width: Fill height: Fit
            flow: Down
            spacing: 1.0
            nm := mod.widgets.nx.InkLbl{}
            ds := mod.widgets.nx.BodyLbl{}
        }
        act := mod.widgets.nx.Mini{ visible: false text: "run" }
        act2 := mod.widgets.nx.Mini{ visible: false text: "cancel" }
    }
}

pub fn script_mod_tokens(vm: &mut ScriptVm) -> ScriptValue {
    script_mod(vm)
}

#[cfg(test)]
mod tests {
    use super::pal::*;

    /// The ceiling is a measured property of the palette, not a preference, so
    /// it gets a test: raising SERIES_MAX without re-running the validator
    /// would silently ship a sixth hue that fails the normal-vision floor on
    /// light (14.9 against a floor of 15.0, measured all-pairs).
    #[test]
    fn five_series_slots_and_no_more() {
        assert_eq!(SERIES_MAX, 5);
    }

    #[test]
    fn series_steps_are_distinct_in_both_themes() {
        for l in [0.0, 1.0] {
            let th = Th { l };
            let cols: Vec<[f32; 3]> = (0..SERIES_MAX)
                .map(|i| {
                    let c = th.series(i);
                    [c.x, c.y, c.z]
                })
                .collect();
            for i in 0..cols.len() {
                for j in i + 1..cols.len() {
                    assert_ne!(cols[i], cols[j], "series {i} and {j} collide at light={l}");
                }
            }
        }
    }

    /// Past the ceiling a hue repeats, and the caller must be told so it can
    /// add the dash — a silent wrap is two unreadable series.
    #[test]
    fn overflow_wraps_and_is_flagged() {
        let th = Th { l: 0.0 };
        assert_eq!(th.series(SERIES_MAX).x, th.series(0).x);
        assert!(!th.series_needs_dash(SERIES_MAX - 1));
        assert!(th.series_needs_dash(SERIES_MAX));
    }

    #[test]
    fn theme_float_picks_the_ends() {
        assert_eq!(Th { l: 0.0 }.void().x, VOID_D.x);
        assert_eq!(Th { l: 1.0 }.void().x, VOID_L.x);
        // and interpolates between them rather than snapping
        let mid = Th { l: 0.5 }.void().x;
        assert!(mid > VOID_D.x && mid < VOID_L.x);
    }
}
