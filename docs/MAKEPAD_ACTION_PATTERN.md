# Makepad Widget Action Pattern

## Problem

When handling custom widget actions in Makepad, using `action.cast::<MyAction>()` directly on the action from `cx.capture_actions()` returns `MyAction::None` instead of the actual action.

## Root Cause

Widget actions dispatched via `cx.widget_action(uid, path, action)` are wrapped in a `WidgetAction` struct. The raw `action.cast()` tries to cast the outer wrapper, not the inner action.

## Wrong Pattern

```rust
// In parent widget's handle_widget_actions:
for action in actions {
    // WRONG - This casts the outer Action, not the inner widget action
    if let MyAction::SomeVariant(value) = action.cast() {
        // This never matches! cast() returns MyAction::None
        self.handle_value(value);
    }
}
```

## Correct Pattern

```rust
// In parent widget's handle_widget_actions:
for action in actions {
    // CORRECT - First unwrap the WidgetAction, then cast its inner action
    if let Some(widget_action) = action.as_widget_action() {
        match widget_action.cast::<MyAction>() {
            MyAction::SomeVariant(value) => {
                self.handle_value(value);
            }
            MyAction::AnotherVariant => {
                self.do_something();
            }
            _ => {}
        }
    }
}
```

## Full Example

### Defining the Action (in child widget)

```rust
#[derive(Clone, Debug, DefaultNone)]
pub enum TimelineAction {
    Play,
    Pause,
    Seek(f64),
    StepForward,
    StepBackward,
    None,
}
```

### Dispatching the Action (in child widget)

```rust
impl Timeline {
    fn scrub_to_position(&mut self, cx: &mut Cx, scope: &mut Scope, time: f64) {
        // Dispatch action to parent
        cx.widget_action(
            self.widget_uid(),
            &scope.path,
            TimelineAction::Seek(time)
        );
    }
}
```

### Receiving the Action (in parent widget)

```rust
impl HomeScreen {
    fn handle_widget_actions(&mut self, cx: &mut Cx, actions: &Actions, scope: &mut Scope) {
        for action in actions {
            // Step 1: Unwrap the WidgetAction
            if let Some(widget_action) = action.as_widget_action() {
                // Step 2: Cast the inner action to your type
                match widget_action.cast::<TimelineAction>() {
                    TimelineAction::Seek(time) => {
                        self.seek_to(cx, time);
                    }
                    TimelineAction::Play => {
                        self.set_playing(cx, true);
                    }
                    TimelineAction::Pause => {
                        self.set_playing(cx, false);
                    }
                    _ => {}
                }
            }
        }
    }
}
```

## Key Points

1. **Always use `action.as_widget_action()`** to unwrap widget actions
2. **Then use `widget_action.cast::<T>()`** to cast to your specific action type
3. **DefaultNone trait** is required for your action enum
4. **Button clicks work differently** - use `button.clicked(&actions)` which handles this internally

## When This Applies

- Custom widget actions dispatched via `cx.widget_action()`
- Parent widgets receiving actions from child widgets
- Any action type implementing `DefaultNone`

## When This Does NOT Apply

- Built-in button clicks: use `button.clicked(&actions)`
- Actions from `cx.action()` (background threads): use `action.downcast_ref::<T>()`
