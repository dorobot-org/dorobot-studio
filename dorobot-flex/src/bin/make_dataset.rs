//! Generate a synthetic LeRobot dataset with the writer.
//!
//! Useful without a robot: produces a dataset the shipping player can open,
//! and doubles as a manual end-to-end check of the writer/reader pair.
//!
//!     cargo run --release -p dorobot-flex --bin make_dataset -- dataset/synthetic-demo

use std::f32::consts::TAU;

use dorobot_flex::data::lerobot_writer::{DatasetSpec, DatasetWriter, FrameRecord};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "dataset/synthetic-demo".to_string());
    if std::path::Path::new(&root).exists() {
        std::fs::remove_dir_all(&root)?;
    }

    let joints = ["shoulder_pan", "shoulder_lift", "elbow_flex", "wrist_flex", "wrist_roll", "gripper"];
    let spec = DatasetSpec {
        robot_type: "so101".into(),
        fps: 30.0,
        camera_keys: vec!["observation.images.cam_high".into()],
        joint_names: joints.iter().map(|s| s.to_string()).collect(),
    };

    let mut w = DatasetWriter::create(&root, spec)?;
    let tasks = ["Pick the block and place it in the box", "Open the drawer"];

    for ep in 0..6u32 {
        let task = tasks[(ep as usize) % tasks.len()];
        let mut e = w.begin_episode(task)?;
        let frames = 90 + ep * 15;
        for i in 0..frames {
            let t = i as f32 / frames as f32;
            // A plausible reach-and-return sweep, phase-shifted per joint.
            let state: Vec<f32> = (0..joints.len())
                .map(|j| {
                    let ph = j as f32 * 0.6 + ep as f32 * 0.2;
                    0.8 * (t * TAU * 0.5 + ph).sin() * (1.0 - 0.3 * j as f32 / 6.0)
                })
                .collect();
            // Commanded leads measured slightly, as a real controller would.
            let action: Vec<f32> = state.iter().map(|v| v * 1.04 + 0.02).collect();
            e.push(FrameRecord { state, action })?;
        }
        let n = e.frame_count();
        let index = w.commit(e)?;
        println!("episode {index}: {n} frames · \"{task}\"");
    }

    println!("\nwrote {} — open with:", root);
    println!("  cargo run --release -p dorobot-flex -- {root}");
    Ok(())
}
