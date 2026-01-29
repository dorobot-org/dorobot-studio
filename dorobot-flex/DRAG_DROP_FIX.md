# Drag-and-Drop Content Swapping Fix

## Problem

When panels were dragged and dropped in the PanelGrid, only the panel chrome (title bar, buttons) moved with the panel_id. The actual content widgets (VideoPlayer, RobotView) stayed in their original physical slots because they were statically defined in the DSL.

**Before fix:**
- Drag panel_0 (cam_high video) to panel_1's position
- Result: Title shows "cam_high" but content shows 3D robot view (wrong!)

## Solution Architecture

### 1. Dual-Content Slots

Each physical slot now contains BOTH content types:

```rust
// app.rs live_design!
s1_1 = {
    title: "cam_high"
    content = {
        video_slot0 = <VideoPlayer> {}
        robot_slot0 = <RobotView> { visible: false, width: 0, height: 0 }
    }
}
s1_2 = {
    title: "3D View"
    content = {
        video_slot1 = <VideoPlayer> { visible: false, width: 0, height: 0 }
        robot_slot1 = <RobotView> {}
    }
}
// ... same pattern for s2_1, s2_2
```

### 2. Slot-to-Panel Mapping

Added tracking in `AppData`:

```rust
// app_data.rs
pub struct AppData {
    // ...
    /// Slot content mapping - which panel_id is at each physical slot
    /// Index 0-3 corresponds to physical slots (s1_1, s1_2, s2_1, s2_2)
    pub slot_to_panel: [String; 4],
}
```

Default mapping:
- Slot 0 (s1_1) → panel_0 (VideoMain)
- Slot 1 (s1_2) → panel_1 (RobotView)
- Slot 2 (s2_1) → panel_2 (VideoCam1)
- Slot 3 (s2_2) → panel_3 (VideoCam2)

### 3. Layout Change Handler

When panels are dragged, `on_layout_changed()` is called:

```rust
fn on_layout_changed(&mut self, cx: &mut Cx, new_state: &LayoutState) {
    // 1. Extract new panel_id positions from row_assignments
    // 2. Update slot_to_panel mapping
    // 3. Call update_slot_content() to swap visibility
}
```

### 4. Content Visibility Swapping

`update_slot_content()` handles the actual swap:

```rust
fn update_slot_content(&mut self, cx: &mut Cx, slot_mapping: &[String; 4]) {
    for (slot_idx, panel_id) in slot_mapping.iter().enumerate() {
        let is_robot = PanelSlot::from_panel_id(panel_id) == Some(PanelSlot::RobotView);

        if is_robot {
            // Show robot view, hide video player
            self.ui.view(video_slots[slot_idx]).apply_over(cx, live! {
                visible: false, width: 0, height: 0
            });
            self.ui.view(robot_slots[slot_idx]).apply_over(cx, live! {
                visible: true, width: Fill, height: Fill
            });
        } else {
            // Show video player, hide robot view
            // Also reload the correct video for this panel_id
            // ...
        }
    }
}
```

## Key Files Modified

| File | Changes |
|------|---------|
| `app.rs` | DSL with dual-content slots, `on_layout_changed()`, `update_slot_content()`, updated all widget references |
| `app_data.rs` | Added `slot_to_panel: [String; 4]` field, `PanelSlot::from_panel_id()` method |

## Widget ID Mapping

| Old ID | New IDs |
|--------|---------|
| `video_main` | `video_slot0` |
| `robot_view` | `robot_slot0`, `robot_slot1`, `robot_slot2`, `robot_slot3` |
| `video_cam1` | `video_slot2` |
| `video_cam2` | `video_slot3` |

## How It Works

1. **Initial state**: Videos loaded into slots 0, 2, 3. Robot view visible in slot 1.

2. **User drags panel_0 to panel_1's position**:
   - PanelGrid emits `PanelAction::LayoutChanged` with new `row_assignments`
   - `on_layout_changed()` detects mapping changed
   - `update_slot_content()` is called:
     - Slot 0: Now has panel_1 (RobotView) → hide video, show robot
     - Slot 1: Now has panel_0 (VideoMain) → show video, hide robot, load cam_high video

3. **Result**: Content follows the panel titles correctly.

## Performance Considerations

- Robot URDF is loaded into ALL 4 robot view slots (only visible one renders)
- Video players reload when panels swap (necessary to show correct content)
- Joint angle updates go to all robot views (lightweight operation)

## Limitations

- Requires 4x VideoPlayer + 4x RobotView widgets (memory overhead)
- Video reload on swap causes brief flash (acceptable trade-off)
- Only supports 4-panel grid (could be extended for more)
