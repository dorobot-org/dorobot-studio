# DoRobot-Flex Development Tasks

> Generated from code review on 2026-01-28

## Phase 1: Critical Fixes (P0) - Must Fix

### [x] 1.1 Implement Content-Panel Registry (COMPLETED)
**Priority:** CRITICAL
**Effort:** Large
**Files:** `app.rs`, `app_data.rs`

Create a registry that maps panel IDs to their content:

```rust
// Add to app_data.rs
#[derive(Clone, Debug)]
pub enum PanelContent {
    Video { key: String },
    RobotView,
    Empty,
}

#[derive(Clone, Debug, Default)]
pub struct PanelRegistry {
    pub panels: HashMap<String, PanelContent>,
}

// Add to AppData
pub panel_registry: PanelRegistry,
```

Initialize in `handle_startup()`:
```rust
self.data.panel_registry.panels.insert("panel_0".into(), PanelContent::Video { key: main_key });
self.data.panel_registry.panels.insert("panel_1".into(), PanelContent::RobotView);
// etc.
```

---

### [x] 1.2 Handle LayoutChanged Events + Content Swapping (COMPLETED)
**Priority:** CRITICAL
**Effort:** Large
**Files:** `app.rs`, `app_data.rs`

**Full documentation:** See `DRAG_DROP_FIX.md`

**Solution implemented:**
1. Each physical slot now has BOTH VideoPlayer AND RobotView widgets
2. Added `slot_to_panel: [String; 4]` to track which panel_id is at each slot
3. `on_layout_changed()` detects panel swaps and updates mapping
4. `update_slot_content()` toggles visibility and reloads videos
5. All robot views share the same URDF (only visible one renders)

**Key insight:** Content widgets can't be "moved" in Makepad DSL, so we have all content types in all slots and control visibility based on which panel_id is at each slot.

---

### [x] 1.3 Fix update_videos_now() (COMPLETED)
**Priority:** CRITICAL
**Effort:** Small
**Files:** `app.rs:1070-1117`

Replace hardcoded panel-widget mapping:

```rust
fn update_videos_now(&mut self, cx: &mut Cx) {
    if !self.time_changed() {
        return;
    }

    let layout_state = self.ui.panel_grid(id!(center_content)).layout_state();

    for (panel_id, content) in &self.data.panel_registry.panels {
        // Skip if panel not visible
        if let Some(ref state) = layout_state {
            if !state.visible_panels.contains(panel_id) {
                continue;
            }
        }

        match content {
            PanelContent::Video { key } => {
                // Get the correct video player based on key
                let player = self.get_video_player_for_key(key);
                player.show_frame_at_time(cx, self.data.current_time);
            }
            PanelContent::RobotView => {
                // Robot view updates handled separately
            }
            PanelContent::Empty => {}
        }
    }
}
```

---

## Phase 2: High Priority Fixes (P1)

### [x] 2.1 Fix Panel Title Race Condition (COMPLETED)
**Priority:** HIGH
**Effort:** Medium
**Files:** `app.rs:960-1015`, `../makepad-flex-layout/src/grid/panel_grid.rs`

**Solution implemented:**
- Modified `PanelGrid::set_layout_state()` to merge `panel_titles` from the incoming `LayoutState`
- Modified `PanelGridRef::set_layout_state()` to also merge titles to thread-local when called before first draw
- Updated `configure_panel_layout_from_registry()` to include titles directly in `LayoutState`
- Removed double `set_panel_titles()` workaround - single atomic call now works correctly

---

### [x] 2.2 Preserve Robot Name in Title (COMPLETED)
**Priority:** HIGH
**Effort:** Small
**Files:** `app.rs`, `app_data.rs`

**Solution implemented (Phase 1):**
- Added `robot_display_name: Option<String>` to AppData
- Added `get_robot_panel_title()` helper method
- Robot name is now preserved and used in panel titles via the registry

---

### [ ] 2.3 Sync makepad-flex-layout to GitHub
**Priority:** HIGH
**Effort:** Large
**Files:** `../makepad-flex-layout/*`

Options:
- **A)** Push local changes to `mofa-org/makepad-flex-layout` GitHub repo
- **B)** Keep using local path (document in README)

If Option A:
1. `cd ../makepad-flex-layout`
2. Review changes: `git diff origin/main`
3. Create feature branch: `git checkout -b feature/string-panel-ids`
4. Commit and push
5. Update Cargo.toml to use git dependency

---

## Phase 3: Medium Priority Fixes (P2)

### [x] 3.1 Create PanelId Enum (COMPLETED)
**Priority:** MEDIUM
**Effort:** Medium
**Files:** `app_data.rs`

**Already implemented as `PanelSlot` enum in Phase 1:**
- `PanelSlot` enum with `VideoMain`, `RobotView`, `VideoCam1`, `VideoCam2` variants
- `default_panel_id()` method to convert to string
- `from_panel_id()` method to parse from string (added in Phase 3)
- `all()` and `video_slots()` helper methods

---

### [x] 3.2 Add Video Load Error Handling (COMPLETED)
**Priority:** MEDIUM
**Effort:** Small
**Files:** `app.rs:1050-1053`

**Already implemented in Phase 1:**
```rust
match player.load_video(cx, &path.to_string_lossy()) {
    Ok(_) => ::log::debug!("[init_video_player] Video loaded successfully"),
    Err(e) => ::log::error!("[init_video_player] Failed to load video: {}", e),
}
```

---

### [x] 3.3 Cache Panel Visibility (COMPLETED)
**Priority:** MEDIUM
**Effort:** Small
**Files:** `app.rs:1202-1214`

**Already implemented via `get_panel_visibility()` method:**
- Returns tuple `(bool, bool, bool, bool)` for all panel visibility states
- Called once per update, result used inline
- Avoids repeated HashMap lookups

---

## Phase 4: Low Priority Fixes (P3)

### [N/A] 4.1 Remove Unnecessary Clone
**Priority:** LOW
**Effort:** Trivial
**Files:** `app.rs:762`

**Status:** Not applicable - clone is necessary because `episodes` is used twice:
1. `self.data.episodes = episodes.clone();` (storage for other components)
2. `episode_list.set_episodes(cx, episodes);` (ownership transfer to widget)

---

### [x] 4.2 Fix Log Levels (COMPLETED)
**Priority:** LOW
**Effort:** Small
**Files:** `app.rs`

**Fixed:** Changed 15 `info!` logs to `debug!` for initialization details:
- `[on_layout_changed]` logs
- `[Timing]` logs
- `[init_videos]` logs
- `[init_video_player]` logs
- `[configure_panel_layout]` logs
- `[clear_videos]` logs
- `[apply_layout_reset]` logs

Kept as `info!`:
- Dataset loading/found messages (user actions)
- Episode loading messages (user actions)
- URDF loading messages (user actions)

---

### [ ] 4.3 Create Config Module
**Priority:** LOW
**Effort:** Small
**Files:** New file `app_config.rs`

Move constants to dedicated module:

```rust
// app_config.rs
pub mod config {
    /// Playback timer frequency
    pub const PLAYBACK_TIMER_FPS: f64 = 60.0;

    /// Minimum interval between video decodes during scrubbing
    pub const SCRUB_RATE_LIMIT_MS: u64 = 100;

    /// Minimum time change to trigger video update
    pub const TIME_EPSILON: f64 = 0.001;

    /// Sliding window for time series plots
    pub const PLOT_WINDOW_SIZE: f64 = 10.0;

    /// Maximum channels to display in plots
    pub const MAX_PLOT_CHANNELS: usize = 14;
}
```

---

## Quick Wins (Can Do Now)

1. **[N/A] Task 4.1** - Clone is necessary (both destinations need ownership)
2. **[x] Task 3.2** - Already implemented with match statement
3. **[x] Task 4.2** - Fixed 15 info→debug log level changes

---

## Notes

- All P0 tasks should be completed before P1
- Tasks 1.1, 1.2, 1.3 are interconnected - do together
- Task 2.3 (GitHub sync) can be done independently
- Testing checklist in CODE_REVIEW.md after each phase
