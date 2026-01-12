# URDF Visualization Integration Plan

> Analysis and implementation checklist for integrating URDF robot visualization into DoRobot using kiss3d

**Date:** January 2026
**Status:** Planning

---

## Table of Contents

1. [Executive Summary](#executive-summary)
2. [Technology Analysis](#technology-analysis)
3. [Integration Options](#integration-options)
4. [Chosen Approach](#chosen-approach)
5. [Architecture](#architecture)
6. [Implementation Checklist](#implementation-checklist)
7. [References](#references)

---

## Executive Summary

### Goal
Add URDF robot model visualization to DoRobot, synchronized with LeRobot dataset playback timeline.

### Chosen Approach
**Option 3: Texture Streaming** - kiss3d renders 3D scene to offscreen texture, Makepad displays it via Image widget with mouse event forwarding for camera control.

### Why This Approach
- Proven pattern (kiss3d-egui uses same architecture)
- Minimal coupling between rendering systems
- No Makepad refactoring required
- Can upgrade to shared wgpu later if needed

---

## Technology Analysis

### kiss3d Architecture

| Component | Description |
|-----------|-------------|
| **Graphics Backend** | wgpu (WebGPU) - modern, cross-platform |
| **Shader Language** | WGSL (runtime compilation) |
| **Scene Graph** | `Rc<RefCell<SceneNode3d>>` + `Weak<>` hierarchy |
| **Camera** | ArcBall (orbit), FirstPerson, Fixed |
| **Materials** | Cook-Torrance PBR with textures |
| **Primitives** | Box, Cylinder, Sphere, Cone (procedural) |

### kiss3d Render Pipeline

```
acquire_surface_texture()
    │
    ▼
[CLEAR PASS] - clear_color + clear_depth
    │
    ▼
[3D RENDER]
├── scene.prepare()     // hierarchical transform updates
├── material.flush()    // batch GPU uploads
└── scene.render()      // draw calls
    │
    ▼
[2D OVERLAY] - surfaces, polylines, points
    │
    ▼
[EGUI RENDER] - UI overlay (if enabled)
    │
    ▼
queue.submit() → present()
```

### kiss3d-egui Integration Pattern

kiss3d embeds egui using this pattern:
1. **Separate Renderers** - egui has its own `EguiRenderer` with wgpu pipeline
2. **Event Forwarding** - `feed_egui_event()` converts window events to egui `RawInput`
3. **Frame Lifecycle** - `begin_frame()` → UI closure → `end_frame()` → `render()`
4. **Input Capture** - `wants_pointer_input()` / `wants_keyboard_input()` for event routing

```rust
// kiss3d-egui usage pattern
while window.render_3d(&mut camera, &state).await {
    window.draw_ui(|ctx| {
        egui::Window::new("Controls").show(ctx, |ui| {
            ui.add(egui::Slider::new(&mut speed, 0.0..=0.1));
        });
    });
}
```

### urdf-viz Architecture

| Component | Library |
|-----------|---------|
| URDF Parsing | `urdf-rs` |
| 3D Rendering | `kiss3d` |
| Mesh Loading | `mesh-loader` (STL, OBJ, DAE) |
| Math | `nalgebra` |

**Key Function:** `add_geometry()` creates kiss3d SceneNodes for URDF links:
- Primitives: Box, Cylinder, Sphere → procedural geometry
- Meshes: STL/OBJ/DAE files → loaded via mesh-loader

---

## Integration Options

### Option 1: kiss3d as Embedded Renderer
- Manage two render contexts within Makepad
- Medium complexity, some coupling
- **Effort:** 2-3 weeks

### Option 2: Shared wgpu Context
- Both kiss3d and Makepad share GPU device/queue
- High complexity, deep integration
- **Effort:** 4-6 weeks

### Option 3: Texture Streaming (CHOSEN)
- kiss3d renders to offscreen texture
- Makepad displays texture via Image widget
- Mouse events forwarded to kiss3d for camera control
- **Effort:** 1 week

### Comparison Matrix

| Criteria | Option 1 | Option 2 | Option 3 |
|----------|----------|----------|----------|
| Complexity | Medium | High | **Low** |
| Risk | Medium | High | **Low** |
| Matches kiss3d-egui | Partial | No | **Yes** |
| Implementation time | 2-3 weeks | 4-6 weeks | **1 week** |
| Makepad changes | Some | Significant | **None** |
| Camera interactivity | Yes | Yes | **Yes** |
| Upgradeable | Yes | - | **Yes** |

---

## Chosen Approach

### Option 3: Texture Streaming

**Rationale:**
1. Proven pattern from kiss3d-egui integration
2. Clean separation of concerns
3. No existing code refactoring required
4. Full camera interactivity via event forwarding
5. Can migrate to Option 2 if performance requires

### Data Flow

```
┌─────────────────────────────────────────────────────────────┐
│  Makepad UI Thread                                           │
│                                                              │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │ Timeline    │  │ Waveforms   │  │ URDFViewport        │  │
│  │ Slider      │  │ Plot        │  │ (Image widget)      │  │
│  └──────┬──────┘  └─────────────┘  └──────────▲──────────┘  │
│         │                                      │             │
│         │ current_time                         │ texture     │
│         ▼                                      │             │
│  ┌────────────────────────────────────────────┴──────────┐  │
│  │  SharedRobotState (existing bridge pattern)            │  │
│  │  - joint_positions: Vec<f64>                           │  │
│  │  - camera_command: Option<CameraCommand>               │  │
│  │  - render_texture: Option<Vec<u8>>                     │  │
│  └────────────────────────────────────────────────────────┘  │
│                              │                               │
│                              ▼                               │
│  ┌────────────────────────────────────────────────────────┐  │
│  │  kiss3d Render Thread (background)                      │  │
│  │                                                         │  │
│  │  loop {                                                 │  │
│  │      // 1. Get joint positions from shared state        │  │
│  │      let joints = shared_state.read().joint_positions;  │  │
│  │      robot.set_joint_positions(&joints);                │  │
│  │                                                         │  │
│  │      // 2. Process camera commands                      │  │
│  │      if let Some(cmd) = shared_state.take_camera_cmd()  │  │
│  │          camera.apply(cmd);                             │  │
│  │      }                                                  │  │
│  │                                                         │  │
│  │      // 3. Render to offscreen texture                  │  │
│  │      let pixels = window.render_offscreen();            │  │
│  │                                                         │  │
│  │      // 4. Send texture back to UI                      │  │
│  │      shared_state.write().set_texture(pixels);          │  │
│  │      SignalToUI::set_ui_signal();                       │  │
│  │  }                                                      │  │
│  └────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### Mouse Event Forwarding

```rust
// Camera commands sent from Makepad to kiss3d
pub enum CameraCommand {
    Orbit { dx: f32, dy: f32 },      // Left-drag: rotate around target
    Pan { dx: f32, dy: f32 },        // Right-drag/Shift+drag: pan
    Zoom { delta: f32 },             // Scroll wheel: zoom in/out
    Reset,                           // Double-click: reset to default view
}

// Makepad widget captures mouse events
impl Widget for URDFViewport {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        match event.hits(cx, self.view.area()) {
            Hit::FingerDown(fe) => {
                self.drag_start = Some(fe.abs);
                self.is_dragging = true;
            }
            Hit::FingerMove(fe) if self.is_dragging => {
                let dx = fe.abs.x - self.last_pos.x;
                let dy = fe.abs.y - self.last_pos.y;
                self.send_camera_command(CameraCommand::Orbit { dx, dy });
                self.last_pos = fe.abs;
            }
            Hit::FingerScroll(se) => {
                self.send_camera_command(CameraCommand::Zoom {
                    delta: se.scroll.y
                });
            }
            Hit::FingerUp(_) => {
                self.is_dragging = false;
            }
            _ => {}
        }
    }
}
```

### kiss3d ArcBall Camera Integration

```rust
// kiss3d's ArcBall camera already supports orbit/zoom/pan
fn process_camera_commands(camera: &mut ArcBall, rx: &Receiver<CameraCommand>) {
    while let Ok(cmd) = rx.try_recv() {
        match cmd {
            CameraCommand::Orbit { dx, dy } => {
                // ArcBall rotates around target point
                camera.rotate(dx * 0.01, dy * 0.01);
            }
            CameraCommand::Zoom { delta } => {
                // Adjust distance from target
                let new_dist = camera.dist() * (1.0 - delta * 0.1);
                camera.set_dist(new_dist.max(0.1));
            }
            CameraCommand::Pan { dx, dy } => {
                // Move target point in screen space
                let right = camera.eye_dir().cross(&Vector3::y());
                let up = right.cross(&camera.eye_dir());
                camera.translate_mut(&(right * dx * 0.01 + up * dy * 0.01));
            }
            CameraCommand::Reset => {
                camera.look_at(Point3::new(0.0, 0.5, 2.0), Point3::origin());
            }
        }
    }
}
```

---

## Architecture

### Module Structure

```
dorobot/
├── crates/
│   ├── dorobot-app/
│   │   └── src/
│   │       ├── widgets/
│   │       │   ├── mod.rs
│   │       │   ├── urdf_viewport.rs    # NEW: URDF viewer widget
│   │       │   └── ...
│   │       └── data/
│   │           ├── mod.rs
│   │           ├── urdf_renderer.rs    # NEW: kiss3d thread manager
│   │           └── ...
│   │
│   └── dorobot-dora-bridge/
│       └── src/
│           └── shared_state.rs         # EXTEND: add URDF state
│
└── Cargo.toml                          # ADD: kiss3d, urdf-rs deps
```

### Dependencies

```toml
# In dorobot-app/Cargo.toml
[dependencies]
kiss3d = { version = "0.35", optional = true }
urdf-rs = { version = "0.8", optional = true }
nalgebra = { version = "0.33", optional = true }

[features]
default = []
urdf = ["kiss3d", "urdf-rs", "nalgebra"]
```

---

## Implementation Checklist

### Phase 1: Foundation
- [ ] Add kiss3d and urdf-rs dependencies to Cargo.toml
- [ ] Create feature flag `urdf` for optional compilation
- [ ] Extend `SharedRobotState` with URDF-related fields:
  - [ ] `urdf_path: Option<String>`
  - [ ] `joint_positions: Vec<f64>`
  - [ ] `camera_command: Option<CameraCommand>`
  - [ ] `render_texture: Option<RenderTexture>`
- [ ] Define `CameraCommand` enum in dorobot-dora-bridge

### Phase 2: kiss3d Renderer Thread
- [ ] Create `urdf_renderer.rs` module
- [ ] Implement `URDFRenderer` struct:
  - [ ] `new(urdf_path: &str)` - load URDF and create kiss3d scene
  - [ ] `set_joint_positions(&mut self, positions: &[f64])`
  - [ ] `apply_camera_command(&mut self, cmd: CameraCommand)`
  - [ ] `render_to_texture(&mut self) -> Vec<u8>`
- [ ] Implement background thread spawning with channel communication
- [ ] Add offscreen rendering support (kiss3d `render_to_texture`)
- [ ] Test standalone URDF loading and rendering

### Phase 3: Makepad Widget
- [ ] Create `urdf_viewport.rs` widget module
- [ ] Implement `URDFViewport` widget:
  - [ ] `live_design!` with Image display area
  - [ ] Mouse event handling for camera control
  - [ ] Timer for texture refresh (30-60 fps)
- [ ] Add `URDFViewportRef` wrapper methods
- [ ] Register widget in `widgets/mod.rs`
- [ ] Test widget with placeholder texture

### Phase 4: Integration
- [ ] Connect `URDFViewport` to `URDFRenderer` via shared state
- [ ] Sync joint positions from timeline/dataset
- [ ] Forward camera commands from widget to renderer
- [ ] Update texture display on `SignalToUI`
- [ ] Add URDF viewport to home_screen layout

### Phase 5: Dataset Integration
- [ ] Detect URDF files in LeRobot dataset
- [ ] Auto-load URDF when dataset contains robot model
- [ ] Map dataset joint columns to URDF joint names
- [ ] Sync robot pose with timeline scrubbing

### Phase 6: Polish
- [ ] Add loading indicator while URDF loads
- [ ] Add error handling for missing/invalid URDF
- [ ] Add viewport controls (reset view button, etc.)
- [ ] Add joint limit visualization
- [ ] Performance optimization (texture size, frame rate)
- [ ] Documentation and examples

---

## Key Code Snippets

### URDFViewport Widget (live_design)

```rust
live_design! {
    use link::theme::*;
    use link::widgets::*;

    pub URDFViewport = {{URDFViewport}} {
        width: Fill
        height: Fill

        show_bg: true
        draw_bg: { color: #1a1a2e }

        flow: Overlay

        // 3D viewport (texture display)
        viewport_image = <Image> {
            width: Fill
            height: Fill
            fit: Smallest
        }

        // Loading overlay
        loading_overlay = <View> {
            width: Fill
            height: Fill
            align: { x: 0.5, y: 0.5 }
            visible: false

            <Label> {
                draw_text: {
                    text_style: <THEME_FONT_REGULAR>{ font_size: 14.0 }
                    color: #888
                }
                text: "Loading URDF..."
            }
        }

        // Controls overlay (top-right)
        controls_overlay = <View> {
            width: Fill
            height: Fill
            padding: 8
            align: { x: 1.0, y: 0.0 }

            reset_btn = <Button> {
                width: Fit
                height: Fit
                text: "Reset View"
            }
        }
    }
}
```

### URDFRenderer (kiss3d thread)

```rust
pub struct URDFRenderer {
    window: kiss3d::window::Window,
    camera: kiss3d::camera::ArcBall,
    robot: Option<urdf_viz::Robot>,
    joint_names: Vec<String>,
}

impl URDFRenderer {
    pub fn new() -> Self {
        let window = kiss3d::window::Window::new_hidden("URDF Renderer");
        let camera = kiss3d::camera::ArcBall::new(
            Point3::new(0.0, 0.5, 2.0),
            Point3::origin()
        );
        Self {
            window,
            camera,
            robot: None,
            joint_names: Vec::new(),
        }
    }

    pub fn load_urdf(&mut self, path: &str) -> Result<(), String> {
        let urdf = urdf_rs::read_file(path)
            .map_err(|e| format!("Failed to parse URDF: {}", e))?;

        // Create robot visualization
        let robot = urdf_viz::Robot::from_urdf(&urdf, &mut self.window)?;
        self.joint_names = robot.joint_names().to_vec();
        self.robot = Some(robot);
        Ok(())
    }

    pub fn set_joint_positions(&mut self, positions: &[f64]) {
        if let Some(robot) = &mut self.robot {
            robot.set_joint_positions_clamped(positions);
        }
    }

    pub fn render_to_rgba(&mut self, width: u32, height: u32) -> Vec<u8> {
        self.window.render_to_texture(width, height, &mut self.camera)
    }
}
```

---

## References

- [kiss3d Repository](https://github.com/dimforge/kiss3d)
- [kiss3d egui_renderer.rs](https://github.com/dimforge/kiss3d/blob/master/src/renderer/egui_renderer.rs)
- [kiss3d egui_integration.rs](https://github.com/dimforge/kiss3d/blob/master/src/window/egui_integration.rs)
- [urdf-viz Repository](https://github.com/openrr/urdf-viz)
- [urdf-rs Documentation](https://docs.rs/urdf-rs)
- [Makepad Widgets](https://github.com/makepad/makepad)
- [DoRobot SharedRobotState](../crates/dorobot-dora-bridge/src/shared_state.rs)
