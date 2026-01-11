//! Home screen with main dashboard layout

pub mod home_screen;

use makepad_widgets::Cx;

pub fn live_design(cx: &mut Cx) {
    self::home_screen::live_design(cx);
}
