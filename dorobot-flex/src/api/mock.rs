//! In-memory backend whose contents mirror the design mockups exactly.
//!
//! Every string and number here is transcribed from `docs/ux/ux-0*.png`, so a
//! screenshot of the running app can be diffed against the design render
//! without accounting for data differences. Swap a real backend in per screen;
//! this one stays as the fixture the visual regression suite runs against.

use std::f32::consts::TAU;
use std::path::PathBuf;

use super::*;

/// Deterministic pseudo-signal so traces look alive but never change between
/// runs — a moving screenshot cannot be compared against a fixed render.
fn wave(len: usize, freq: f32, phase: f32, amp: f32, bias: f32) -> Vec<f32> {
    (0..len)
        .map(|i| {
            let t = i as f32 / len as f32;
            bias + amp * ((t * TAU * freq + phase).sin() + 0.28 * (t * TAU * freq * 2.7 + phase).sin())
        })
        .collect()
}

pub struct MockBackend {
    screen: Screen,
    library: LibraryState,
    hardware: HardwareState,
    record: RecordState,
    playback: PlaybackState,
    eval: EvalState,
}

impl Default for MockBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl MockBackend {
    pub fn new() -> Self {
        Self {
            screen: Screen::Library,
            library: library_fixture(),
            hardware: hardware_fixture(),
            record: record_fixture(),
            playback: playback_fixture(),
            eval: eval_fixture(),
        }
    }
}

impl Backend for MockBackend {
    fn screen(&self) -> Screen {
        self.screen
    }
    fn library(&self) -> &LibraryState {
        &self.library
    }
    fn hardware(&self) -> &HardwareState {
        &self.hardware
    }
    fn record(&self) -> &RecordState {
        &self.record
    }
    fn playback(&self) -> &PlaybackState {
        &self.playback
    }
    fn eval(&self) -> &EvalState {
        &self.eval
    }

    fn dispatch(&mut self, intent: Intent) {
        match intent {
            Intent::Navigate(s) => self.screen = s,
            Intent::TogglePlay => self.playback.is_playing = !self.playback.is_playing,
            Intent::Seek(t) => {
                self.playback.current_time = t.clamp(0.0, self.playback.stats.duration_s)
            }
            Intent::StepFrames(n) => {
                let fps = self.playback.stats.fps.max(1e-6);
                self.playback.current_time =
                    (self.playback.current_time + n as f64 / fps).clamp(0.0, self.playback.stats.duration_s);
            }
            Intent::SetSpeed(v) => self.playback.speed = v.clamp(0.05, 8.0),
            Intent::NewRecordingSession => self.screen = Screen::Record,
            Intent::OpenDataset(id) => {
                self.playback.dataset_name = id;
                self.screen = Screen::Play;
            }

            Intent::WizardAdvance => {
                if let Some(i) = self
                    .hardware
                    .steps
                    .iter()
                    .position(|s| s.state == StepState::Active)
                {
                    self.hardware.steps[i].state = StepState::Done;
                    if let Some(next) = self.hardware.steps.get_mut(i + 1) {
                        next.state = StepState::Active;
                    }
                }
            }
            Intent::WizardRestartStep => {
                for j in &mut self.hardware.joints {
                    j.progress = JointProgress::NotStarted;
                    j.swept = 0.0;
                }
            }

            Intent::RecordStart => self.record.recording = true,
            Intent::RecordStop => self.record.recording = false,
            Intent::SaveEpisode => {
                self.record.saved += 1;
                self.record.episode_index += 1;
                self.record.last_take = None;
            }
            Intent::DiscardLast => {
                self.record.discarded += 1;
                self.record.last_take = None;
            }
            Intent::ReRecord => self.record.last_take = None,
            Intent::SetSoundCues(on) => self.record.sound_cues = on,

            Intent::SelectEpisode(idx) => self.playback.selected = Some(idx),
            Intent::TagEpisode { episode, tag } => {
                if let Some(e) = self.playback.episodes.iter_mut().find(|e| e.index == episode) {
                    e.tag = tag;
                }
            }
            Intent::DeleteEpisode(idx) => {
                self.playback.episodes.retain(|e| e.index != idx);
            }

            Intent::StopRollout => self.eval.driving = false,

            Intent::RescanDatasets | Intent::PullFromHub | Intent::PushToHub => {}
        }
    }
}

// ============================================================================
// Fixtures — transcribed from docs/ux/ux-01-library.png
// ============================================================================

fn dataset(
    name: &str,
    episodes: u32,
    fps: f64,
    robot: &str,
    size_gb: f64,
    good: u32,
    bad: u32,
    sync: SyncState,
) -> DatasetSummary {
    DatasetSummary {
        id: name.to_string(),
        name: name.to_string(),
        episodes,
        fps,
        robot: robot.to_string(),
        size_gb,
        good,
        bad,
        sync,
        thumbnail: None,
        path: PathBuf::from(format!("dataset/{name}")),
    }
}

fn library_fixture() -> LibraryState {
    use SyncState::{LocalOnly, Synced};
    LibraryState {
        datasets: vec![
            dataset("aimee-0720", 206, 10.0, "SO-101", 1.2, 184, 22, Synced),
            dataset("so101-pickplace", 154, 30.0, "SO-101", 0.9, 132, 22, LocalOnly),
            dataset("aloha-fold-towels", 276, 50.0, "ALOHA bimanual", 4.1, 238, 38, Synced),
            dataset("pusht-demo", 198, 10.0, "PushT sim", 0.3, 168, 30, LocalOnly),
            dataset("koch-stack-cubes", 247, 30.0, "Koch v1.1", 1.4, 208, 39, Synced),
            dataset("so101-drawer-open", 89, 30.0, "SO-101", 0.6, 71, 18, LocalOnly),
        ],
        devices: vec![
            DeviceStatus {
                name: "SO-101 follower".into(),
                detail: "/dev/tty.usb31".into(),
                kind: DeviceKind::Robot,
                online: true,
            },
            DeviceStatus {
                name: "cam_high".into(),
                detail: "1080p30".into(),
                kind: DeviceKind::Camera,
                online: true,
            },
        ],
        sessions: vec![
            SessionSummary {
                dataset: "aimee-0720".into(),
                started: "2025-05-20 14:32".into(),
                episodes: 36,
                outcome: SessionOutcome::Completed,
                thumbnail: None,
            },
            SessionSummary {
                dataset: "aloha-fold-towels".into(),
                started: "2025-05-19 11:05".into(),
                episodes: 52,
                outcome: SessionOutcome::Completed,
                thumbnail: None,
            },
            SessionSummary {
                dataset: "so101-drawer-open".into(),
                started: "2025-05-18 16:48".into(),
                episodes: 28,
                outcome: SessionOutcome::InProgress,
                thumbnail: None,
            },
        ],
        busy: None,
    }
}

// ---------------------------------------------------------------- hardware --

fn hardware_fixture() -> HardwareState {
    use JointProgress::{Done, NotStarted, Partial};
    use StepState::{Active, Pending};

    let step = |t: &str, s: StepState| WizardStep {
        title: t.into(),
        state: s,
    };
    let joint = |n: &str, lo: f32, hi: f32, cur: f32, swept: f32, p: JointProgress| {
        JointCalibration {
            name: n.into(),
            min_deg: lo,
            max_deg: hi,
            current_deg: cur,
            swept,
            progress: p,
        }
    };

    HardwareState {
        robot_label: "SO-101".into(),
        steps: vec![
            step("Find port", StepState::Done),
            step("Motors", StepState::Done),
            step("Calibrate", Active),
            step("Cameras", Pending),
            step("Save profile", Pending),
        ],
        joints: vec![
            joint("shoulder_pan", -180.0, 180.0, -12.4, 1.0, Done),
            joint("shoulder_lift", -135.0, 135.0, 43.7, 1.0, Done),
            joint("elbow_flex", -150.0, 0.0, -78.2, 1.0, Done),
            joint("wrist_flex", -120.0, 120.0, -34.1, 1.0, Done),
            joint("wrist_roll", -180.0, 180.0, 91.6, 0.34, Partial),
            joint("gripper", 0.0, 60.0, 18.9, 0.04, NotStarted),
        ],
        live_angles: vec![-0.21, 0.76, -1.36, -0.59, 1.59, 0.33],
        active_joint: Some(4),
        instruction: "Move every joint through its full range".into(),
    }
}

// ------------------------------------------------------------------ record --

fn record_fixture() -> RecordState {
    let limits: [(f32, f32); 6] = [
        (-2.967, 2.967),
        (-2.094, 2.094),
        (-2.967, 2.967),
        (-3.141, 3.141),
        (-2.094, 2.094),
        (-3.141, 3.141),
    ];
    let values = [0.123f32, -0.456, 1.234, -1.111, 0.789, 2.345];

    RecordState {
        profile_label: "SO-101 · 2 cams · profile: bench-rig".into(),
        task: "Pick the lego block and place it in the box".into(),
        episode_index: 23,
        episode_target: 50,
        elapsed_s: 12.4,
        saved: 21,
        discarded: 2,
        recording: true,
        sound_cues: true,
        cameras: vec!["cam_high".into(), "cam_wrist".into()],
        joints: (0..6)
            .map(|i| LiveJoint {
                name: format!("J{}", i + 1),
                value: values[i],
                min: limits[i].0,
                max: limits[i].1,
                history: wave(120, 3.0 + i as f32 * 0.7, i as f32 * 1.1, 0.22, values[i]),
            })
            .collect(),
        last_take: Some(TakeReview {
            thumbnails: Vec::new(),
            verdict: TakeVerdict::ReadyToSave,
            warnings: Vec::new(),
        }),
    }
}

// ---------------------------------------------------------------- playback --

fn playback_fixture() -> PlaybackState {
    let ep = |index: u64, group: &str, dur: f64, tag: Option<Tag>| EpisodeEntry {
        index,
        task_group: group.into(),
        duration_s: dur,
        tag,
    };
    let g = Some(Tag::Good);
    let b = Some(Tag::Bad);

    PlaybackState {
        dataset_name: "aimee-0720".into(),
        episodes: vec![
            ep(0, "assemble_kit", 14.2, g),
            ep(1, "assemble_kit", 15.8, g),
            ep(2, "assemble_kit", 13.7, b),
            ep(3, "assemble_kit", 17.0, g),
            ep(4, "assemble_kit", 16.1, g),
            ep(5, "assemble_kit", 18.3, b),
            ep(6, "pick_place", 12.9, g),
            ep(7, "pick_place", 13.4, g),
            ep(8, "pick_place", 11.2, b),
            ep(9, "pick_place", 15.6, g),
            ep(10, "drawer_open", 19.6, g),
            ep(11, "drawer_open", 21.3, b),
            ep(12, "drawer_open", 18.9, g),
        ],
        selected: Some(4),
        stats: EpisodeStats {
            frames: 161,
            duration_s: 16.1,
            fps: 10.0,
            drift_frames: 0.3,
            task: "Assemble the kit by inserting the blue peg into the board \
                   then tightening the two screws."
                .into(),
            state_channels: 6,
            action_channels: 6,
        },
        // Drift stays flat then grows near the end — the shape the strip shows.
        drift_series: (0..161)
            .map(|i| {
                let t = i as f64 / 160.0;
                if t < 0.8 { 0.02 } else { (t - 0.8) * 3.0 }
            })
            .collect(),
        state_names: vec![
            "shoulder_pan".into(),
            "shoulder_lift".into(),
            "elbow_flex".into(),
            "wrist_flex".into(),
            "wrist_roll".into(),
            "gripper".into(),
        ],
        action_names: vec![
            "shoulder_pan".into(),
            "shoulder_lift".into(),
            "elbow_flex".into(),
            "wrist_flex".into(),
            "wrist_roll".into(),
            "gripper".into(),
        ],
        // A reach-and-return sweep per joint, with the command leading the
        // measurement — the shape the design render shows.
        state_series: plot_fixture(0.0),
        action_series: plot_fixture(0.06),
        // ^ lead is a phase, not an offset: a constant gap would make
        //   action-minus-state a flat line and the tracking-error view
        //   degenerate.
        // The design fixtures have no media behind them; the transport is
        // parked so the mock screens render a still, defined frame.
        current_time: 0.0,
        is_playing: false,
        speed: 1.0,
        video_paths: BTreeMap::new(),
        video_frame_offset: 0,
        robot_urdf: None,
        joint_frames: Vec::new(),
    }
}

/// Six phase-shifted traces over the fixture's 16.1s. `lead` advances the
/// command ahead of the measurement in phase, the way a controller runs.
fn plot_fixture(lead: f64) -> Vec<PlotChannel> {
    const JOINTS: [&str; 6] = [
        "shoulder_pan",
        "shoulder_lift",
        "elbow_flex",
        "wrist_flex",
        "wrist_roll",
        "gripper",
    ];
    JOINTS
        .iter()
        .enumerate()
        .map(|(c, name)| PlotChannel {
            name: (*name).into(),
            points: (0..161)
                .map(|i| {
                    let t = i as f64 / 10.0;
                    let ph = c as f64 * 0.6;
                    let v = 0.8 * (t * 0.39 + ph + lead).sin() * (1.0 - 0.08 * c as f64);
                    (t, v)
                })
                .collect(),
        })
        .collect()
}

// -------------------------------------------------------------------- eval --

fn eval_fixture() -> EvalState {
    let names = [
        "base_rot",
        "shoulder_lift",
        "elbow_flex",
        "wrist_flex",
        "wrist_rot",
        "gripper",
    ];
    let joints = names
        .iter()
        .enumerate()
        .map(|(i, n)| {
            let measured = wave(180, 1.4 + i as f32 * 0.3, i as f32 * 0.8, 34.0, 0.0);
            // wrist_flex is the diverging joint: commanded pulls away late.
            let commanded = if i == 3 {
                measured
                    .iter()
                    .enumerate()
                    .map(|(k, v)| {
                        let t = k as f32 / 180.0;
                        v + if t > 0.55 { (t - 0.55) * 24.0 } else { 0.0 }
                    })
                    .collect()
            } else {
                measured.iter().map(|v| v * 0.96 + 1.2).collect()
            };
            JointDivergence {
                name: n.to_string(),
                measured,
                commanded,
                delta_deg: if i == 3 { 9.8 } else { 1.1 + i as f32 * 0.2 },
                alarm: i == 3,
            }
        })
        .collect();

    EvalState {
        checkpoint: "act-so101-pickplace.safetensors".into(),
        driving: true,
        inference_ms: 18.0,
        joints,
        runs: vec![
            RolloutRun { id: 7, success: true, duration_s: 14.2, note: None },
            RolloutRun {
                id: 6,
                success: false,
                duration_s: 0.0,
                note: Some("timeout".into()),
            },
            RolloutRun { id: 5, success: true, duration_s: 12.8, note: None },
        ],
        session_success: 6,
        session_total: 8,
        predicted_path: (0..24)
            .map(|i| {
                let t = i as f32 / 23.0;
                [0.18 + t * 0.22, 0.30 - t * t * 0.20, -0.04 + t * 0.10]
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixtures_match_the_design_renders() {
        let b = MockBackend::new();
        assert_eq!(b.library().datasets.len(), 6);
        assert_eq!(b.library().datasets[0].meta_line(), "206 ep · 10 fps · SO-101 · 1.2 GB");
        assert_eq!(b.hardware().joints_done(), 4);
        assert!(!b.hardware().can_continue());
        assert_eq!(b.record().counter_label(), "EP 23 / 50");
        assert_eq!(b.record().elapsed_label(), "00:12.4");
        assert_eq!(b.eval().success_label(), "6/8 success · 75%");
    }

    #[test]
    fn navigation_and_curation_round_trip() {
        let mut b = MockBackend::new();
        b.dispatch(Intent::Navigate(Screen::Record));
        assert_eq!(b.screen(), Screen::Record);
        b.dispatch(Intent::TagEpisode { episode: 0, tag: Some(Tag::Bad) });
        assert_eq!(b.playback().episodes[0].tag, Some(Tag::Bad));
        b.dispatch(Intent::DeleteEpisode(0));
        assert!(b.playback().episodes.iter().all(|e| e.index != 0));
    }
}
