//! URDF Robot Viewer Library
//!
//! Provides embeddable widgets for viewing URDF robots in Makepad applications.

use makepad_widgets::*;

pub mod mesh;
pub mod robot_view;

pub fn live_design(cx: &mut Cx) {
    mesh::live_design(cx);
    robot_view::live_design(cx);
}
