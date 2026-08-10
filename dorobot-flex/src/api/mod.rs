//! Service boundary between the UI and everything behind it.
//!
//! Screens read plain data structs and emit intents; they never talk to a
//! dataset writer, a serial bus, or a policy runner directly. That keeps the
//! UI buildable (and visually verifiable) against [`mock::MockBackend`] long
//! before the real implementations exist.
//!
//! ## Shape
//!
//! - **State** is pulled: `Backend::library()`, `.hardware()`, … return borrowed
//!   snapshots that a widget can read during `draw_walk`.
//! - **Intents** are pushed: [`Intent`] values go through `Backend::dispatch`,
//!   which is the only mutation path. Long-running work (encode, calibration
//!   sweep, inference) happens off-thread and lands in the next snapshot.
//!
//! Real backends replace mock ones one screen at a time; the UI does not change.

pub mod files;
pub mod mock;

use std::collections::BTreeMap;
use std::path::PathBuf;

// ============================================================================
// Library
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncState {
    Synced,
    LocalOnly,
}

impl SyncState {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Synced => "synced",
            Self::LocalOnly => "local only",
        }
    }
}

/// One dataset as shown on a Library card. Metadata only — no frames loaded.
#[derive(Clone, Debug)]
pub struct DatasetSummary {
    pub id: String,
    pub name: String,
    pub episodes: u32,
    pub fps: f64,
    /// Human-facing robot label, e.g. "SO-101", "ALOHA bimanual", "PushT sim".
    pub robot: String,
    pub size_gb: f64,
    /// Curation tallies, from the tag sidecar.
    pub good: u32,
    pub bad: u32,
    pub sync: SyncState,
    /// Cached frame-0 thumbnail; `None` renders a placeholder.
    pub thumbnail: Option<PathBuf>,
    pub path: PathBuf,
}

impl DatasetSummary {
    /// The monospace fact line under the dataset name.
    pub fn meta_line(&self) -> String {
        format!(
            "{} ep · {} fps · {} · {:.1} GB",
            self.episodes, self.fps as u32, self.robot, self.size_gb
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceKind {
    Robot,
    Camera,
}

/// A device in the active rig profile, with live connection state.
#[derive(Clone, Debug)]
pub struct DeviceStatus {
    pub name: String,
    /// Port, resolution, or other identifying detail.
    pub detail: String,
    pub kind: DeviceKind,
    pub online: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionOutcome {
    Completed,
    InProgress,
}

impl SessionOutcome {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Completed => "COMPLETED",
            Self::InProgress => "IN PROGRESS",
        }
    }
}

/// A past or resumable capture session.
#[derive(Clone, Debug)]
pub struct SessionSummary {
    pub dataset: String,
    /// Preformatted for display; the backend owns date formatting.
    pub started: String,
    pub episodes: u32,
    pub outcome: SessionOutcome,
    pub thumbnail: Option<PathBuf>,
}

#[derive(Clone, Debug, Default)]
pub struct LibraryState {
    pub datasets: Vec<DatasetSummary>,
    pub devices: Vec<DeviceStatus>,
    pub sessions: Vec<SessionSummary>,
    /// Set while a scan or Hub transfer is in flight.
    pub busy: Option<String>,
}

// ============================================================================
// Hardware setup & calibration
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepState {
    Done,
    Active,
    Pending,
}

#[derive(Clone, Debug)]
pub struct WizardStep {
    pub title: String,
    pub state: StepState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JointProgress {
    Done,
    Partial,
    NotStarted,
}

impl JointProgress {
    /// Amber hint shown to the right of an incomplete joint row.
    pub fn hint(&self) -> &'static str {
        match self {
            Self::Done => "",
            Self::Partial => "keep moving",
            Self::NotStarted => "not started",
        }
    }
}

/// One joint's calibration row: learned range, live position, completion.
#[derive(Clone, Debug)]
pub struct JointCalibration {
    pub name: String,
    pub min_deg: f32,
    pub max_deg: f32,
    pub current_deg: f32,
    /// Fraction of the expected sweep observed so far, 0.0..=1.0.
    pub swept: f32,
    pub progress: JointProgress,
}

#[derive(Clone, Debug, Default)]
pub struct HardwareState {
    pub robot_label: String,
    pub steps: Vec<WizardStep>,
    pub joints: Vec<JointCalibration>,
    /// Live angles feeding the 3D mirror, in URDF joint order.
    pub live_angles: Vec<f32>,
    /// Index into `joints` currently being moved, for the 3D highlight.
    pub active_joint: Option<usize>,
    pub instruction: String,
    /// `(urdf, assets_dir)` for the mirror, when a model ships for this robot.
    pub robot_urdf: Option<(PathBuf, PathBuf)>,
}

impl HardwareState {
    pub fn joints_done(&self) -> usize {
        self.joints
            .iter()
            .filter(|j| j.progress == JointProgress::Done)
            .count()
    }

    /// The wizard only advances once every joint has been swept.
    pub fn can_continue(&self) -> bool {
        !self.joints.is_empty() && self.joints_done() == self.joints.len()
    }
}

// ============================================================================
// Recording
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TakeVerdict {
    ReadyToSave,
    Warning,
}

/// The just-finished episode, staged and awaiting save/discard.
#[derive(Clone, Debug)]
pub struct TakeReview {
    /// Evenly spaced filmstrip frames from the staged take.
    pub thumbnails: Vec<PathBuf>,
    pub verdict: TakeVerdict,
    /// Integrity findings; empty when the take is clean.
    pub warnings: Vec<String>,
}

/// One live joint trace in the recording console.
#[derive(Clone, Debug)]
pub struct LiveJoint {
    pub name: String,
    pub value: f32,
    pub min: f32,
    pub max: f32,
    /// Rolling window, oldest first.
    pub history: Vec<f32>,
}

#[derive(Clone, Debug, Default)]
pub struct RecordState {
    pub profile_label: String,
    pub task: String,
    pub episode_index: u32,
    pub episode_target: u32,
    pub elapsed_s: f64,
    pub saved: u32,
    pub discarded: u32,
    pub recording: bool,
    pub sound_cues: bool,
    pub cameras: Vec<String>,
    pub joints: Vec<LiveJoint>,
    pub last_take: Option<TakeReview>,
}

impl RecordState {
    pub fn progress(&self) -> f64 {
        if self.episode_target == 0 {
            return 0.0;
        }
        self.saved as f64 / self.episode_target as f64
    }

    pub fn counter_label(&self) -> String {
        format!("EP {} / {}", self.episode_index, self.episode_target)
    }

    pub fn elapsed_label(&self) -> String {
        format!("{:02}:{:04.1}", (self.elapsed_s / 60.0) as u32, self.elapsed_s % 60.0)
    }
}

// ============================================================================
// Playback & curation
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tag {
    Good,
    Bad,
}

impl Tag {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Good => "good",
            Self::Bad => "bad",
        }
    }
}

#[derive(Clone, Debug)]
pub struct EpisodeEntry {
    pub index: u64,
    pub task_group: String,
    pub duration_s: f64,
    pub tag: Option<Tag>,
}

/// Per-episode facts shown in the inspector, including sync health.
#[derive(Clone, Debug, Default)]
pub struct EpisodeStats {
    pub frames: u64,
    pub duration_s: f64,
    pub fps: f64,
    /// Mean `timestamp − frame_index/fps`, in frames. Non-zero means the video
    /// and the parquet disagree about when things happened.
    pub drift_frames: f64,
    pub task: String,
    pub state_channels: usize,
    pub action_channels: usize,
}

#[derive(Clone, Debug, Default)]
pub struct PlaybackState {
    pub dataset_name: String,
    pub episodes: Vec<EpisodeEntry>,
    pub selected: Option<u64>,
    pub stats: EpisodeStats,
    /// Per-frame drift for the timeline strip.
    pub drift_series: Vec<f64>,
    /// Channel names from `features.names`, used for plot legends.
    pub state_names: Vec<String>,
    pub action_names: Vec<String>,
    /// Measured and commanded values for the selected episode, channel-major
    /// and paired with the frame's own timestamp, so the plot shows the
    /// recorded timing rather than assuming `frame / fps`.
    pub state_series: Vec<PlotChannel>,
    pub action_series: Vec<PlotChannel>,

    // ---- transport -------------------------------------------------------
    /// Playhead, in seconds from the start of the episode.
    pub current_time: f64,
    pub is_playing: bool,
    pub speed: f64,
    /// Camera key -> video file backing the selected episode.
    pub video_paths: BTreeMap<String, PathBuf>,
    /// Where this episode starts inside a v3.0 concatenated video file.
    pub video_frame_offset: u64,
    /// `(urdf, assets_dir)` for this dataset's robot, when one ships with it.
    pub robot_urdf: Option<(PathBuf, PathBuf)>,
    /// Measured joint values per frame, for the 3D mirror. Frame-major, unlike
    /// the plot series, because the viewer wants one pose at a time.
    pub joint_frames: Vec<Vec<f32>>,
}

impl PlaybackState {
    /// Frame the playhead currently sits on.
    pub fn frame_index(&self) -> u64 {
        (self.current_time * self.stats.fps).max(0.0) as u64
    }

    /// Pose under the playhead, for the robot viewer.
    pub fn joints_now(&self) -> Option<&Vec<f32>> {
        self.joint_frames.get(self.frame_index() as usize)
    }

    pub fn time_label(&self) -> String {
        format!("{:.1}s / {:.1}s", self.current_time, self.stats.duration_s)
    }
}

/// One plotted channel: a name and its `(seconds, value)` samples.
#[derive(Clone, Debug, Default)]
pub struct PlotChannel {
    pub name: String,
    pub points: Vec<(f64, f64)>,
}

// ============================================================================
// Policy rollout
// ============================================================================

#[derive(Clone, Debug)]
pub struct JointDivergence {
    pub name: String,
    pub measured: Vec<f32>,
    pub commanded: Vec<f32>,
    pub delta_deg: f32,
    /// Set when `delta_deg` crosses the alarm threshold; drives red shading.
    pub alarm: bool,
}

#[derive(Clone, Debug)]
pub struct RolloutRun {
    pub id: u32,
    pub success: bool,
    pub duration_s: f64,
    /// Failure reason, e.g. "timeout".
    pub note: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct EvalState {
    pub checkpoint: String,
    pub driving: bool,
    pub inference_ms: f64,
    pub joints: Vec<JointDivergence>,
    /// Most recent runs, newest first — the ledger is windowed for display.
    pub runs: Vec<RolloutRun>,
    /// Session totals, which outlive the visible window.
    pub session_success: u32,
    pub session_total: u32,
    /// Predicted end-effector path for the 3D ghost ribbon.
    pub predicted_path: Vec<[f32; 3]>,
}

impl EvalState {
    pub fn success_label(&self) -> String {
        let pct = if self.session_total == 0 {
            0
        } else {
            self.session_success * 100 / self.session_total
        };
        format!(
            "{}/{} success · {}%",
            self.session_success, self.session_total, pct
        )
    }
}

// ============================================================================
// Screens & intents
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Screen {
    Library,
    Hardware,
    Record,
    Play,
    Eval,
}

impl Screen {
    pub const ALL: [Screen; 5] = [
        Screen::Library,
        Screen::Hardware,
        Screen::Record,
        Screen::Play,
        Screen::Eval,
    ];

    pub fn label(&self) -> &'static str {
        match self {
            Self::Library => "Library",
            Self::Hardware => "Hardware",
            Self::Record => "Record",
            Self::Play => "Play",
            Self::Eval => "Eval",
        }
    }
}

/// Every mutation the UI can request. One enum keeps the surface auditable and
/// makes the whole app scriptable (and testable) without synthetic input.
#[derive(Clone, Debug)]
pub enum Intent {
    Navigate(Screen),

    // Library
    RescanDatasets,
    OpenDataset(String),
    PullFromHub,
    NewRecordingSession,

    // Hardware
    WizardAdvance,
    WizardRestartStep,

    // Record
    RecordStart,
    RecordStop,
    SaveEpisode,
    DiscardLast,
    ReRecord,
    SetSoundCues(bool),

    // Playback transport
    TogglePlay,
    /// Absolute seek, in seconds; clamped to the episode by the backend.
    Seek(f64),
    StepFrames(i32),
    SetSpeed(f64),

    // Playback & curation
    SelectEpisode(u64),
    TagEpisode { episode: u64, tag: Option<Tag> },
    DeleteEpisode(u64),
    PushToHub,

    // Eval
    StopRollout,
}

/// What every screen reads and writes. Implemented by [`mock::MockBackend`]
/// today; real backends drop in per screen without UI changes.
pub trait Backend {
    fn screen(&self) -> Screen;
    fn library(&self) -> &LibraryState;
    fn hardware(&self) -> &HardwareState;
    fn record(&self) -> &RecordState;
    fn playback(&self) -> &PlaybackState;
    fn eval(&self) -> &EvalState;

    /// Apply an intent. Must be cheap and non-blocking — anything slow is
    /// spawned and surfaced through a later snapshot.
    fn dispatch(&mut self, intent: Intent);

    /// Called once per frame so backends can drain worker channels.
    fn poll(&mut self) {}
}
