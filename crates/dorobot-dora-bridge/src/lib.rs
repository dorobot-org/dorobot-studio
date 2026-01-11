//! Dorobot Dora Bridge
//!
//! This crate provides:
//! - `SharedRobotState`: Thread-safe shared state with dirty tracking for UI polling
//! - `RerunLogger`: Integration with Rerun for 3D visualization
//! - Dirty tracking primitives (`DirtyValue`, `DirtyVec`)

mod dirty;
mod shared_state;

#[cfg(feature = "rerun")]
mod rerun_logger;

pub use dirty::{DirtyValue, DirtyVec};
pub use shared_state::{LogEntry, LogLevel, SharedRobotState, StateReader, StateWriter};

#[cfg(feature = "rerun")]
pub use rerun_logger::RerunLogger;
