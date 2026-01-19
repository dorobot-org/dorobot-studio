# Makepad Feature Request: Overlay Positioning Relative to Image Content Bounds

## Problem Statement

When using an `Image` widget with `fit: Smallest`, the image maintains aspect ratio and is centered within its container, leaving empty space around it. However, there's no way to position overlay elements (like labels, buttons) relative to the **actual rendered image bounds** rather than the container bounds.

## Visual Example

```
Container (400x300)
+------------------------------------------+
|                                          |
|    +----------------------------+        |
|    |                            |        |
|    |     Rendered Image         | <- Overlay should be HERE
|    |     (400x225, centered)    |        |   (top-right of image)
|    |                            |        |
|    +----------------------------+        |
|                                          |  <- But ends up HERE
+------------------------------------------+   (top-right of container)
```

## Current Behavior

```rust
live_design! {
    VideoPlayer = {{VideoPlayer}} {
        width: Fill
        height: Fill
        flow: Overlay

        // Image uses fit: Smallest - maintains aspect ratio, centers content
        frame_image = <Image> {
            width: Fill
            height: Fill
            fit: Smallest
        }

        // Overlay positioned relative to CONTAINER, not image content
        info_overlay = <View> {
            width: Fill
            height: Fill
            padding: 8
            align: { x: 1.0, y: 0.0 }  // top-right of CONTAINER

            overlay_label = <Label> {
                text: "30 fps"
            }
        }
    }
}
```

**Result:** The label appears at the top-right of the container, which may be outside the visible image area when the image has a different aspect ratio.

## Approaches Attempted (All Failed)

### 1. Dynamic Margin/Padding in draw_walk

```rust
fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
    // Calculate empty space around image based on aspect ratios
    let container_rect = cx.turtle().rect();
    let video_ar = self.video_width as f64 / self.video_height as f64;
    let container_ar = container_rect.size.x / container_rect.size.y;

    let h_margin = if video_ar < container_ar {
        let rendered_w = container_rect.size.y * video_ar;
        (container_rect.size.x - rendered_w) / 2.0
    } else { 0.0 };

    // Apply margin to push overlay into image bounds
    self.view.view(id!(info_overlay)).apply_over(cx, live! {
        margin: { left: (h_margin), right: (h_margin) }
    });

    self.view.draw_walk(cx, scope, walk)
}
```

**Result:** `apply_over` during `draw_walk` doesn't reliably update layout. The margin values are calculated correctly but don't take effect.

### 2. Absolute Positioning (abs_pos)

```rust
// Calculate position and use abs_pos
let overlay_x = video_right_edge - overlay_width - padding;
let overlay_y = video_top_edge + padding;

self.view.view(id!(overlay_label)).apply_over(cx, live! {
    abs_pos: { x: (overlay_x), y: (overlay_y) }
});
```

**Result:** Labels disappear entirely. `abs_pos` doesn't work correctly when applied dynamically.

### 3. Fit-Sized Container

```rust
live_design! {
    // Wrapper that should shrink to image size
    video_container = <View> {
        width: Fit
        height: Fit
        flow: Overlay

        frame_image = <Image> {
            width: Fit
            height: Fit
        }

        info_overlay = <View> { ... }
    }
}
```

**Result:** Image doesn't render at all. The `Image` widget with `width: Fit, height: Fit` doesn't work with dynamically set textures.

## Current Workaround

We render the overlay text directly onto the video frame buffer before creating the texture:

```rust
pub fn set_frame(&mut self, cx: &mut Cx, width: usize, height: usize, data: &[u8]) {
    let mut frame_data = data.to_vec();

    // Render text directly onto pixel buffer
    Self::render_text_on_frame(&mut frame_data, width, height, &self.overlay_text);

    // Create texture from modified buffer
    let bgra_data = Self::rgb_to_bgra_u32(&frame_data);
    let texture = Texture::new_with_format(cx, TextureFormat::VecBGRAu8_32 {
        data: Some(bgra_data),
        width,
        height,
        updated: TextureUpdated::Full,
    });

    self.view.image(id!(frame_image)).set_texture(cx, Some(texture));
}

fn render_text_on_frame(data: &mut [u8], width: usize, height: usize, text: &str) {
    // Simple bitmap font rendering onto RGB buffer
    let scale = 2;
    let padding = 8;
    let text_x = width - text.len() * 12 - padding;  // Position at top-right of IMAGE
    let text_y = padding;

    // Draw background
    for y in (text_y - 4)..(text_y + 18) {
        for x in (text_x - 4)..(width - 4) {
            let idx = (y * width + x) * 3;
            data[idx] = 0;      // Black background
            data[idx + 1] = 0;
            data[idx + 2] = 0;
        }
    }

    // Draw text using bitmap font
    for (i, c) in text.chars().enumerate() {
        draw_char(data, width, text_x + i * 12, text_y, c);
    }
}
```

**Limitations of this workaround:**
- Requires custom bitmap font implementation
- Text quality is lower than native Makepad text rendering
- CPU overhead for modifying every frame
- Can't use Makepad's theming/styling for the overlay

## Feature Request

### Option A: Expose Image Content Rect

Add a method to `Image` widget that returns the actual rendered content bounds:

```rust
impl Image {
    /// Returns the rect where the image content is actually rendered
    /// (accounting for fit mode and centering)
    pub fn content_rect(&self) -> Rect {
        // Calculate based on texture size, widget size, and fit mode
    }
}

// Usage in draw_walk:
fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
    let image = self.view.image(id!(frame_image));
    let content_rect = image.content_rect();

    // Position overlay relative to content_rect
    self.view.view(id!(overlay)).apply_over(cx, live! {
        abs_pos: { x: (content_rect.pos.x + content_rect.size.x - 100.0), y: (content_rect.pos.y + 8.0) }
    });

    self.view.draw_walk(cx, scope, walk)
}
```

### Option B: New Layout Mode for Image

Add a layout mode where the `Image` widget's bounds match its content:

```rust
live_design! {
    frame_image = <Image> {
        width: Fill
        height: Fill
        fit: Smallest
        layout_to_content: true  // NEW: Widget bounds match rendered content
    }
}
```

When `layout_to_content: true`:
- The widget reports its size as the rendered content size
- Children in an `Overlay` flow are positioned relative to content bounds
- The image is still centered in available space

### Option C: Content-Relative Alignment

Add alignment options that reference content bounds:

```rust
live_design! {
    VideoPlayer = {{VideoPlayer}} {
        flow: Overlay

        frame_image = <Image> {
            id: video
            fit: Smallest
        }

        overlay = <View> {
            // NEW: Align relative to sibling's content bounds
            align_to: video.content
            align: { x: 1.0, y: 0.0 }  // top-right of video CONTENT
        }
    }
}
```

## Use Cases

1. **Video players** - Displaying frame count, FPS, timestamps on video
2. **Image viewers** - Showing metadata, zoom level, resolution overlays
3. **Games** - HUD elements that should stay within the game viewport
4. **Photo editors** - Tool overlays that shouldn't extend beyond the image

## Environment

- Makepad version: Latest (as of 2025)
- Platform: macOS, but issue is cross-platform
- Widget: Image with fit: Smallest

## Related

This is a fundamental limitation of how Makepad's layout system interacts with content-aware sizing. The `fit` property affects rendering but not layout bounds.
