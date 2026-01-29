# DoRobot-Flex Code Review & Development Plan

**Review Date:** 2026-01-28
**Reviewer:** Claude Code
**Quality Rating:** 5/10 - Significant architectural issues requiring attention

---

## Executive Summary

The dorobot-flex application has a **fundamental architectural mismatch** between how widgets are placed (statically in DSL) and how the panel grid manages layout (dynamically via panel IDs). This causes the core drag-and-drop functionality to be broken - panel titles move but content stays in place.

Additionally, there are several medium-priority issues around panel title synchronization, error handling, and performance.

---

## Table of Contents

1. [Critical Issues](#critical-issues)
2. [High Priority Issues](#high-priority-issues)
3. [Medium Priority Issues](#medium-priority-issues)
4. [Low Priority Issues](#low-priority-issues)
5. [Development Plan](#development-plan)
6. [Architecture Recommendations](#architecture-recommendations)

---

## Critical Issues

### CRIT-1: Panel Content Does NOT Move During Drag-and-Drop

**Location:** `app.rs:173-222` (live_design) + `makepad-flex-layout/src/grid/panel_grid.rs`

**Problem:**

Widgets are **statically embedded** in specific slots in the DSL:

```rust
center_content = <PanelGrid> {
    window_container = {
        row1 = {
            s1_1 = {
                title: "cam_high"
                content = { video_main = <VideoPlayer> {} }  // STATIC in s1_1
            }
            s1_2 = {
                title: "3D View"
                content = { robot_view = <RobotView> {} }    // STATIC in s1_2
            }
        }
        row2 = {
            s2_1 = { content = { video_cam1 = <VideoPlayer> {} } }  // STATIC in s2_1
            s2_2 = { content = { video_cam2 = <VideoPlayer> {} } }  // STATIC in s2_2
        }
    }
}
```

But `PanelGrid.move_panel()` only moves panel IDs in `row_assignments` - it does NOT physically move widget content between slots.

**What Actually Happens:**

1. User drags "cam_high" panel from row1.s1_1 to row2.s2_1
2. `move_panel("panel_0", 1, 0)` updates `row_assignments`
3. `apply_row_layout()` shows/hides slots based on new assignments
4. **BUT** the `VideoPlayer` widget stays in slot `s1_1`
5. **RESULT**: Panel title moves, video content stays in original slot

**Impact:** Core drag-and-drop functionality is broken

---

### CRIT-2: Hardcoded Panel-to-Widget Mapping in Video Updates

**Location:** `app.rs:1082-1110`

**Problem:**

```rust
fn update_videos_now(&mut self, cx: &mut Cx) {
    let is_panel_visible = |panel_id: &str| -> bool {
        layout_state.as_ref()
            .map(|s| s.visible_panels.contains(panel_id))
            .unwrap_or(true)
    };

    // HARDCODED: Assumes video_main is ALWAYS in panel_0
    if is_panel_visible("panel_0") {
        let video_main = self.ui.video_player(id!(video_main));
        video_main.show_frame_at_time(cx, self.data.current_time);
    }

    // HARDCODED: Assumes video_cam1 is ALWAYS in panel_2
    if is_panel_visible("panel_2") {
        let video_cam1 = self.ui.video_player(id!(video_cam1));
        // ...
    }
}
```

After drag-and-drop rearranges panels, this mapping is incorrect:
- Videos may not update when their panel is visible
- Videos may decode when their panel is hidden (wasting CPU)

**Impact:** Video playback broken after drag-and-drop

---

## High Priority Issues

### HIGH-1: Panel Titles Lost During Layout Configuration

**Location:** `app.rs:946-988`

**Problem:**

```rust
fn configure_panel_layout(&mut self, cx: &mut Cx, ...) {
    panel_grid.set_panel_titles(&titles);      // Line 946 - Set first

    let layout_state = match camera_count { ... };

    panel_grid.set_layout_state(cx, layout_state);  // Line 978 - May reset

    // WORKAROUND: Set titles AGAIN after layout
    panel_grid.set_panel_titles(&titles);      // Line 987 - Why needed?
}
```

The double `set_panel_titles()` is a workaround for an underlying sync bug. This is fragile and may fail in edge cases.

**Impact:** Panel titles may disappear or show wrong values

---

### HIGH-2: Robot Name Title Gets Overwritten

**Location:** `app.rs:1128-1143`

**Problem:**

```rust
fn load_robot_urdf(&mut self, cx: &mut Cx) {
    if let Some((_, _, display_name)) = Self::get_urdf_path(&self.data.robot_type) {
        let title = format!("{} 3D View", display_name);  // "ViperX 300s 3D View"
        panel_grid.set_panel_titles(&[("panel_1", &title)]);
    }
}
```

But `configure_panel_layout()` later overwrites with generic "3D View":

```rust
("panel_1", "3D View")  // Loses robot name!
```

**Scenario:**
1. Load dataset with ViperX 300s robot
2. Title becomes "ViperX 300s 3D View" ✓
3. Change episode → `configure_panel_layout()` called
4. Title reverts to "3D View" ✗

**Impact:** User-facing feature regression

---

### HIGH-3: GitHub Version Incompatible

**Location:** `Cargo.toml` workspace dependency

**Problem:**

The GitHub version of `makepad-flex-layout` uses `u64` for panel IDs:
```rust
// GitHub version
pub visible_panels: Vec<u64>,
pub row_assignments: Vec<Vec<u64>>,
```

But the local version uses `String`:
```rust
// Local version
pub visible_panels: HashSet<String>,
pub row_assignments: Vec<Vec<String>>,
```

The local version also has additional features not in GitHub:
- `FooterGrid` widget
- `get_global_dark_mode()` function
- `PanelGridWidgetRefExt` trait with `set_panel_titles()`

**Impact:** Cannot use git dependency; must use local path

---

## Medium Priority Issues

### MED-1: Three Inconsistent Panel ID Systems

**Location:** Throughout codebase

**Problem:**

The codebase uses three different identification systems with no mapping:

| System | Example | Where Used |
|--------|---------|------------|
| String IDs | `"panel_0"`, `"panel_1"` | LayoutState |
| Widget IDs | `id!(video_main)`, `id!(robot_view)` | Rust code |
| Slot positions | `s1_1`, `s1_2`, `s2_1` | DSL live_design |

**Impact:** Architecture confusion, hard to maintain

---

### MED-2: Inefficient Visibility Check in Hot Path

**Location:** `app.rs:1084-1088`

**Problem:**

```rust
let is_panel_visible = |panel_id: &str| -> bool {
    layout_state.as_ref()
        .map(|s| s.visible_panels.contains(panel_id))  // Contains check
        .unwrap_or(true)
};
```

Called 3 times per frame at 60fps = 180 checks/second. While local version uses `HashSet` (O(1)), could be optimized further.

**Impact:** Minor performance overhead

---

### MED-3: Silent Video Load Failure

**Location:** `app.rs` ~line 892 (in `init_video_player`)

**Problem:**

```rust
let _ = player.load_video(cx, &path.to_string_lossy());  // Result ignored!
```

If video loading fails, error is silently discarded.

**Impact:** No user feedback on errors

---

### MED-4: Default Visibility Assumption

**Location:** `app.rs:1087`

**Problem:**

```rust
.unwrap_or(true)  // Default to visible if layout_state unavailable
```

During startup, if `layout_state()` returns None, assumes all panels visible. This causes unnecessary video decoding for potentially hidden panels.

**Impact:** Wasted CPU during initialization

---

## Low Priority Issues

### LOW-1: Unnecessary Clone in Episode Loading

**Location:** `app.rs:723`

**Problem:**

```rust
self.data.episodes = episodes.clone();  // Unnecessary clone
```

Should be:
```rust
self.data.episodes = episodes;  // Move, don't clone
```

**Impact:** Minor memory waste

---

### LOW-2: Excessive Debug Logging

**Location:** Multiple locations in `app.rs`

**Problem:**

```rust
::log::info!("[init_videos] camera_count={}, fps={}, video_keys={:?}", ...);
::log::info!("[init_videos] main_key={:?}, cam1_key={:?}, cam2_key={:?}", ...);
```

All logs use `info!` level when some should be `debug!` or `trace!`.

**Impact:** Log noise in production

---

### LOW-3: Magic Numbers

**Location:** `app.rs:10-24`

**Problem:**

Constants are defined but scattered:
```rust
const PLAYBACK_TIMER_FPS: f64 = 60.0;
const SCRUB_RATE_LIMIT_MS: u64 = 100;
const MAX_PLOT_CHANNELS: usize = 14;
```

No validation that these values are consistent.

**Impact:** Maintainability

---

## Development Plan

### Phase 1: Critical Fixes (P0)

#### Task 1.1: Implement Content-Panel Registry
**Effort:** Large
**Files:** `app.rs`, potentially `makepad-flex-layout/src/grid/panel_grid.rs`

Create a registry mapping panel IDs to content types:

```rust
#[derive(Clone, Debug)]
enum PanelContent {
    Video { key: String, widget_id: LiveId },
    RobotView { widget_id: LiveId },
    Empty,
}

struct PanelRegistry {
    panels: HashMap<String, PanelContent>,
}
```

**Steps:**
1. Define `PanelContent` enum and `PanelRegistry` struct
2. Initialize registry with default mappings in `handle_startup()`
3. Update registry when panels are rearranged via drag-drop
4. Use registry in `update_videos_now()` instead of hardcoded mappings

---

#### Task 1.2: Handle Drag-Drop Layout Changes
**Effort:** Medium
**Files:** `app.rs`

Listen for `PanelAction::LayoutChanged` and update content mappings:

```rust
// In handle_event
if let Some(action) = actions.find_widget_action(panel_grid.widget_uid()) {
    if let PanelAction::LayoutChanged(new_state) = action.cast() {
        self.sync_content_with_layout(&new_state);
    }
}
```

**Steps:**
1. Add handler for `LayoutChanged` action
2. Implement `sync_content_with_layout()` to update registry
3. Ensure video updates use registry, not hardcoded IDs

---

#### Task 1.3: Fix Video Update Logic
**Effort:** Small
**Files:** `app.rs:1070-1117`

Refactor `update_videos_now()` to use registry:

```rust
fn update_videos_now(&mut self, cx: &mut Cx) {
    for (panel_id, content) in &self.panel_registry.panels {
        if !self.is_panel_visible(panel_id) {
            continue;
        }
        match content {
            PanelContent::Video { widget_id, .. } => {
                let player = self.ui.video_player(*widget_id);
                player.show_frame_at_time(cx, self.data.current_time);
            }
            _ => {}
        }
    }
}
```

---

### Phase 2: High Priority Fixes (P1)

#### Task 2.1: Fix Panel Title Synchronization
**Effort:** Medium
**Files:** `app.rs`, `makepad-flex-layout/src/grid/panel_grid.rs`

Store titles in `LayoutState` and ensure persistence:

```rust
// In configure_panel_layout
layout_state.panel_titles.insert("panel_0".to_string(), main_name.to_string());
layout_state.panel_titles.insert("panel_1".to_string(), robot_title.to_string());
// ...
panel_grid.set_layout_state(cx, layout_state);  // Titles included
```

**Steps:**
1. Remove double `set_panel_titles()` workaround
2. Set titles via `layout_state.panel_titles` before `set_layout_state()`
3. Verify titles persist through layout changes

---

#### Task 2.2: Preserve Robot Name in Title
**Effort:** Small
**Files:** `app.rs`

Store robot display name and use it consistently:

```rust
// In AppData
robot_display_name: Option<String>,

// In load_robot_urdf
self.data.robot_display_name = Some(display_name.to_string());

// In configure_panel_layout
let robot_title = self.data.robot_display_name
    .as_ref()
    .map(|n| format!("{} 3D View", n))
    .unwrap_or_else(|| "3D View".to_string());
```

---

#### Task 2.3: Sync Local and GitHub Versions
**Effort:** Large
**Files:** `makepad-flex-layout/*`

Either:
- **Option A:** Push local changes to GitHub repo
- **Option B:** Document that local path dependency is required

**Steps for Option A:**
1. Review all local changes vs GitHub
2. Create PR with new features (FooterGrid, string IDs, etc.)
3. Update dorobot Cargo.toml to use git dependency
4. Test full application

---

### Phase 3: Medium Priority Fixes (P2)

#### Task 3.1: Unify Panel ID System
**Effort:** Medium
**Files:** `app.rs`, `app_data.rs`

Create a single source of truth for panel-content mapping:

```rust
// Single enum for all panel types
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum PanelId {
    VideoMain,
    RobotView,
    VideoCam1,
    VideoCam2,
}

impl PanelId {
    fn as_str(&self) -> &'static str {
        match self {
            Self::VideoMain => "panel_0",
            Self::RobotView => "panel_1",
            Self::VideoCam1 => "panel_2",
            Self::VideoCam2 => "panel_3",
        }
    }

    fn widget_id(&self) -> LiveId {
        match self {
            Self::VideoMain => id!(video_main),
            Self::RobotView => id!(robot_view),
            Self::VideoCam1 => id!(video_cam1),
            Self::VideoCam2 => id!(video_cam2),
        }
    }
}
```

---

#### Task 3.2: Add Video Load Error Handling
**Effort:** Small
**Files:** `app.rs`

```rust
match player.load_video(cx, &path.to_string_lossy()) {
    Ok(_) => log::debug!("Loaded video: {}", key),
    Err(e) => {
        log::error!("Failed to load video {}: {}", key, e);
        // Optionally show error in UI
        self.show_video_error(cx, panel_id, &e.to_string());
    }
}
```

---

#### Task 3.3: Optimize Visibility Checks
**Effort:** Small
**Files:** `app.rs`

Cache visibility state at start of frame:

```rust
fn update_videos_now(&mut self, cx: &mut Cx) {
    // Cache visibility once per frame
    let visibility = if let Some(state) = self.get_layout_state() {
        PanelVisibility {
            panel_0: state.visible_panels.contains("panel_0"),
            panel_1: state.visible_panels.contains("panel_1"),
            panel_2: state.visible_panels.contains("panel_2"),
            panel_3: state.visible_panels.contains("panel_3"),
        }
    } else {
        PanelVisibility::all_visible()
    };

    // Use cached values
    if visibility.panel_0 { ... }
}
```

---

### Phase 4: Low Priority Fixes (P3)

#### Task 4.1: Remove Unnecessary Clone
**Effort:** Trivial
**Files:** `app.rs:723`

Change:
```rust
self.data.episodes = episodes.clone();
```
To:
```rust
self.data.episodes = episodes;
```

---

#### Task 4.2: Improve Logging Levels
**Effort:** Small
**Files:** `app.rs`

Replace `info!` with appropriate levels:
```rust
log::debug!("Initializing {} cameras at {:.1}fps", video_keys.len(), fps);
log::trace!("Camera assignments: main={:?}, cam1={:?}, cam2={:?}", ...);
```

---

#### Task 4.3: Consolidate Constants
**Effort:** Small
**Files:** `app.rs`

Create config module:
```rust
mod config {
    pub const PLAYBACK_TIMER_FPS: f64 = 60.0;
    pub const SCRUB_RATE_LIMIT_MS: u64 = 100;
    pub const MAX_PLOT_CHANNELS: usize = 14;

    // Derived constants
    pub const FRAME_INTERVAL_MS: f64 = 1000.0 / PLAYBACK_TIMER_FPS;
}
```

---

## Architecture Recommendations

### Current Architecture (Problematic)

```
┌─────────────────────────────────────────────────┐
│                    DSL                          │
│  ┌─────────────────────────────────────────┐   │
│  │ PanelGrid                               │   │
│  │  ├─ s1_1: VideoPlayer (video_main)      │   │ ← Static widget placement
│  │  ├─ s1_2: RobotView (robot_view)        │   │
│  │  ├─ s2_1: VideoPlayer (video_cam1)      │   │
│  │  └─ s2_2: VideoPlayer (video_cam2)      │   │
│  └─────────────────────────────────────────┘   │
└─────────────────────────────────────────────────┘
                      ↕ MISMATCH
┌─────────────────────────────────────────────────┐
│                LayoutState                      │
│  row_assignments: [                             │
│    ["panel_0", "panel_1"],  ← Dynamic IDs      │
│    ["panel_2", "panel_3"]                      │
│  ]                                             │
└─────────────────────────────────────────────────┘
```

### Recommended Architecture

```
┌─────────────────────────────────────────────────┐
│              PanelRegistry (NEW)                │
│  ┌─────────────────────────────────────────┐   │
│  │ panel_0 → VideoContent { key: "main" }  │   │
│  │ panel_1 → RobotViewContent              │   │
│  │ panel_2 → VideoContent { key: "cam1" }  │   │
│  │ panel_3 → VideoContent { key: "cam2" }  │   │
│  └─────────────────────────────────────────┘   │
└─────────────────────────────────────────────────┘
                      ↕ SYNC
┌─────────────────────────────────────────────────┐
│                LayoutState                      │
│  row_assignments: [                             │
│    ["panel_0", "panel_1"],                     │
│    ["panel_2", "panel_3"]                      │
│  ]                                             │
│  panel_titles: {                               │
│    "panel_0": "cam_high",                      │
│    "panel_1": "ViperX 300s 3D View",          │
│  }                                             │
└─────────────────────────────────────────────────┘
                      ↕ DRIVES
┌─────────────────────────────────────────────────┐
│              Dynamic Rendering                  │
│  For each visible panel_id:                    │
│    1. Get content from PanelRegistry           │
│    2. Render appropriate widget                │
│    3. Update video/robot based on content      │
└─────────────────────────────────────────────────┘
```

### Key Principles

1. **Single Source of Truth:** `LayoutState` + `PanelRegistry` together define what's where
2. **Content Follows Panel:** When panel_0 moves, its video content moves with it
3. **Titles in State:** Panel titles stored in `LayoutState.panel_titles`, not set separately
4. **Type-Safe IDs:** Use enum `PanelId` instead of string literals

---

## Testing Checklist

After implementing fixes, verify:

- [ ] Drag panel from row1 to row2 → content follows
- [ ] Drag panel within same row → content follows
- [ ] Close panel → other panels adjust correctly
- [ ] Maximize panel → shows correct content
- [ ] Load new dataset → titles update correctly
- [ ] Robot name appears in 3D View panel title
- [ ] Video errors show user feedback
- [ ] No console errors during drag-drop
- [ ] Performance: 60fps maintained during playback

---

## Appendix: File Locations

| Component | File Path |
|-----------|-----------|
| Main App | `dorobot-flex/src/app.rs` |
| App Data | `dorobot-flex/src/app_data.rs` |
| Video Player | `dorobot-flex/src/widgets/video_player.rs` |
| Panel Grid | `makepad-flex-layout/src/grid/panel_grid.rs` |
| Layout State | `makepad-flex-layout/src/grid/layout_state.rs` |
| Drop Handler | `makepad-flex-layout/src/grid/drop_handler.rs` |
