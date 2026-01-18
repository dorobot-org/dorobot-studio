//! Makepad URDF Player
//!
//! A Makepad-based URDF robot viewer with embeddable RobotView widget.

use makepad_widgets::*;

pub mod mesh;
pub mod robot_view;

pub fn live_design(cx: &mut Cx) {
    mesh::live_design(cx);
    robot_view::live_design(cx);
}
