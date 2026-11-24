use dorobot_studio_core::{
    api::{
        aisle::{
            Action, Planner, RevisionSide, RgbTiming, Robot, TargetMotion, Workbench, WorkerAction,
            REFERENCE,
        },
        Backend, Intent,
    },
    data::robot_model::RobotModelLoader,
    ui::frame::{self, Themed},
    widgets::video_player::VideoPlayerWidgetRefExt,
};
use dorobot_studio_plugin::*;
use dorobot_ux::kit::SparkWidgetRefExt;
use makepad_urdf_player::robot_view::RobotViewWidgetRefExt;
use std::path::PathBuf;
mod joint_inspector;

pub struct AislePlugin;
const PAGES: &[PanelDescriptor] = &[
    PanelDescriptor {
        id: "aisle_task",
        title: "Task",
        legacy_screen: None,
    },
    PanelDescriptor {
        id: "aisle_scene",
        title: "Scene",
        legacy_screen: None,
    },
    PanelDescriptor {
        id: "aisle_run",
        title: "Run",
        legacy_screen: None,
    },
    PanelDescriptor {
        id: "aisle_evidence",
        title: "Evidence",
        legacy_screen: None,
    },
];
impl StudioPlugin for AislePlugin {
    fn descriptor(&self) -> PluginDescriptor {
        PluginDescriptor {
            id: "dorobot.aisle",
            title: "AISLE Studio",
            api_version: API_VERSION,
            panels: PAGES,
        }
    }
    fn register(&self, vm: &mut ScriptVm) {
        script_mod(vm);
    }
    fn create(&self, panel: &str, vm: &mut ScriptVm) -> Result<Box<dyn PluginPanel>, String> {
        let value = match panel {
            "aisle_task" => script_eval!(vm,{use mod.widgets.* AisleTask{}}),
            "aisle_scene" => script_eval!(vm,{use mod.widgets.* AisleScene{}}),
            "aisle_run" => script_eval!(vm,{use mod.widgets.* AisleRun{}}),
            "aisle_evidence" => script_eval!(vm,{use mod.widgets.* AisleEvidence{}}),
            _ => return Err(format!("Unknown workbench page {panel}")),
        };
        Ok(Box::new(Panel {
            root: WidgetRef::script_from_value(vm, value),
            page: panel.into(),
            model: Default::default(),
            light: None,
            frame: None,
            draft: None,
            room_preview: None,
            decision_preview: None,
            detailed_trial: None,
            joint_robot: None,
            joint_index: 0,
        }))
    }
    fn can_disable(&self, b: &dyn Backend) -> Result<(), String> {
        if b.aisle().is_some_and(|w| w.busy()) {
            Err("Stop the active AISLE job first".into())
        } else {
            Ok(())
        }
    }
}
script_mod! {
    use mod.prelude.widgets.*
    use mod.widgets.*
    let Title=Label{width:Fill text:"" draw_text +:{text_style:mod.widgets.ux.TEXT_TITLE{}}}
    let Section=Label{width:Fill text:"" draw_text +:{text_style:mod.widgets.ux.TEXT_META{}}}
    let Body=Label{width:Fill max_lines:12 text:""}
    let Card=mod.widgets.ux.Card{padding:16.0 spacing:10.0 flow:Down}
    let Bar=View{width:Fill height:Fit flow:Right spacing:8.0 align:Align{y:0.5}}
    let Page=View{width:Fill height:Fill flow:Down padding:16.0 spacing:12.0}
    mod.widgets.AisleTask=Page{
        Title{text:"Robot task"}
        context_label := Section{}
        row := View{width:Fill height:Fill flow:Right spacing:12.0
            editor := Card{width:Fill height:Fill
                Section{text:"Robot profile"}
                Bar{nano := mod.widgets.ux.ToolButton{text:"Adora Nano"} g1 := mod.widgets.ux.ToolButton{text:"Unitree G1"}}
                Section{text:"Task instruction"}
                instruction := TextInput{width:Fill height:64 is_multiline:true empty_text:"Inspect a shoulder or approach a colored cube with Nano"}
                Bar{reference := mod.widgets.ux.ToolButton{text:"Use reference task"} onboard := mod.widgets.ux.ToolButton{text:"Onboard RGB off"}}
                Bar{octos := mod.widgets.ux.PrimaryButton{text:"DeepSeek via Octos"} codex := mod.widgets.ux.ToolButton{text:"Ask Codex"}}
                Section{text:"Prepared controller"}
                controller_scroll := ScrollYView{width:Fill height:Fill flow:Down
                    detail := Body{max_lines:0}
                }
                Bar{older := mod.widgets.ux.ToolButton{text:"Earlier revision"} newer := mod.widgets.ux.ToolButton{text:"Later revision"}}
                revision_actions := Bar{revise := mod.widgets.ux.ToolButton{text:"Revise with DeepSeek"} revise_codex := mod.widgets.ux.ToolButton{text:"Revise with Codex"}}
            }
            model_card := Card{width:Fill height:Fill Bar{preview_title := Section{text:"Robot preview"} review_controller := mod.widgets.ux.ToolButton{text:"Review changes"}}
                author_inputs := Body{max_lines:2}
                diff_scroll := ScrollYView{width:Fill height:Fill flow:Down controller_changes := Body{max_lines:0}}
                robot := RobotView{width:Fill height:Fill keyboard_controls:false} onboard_photo := VideoPlayer{width:Fill height:Fill}}
        }
        status := Body{}
    }
    mod.widgets.AisleScene=Page{
        Title{text:"Simulation scene"} context_label := Section{}
        scene_modes := Bar{robot_mode := mod.widgets.ux.ToolButton{text:"Robot model"} joint_mode := mod.widgets.ux.ToolButton{text:"Joint limits"} room_mode := mod.widgets.ux.ToolButton{text:"Room survey"}}
        row := View{width:Fill height:Fill flow:Right spacing:12.0
            model_card := Card{width:Fill height:Fill Section{text:"URDF model"} robot := RobotView{width:Fill height:Fill keyboard_controls:false tint_selected_links:true}}
            photo_card := Card{width:Fill height:Fill photo_label := Section{text:"Floor footprint"} photo := VideoPlayer{width:Fill height:Fill}}
            joint_card := Card{width:300 height:Fill
                Section{text:"Find a joint"}
                joint_query := TextInput{width:Fill height:38 empty_text:"Filter by joint name"}
                joint_count := Body{}
                Bar{joint_previous := mod.widgets.ux.ToolButton{text:"Previous"} joint_next := mod.widgets.ux.ToolButton{text:"Next"}}
                ScrollYView{width:Fill height:Fill flow:Down spacing:10
                    joint_name := Section{}
                    joint_detail := Body{max_lines:0}
                }
            }
            survey_card := ScrollYView{width:295 height:Fill flow:Down spacing:8 padding:10
                Section{text:"Room input file"} survey_path := TextInput{width:Fill height:38 empty_text:"Path to a survey or photo input"}
                import_survey := mod.widgets.ux.PrimaryButton{text:"Import survey"}
                Section{text:"Draft from reference photo"}
                Bar{photo_octos := mod.widgets.ux.ToolButton{text:"DeepSeek"} photo_codex := mod.widgets.ux.ToolButton{text:"Codex"}}
                room_detail := Body{}
                draft_actions := Bar{apply_draft := mod.widgets.ux.PrimaryButton{text:"Use this room"} discard_draft := mod.widgets.ux.ToolButton{text:"Discard draft"}}
                room_edits := View{width:Fill height:Fit flow:Down spacing:6
                Section{text:"Edit obstacle · metres"} geometry := TextInput{width:Fill height:38 empty_text:"name x y height"}
                save_geometry := mod.widgets.ux.ToolButton{text:"Save revision"}
                Section{text:"Ambient light · R G B (0–1)"}
                room_ambient := TextInput{width:Fill height:34 empty_text:"Use scene default"}
                Section{text:"Floor friction (0–2)"}
                room_friction := TextInput{width:Fill height:34 empty_text:"Use scene default"}
                save_environment := mod.widgets.ux.ToolButton{text:"Save environment"}
                default_room := mod.widgets.ux.ToolButton{text:"Use default room"}}
            }
            inspector := Card{width:285 height:Fill padding:0.0
                fixture_scroll := ScrollYView{width:Fill height:Fill flow:Down padding:16.0 spacing:10.0
                Section{text:"Simulation fixture"} fixture_kind := Body{}
                fixed_fixtures := Bar{standard := mod.widgets.ux.ToolButton{text:"Standard"} compliant := mod.widgets.ux.ToolButton{text:"Compliant"}}
                standing_fixtures := Bar{
                    level := mod.widgets.ux.ToolButton{text:"Level floor"}
                    impact := mod.widgets.ux.ToolButton{text:"Impact test"}
                }
                fixture := Body{}
                camera_profile := View{width:Fill height:Fit flow:Down spacing:6.0
                    Section{text:"Onboard camera rendering"}
                    Bar{original_visuals := mod.widgets.ux.ToolButton{text:"Original"} compact_visuals := mod.widgets.ux.ToolButton{text:"Compact"}}
                }
                layouts := View{width:Fill height:Fit flow:Down spacing:6.0
                    Bar{spacing:4.0 seed_title := Section{} layout0 := mod.widgets.ux.ToolButton{text:"0"} layout1 := mod.widgets.ux.ToolButton{text:"1"} layout2 := mod.widgets.ux.ToolButton{text:"2"}}
                    Bar{seed_input := TextInput{width:Fill height:32 empty_text:"Enter a layout seed"} apply_seed := mod.widgets.ux.ToolButton{text:"Set seed"}}
                }
                rgb_delivery := View{width:Fill height:Fit flow:Down spacing:6.0
                    Section{text:"Cube motion · world Y"}
                    Bar{still := mod.widgets.ux.ToolButton{text:"Still"} slide_pos := mod.widgets.ux.ToolButton{text:"+Y"} slide_neg := mod.widgets.ux.ToolButton{text:"−Y"}}
                    timing_controls := View{width:Fill height:Fit flow:Down spacing:6.0
                    timing_title := Section{text:"Onboard camera timing"}
                    Bar{on_time := mod.widgets.ux.ToolButton{text:"On time"} late_rgb := mod.widgets.ux.ToolButton{text:"Delay"} frozen_rgb := mod.widgets.ux.ToolButton{text:"Freeze"}}}
                }
                inspect := mod.widgets.ux.PrimaryButton{text:"Inspect model"} Section{text:"Model and motion limits"} detail := Body{}
                }
            }
        }
        status := Body{}
    }
    mod.widgets.AisleRun=Page{
        Title{text:"Live simulation"} context_label := Section{}
        Bar{local_run := mod.widgets.ux.ToolButton{text:"Simulation"} workers := mod.widgets.ux.ToolButton{text:"Workers"}}
        row := View{width:Fill height:Fill flow:Right spacing:12.0
            viewport := Card{width:Fill height:Fill camera_label := Section{} camera := VideoPlayer{width:Fill height:Fill}}
            fleet_controls := Card{width:310 height:Fill padding:0
                fleet_scroll := ScrollYView{width:Fill height:Fill flow:Down padding:12 spacing:6
                    Section{text:"Worker connection"}
                    node := TextInput{width:Fill height:34 empty_text:"local or configured SSH worker"}
                    Bar{connect := mod.widgets.ux.ToolButton{text:"Connect"} refresh := mod.widgets.ux.ToolButton{text:"Refresh"}}
                    capacity := Body{}
                    Bar{clock := mod.widgets.ux.ToolButton{text:"Sim time"} period := mod.widgets.ux.ToolButton{text:"RGB / 1 s"}}
                    Bar{queue_stream := mod.widgets.ux.PrimaryButton{text:"Stream run"} queue_background := mod.widgets.ux.ToolButton{text:"Background run"}}
                    Section{text:"Selected job"} job_detail := Body{}
                    Bar{older_job := mod.widgets.ux.ToolButton{text:"Previous"} newer_job := mod.widgets.ux.ToolButton{text:"Next"} sample_job := mod.widgets.ux.ToolButton{text:"Snapshot"}}
                    Bar{cancel_job := mod.widgets.ux.StopButton{text:"Cancel job"} fetch_job := mod.widgets.ux.ToolButton{text:"Fetch evidence"}}
                }
            }
            inspector := Card{width:275 height:Fill measure_label := Section{} detail := Body{}
                start_run := mod.widgets.ux.PrimaryButton{text:"Start run"} stop_run := mod.widgets.ux.StopButton{text:"Stop"}
            }
        }
        status := Body{}
    }
    mod.widgets.AisleEvidence=Page{
        Title{text:"Run evidence"} context_label := Section{}
        Bar{trial_mode := mod.widgets.ux.ToolButton{text:"Trial"} evaluation_mode := mod.widgets.ux.ToolButton{text:"Batch evaluation"} device_mode := mod.widgets.ux.ToolButton{text:"Device preflight"} room_baseline := mod.widgets.ux.ToolButton{text:"Use as room baseline"} room_details := mod.widgets.ux.ToolButton{text:"Trial details"}}
        device := Card{width:Fill height:Fill
            device_title := Title{}
            device_info := Body{}
            device_endpoint := TextInput{width:Fill height:34 empty_text:"Network interface and DDS domain, e.g. en0@0"}
            Bar{device_replay := mod.widgets.ux.PrimaryButton{text:"Replay through DDS"} device_listen := mod.widgets.ux.ToolButton{text:"Observe device"} device_stop := mod.widgets.ux.StopButton{text:"Stop"}}
            device_result := Body{}
            device_folder := mod.widgets.ux.ToolButton{text:"Open preflight folder"}
        }
        cohort := Card{width:Fill height:Fill
            Bar{Section{width:90 text:"Evaluation"} archive_path := TextInput{width:Fill height:36 empty_text:"Candidate ZIP beside its suite.json"}}
            Bar{Section{width:90 text:"Baseline"} baseline_path := TextInput{width:Fill height:36 empty_text:"Optional ZIP to compare against the candidate"}}
            Bar{verify_archive := mod.widgets.ux.PrimaryButton{text:"Verify evaluation"} compare_archives := mod.widgets.ux.ToolButton{text:"Compare with baseline"}}
            cohort_verdict := Title{} cohort_detail := Body{}
            failure_bar := Bar{earlier_failure := mod.widgets.ux.ToolButton{text:"Previous failure"} later_failure := mod.widgets.ux.ToolButton{text:"Next failure"}}
            ScrollYView{width:Fill height:Fill flow:Down failure_detail := Body{max_lines:64}}
        }
        summary := Card{width:Fill height:Fit summary_title := Section{text:"Task outcome"} verdict := Title{} detail := Body{}}
        plot := Card{width:Fill height:Fill trace_label := Section{} trace := Spark{width:Fill height:Fill} scale := Section{}}
        decision_card := Card{width:Fill height:Fill
            decision_title := Title{}
            Body{text:"Camera order from left to right · Head, left wrist, right wrist"}
            prev_label := Section{} prev_rgb := VideoPlayer{width:Fill height:Fill}
            curr_label := Section{} curr_rgb := VideoPlayer{width:Fill height:Fill}
        }
        change_bar := Bar{
            decision_rgb := mod.widgets.ux.ToolButton{text:"Inspect decision RGB"}
            previous_change := mod.widgets.ux.ToolButton{text:"Previous revision"}
            current_change := mod.widgets.ux.ToolButton{text:"Current revision"}
        }
        trial_bar := Bar{previous := mod.widgets.ux.ToolButton{text:"Previous run"} next := mod.widgets.ux.ToolButton{text:"Next run"}
            folder := mod.widgets.ux.ToolButton{text:"Open run folder"}
            export := mod.widgets.ux.PrimaryButton{text:"Export episode"} replay := mod.widgets.ux.PrimaryButton{text:"Open in Replay"} train_reach := mod.widgets.ux.ToolButton{text:"Train shoulder correction"} train_grasp := mod.widgets.ux.ToolButton{text:"Train grasp detector"}}
        status := Body{}
    }
}
struct Panel {
    root: WidgetRef,
    page: String,
    model: RobotModelLoader,
    light: Option<f64>,
    frame: Option<(PathBuf, u64)>,
    draft: Option<(Robot, Option<PathBuf>)>,
    room_preview: Option<PathBuf>,
    decision_preview: Option<PathBuf>,
    detailed_trial: Option<PathBuf>,
    joint_robot: Option<Robot>,
    joint_index: usize,
}
impl Panel {
    fn label(&self, cx: &mut Cx, id: &[LiveId], s: &str) {
        self.root.label(cx, id).set_text(cx, s);
    }
    fn show_camera(&mut self, cx: &mut Cx, dir: Option<&PathBuf>, buffered: bool) {
        use std::io::Read;
        let camera = self.root.video_player(cx, ids!(camera));
        if self.frame.as_ref().is_some_and(|(old, _)| Some(old) != dir) {
            camera.clear(cx);
            self.frame = None;
        }
        let Some(dir) = dir else { return };
        let last = self.frame.as_ref().map(|(_, tick)| *tick);
        if buffered && last.is_some() && camera.borrow().is_some_and(|v| v.measured_frame_pending()) {
            return;
        }
        let open = |name: &str, recent: bool| {
            let mut file = std::fs::File::open(dir.join(name)).ok()?;
            let mut stamp = [0; 4];
            file.read_exact(&mut stamp).ok()?;
            let tick = u32::from_be_bytes(stamp) as u64;
            if last.is_some_and(|previous| tick <= previous) {
                return None;
            }
            if recent && file.metadata().ok()?.modified().ok()?.elapsed().ok()? > std::time::Duration::from_millis(80) {
                return None;
            }
            Some((tick, file))
        };
        let next = if buffered && last.is_some() {
            open("preview-previous.rgb", true)
        } else {
            None
        };
        let Some((tick, mut file)) = next.or_else(|| open("preview.rgb", false)) else { return };
        // The producer atomically replaces this file. Its header and pixels belong to the same exposure.
        let Ok(meta) = file.metadata() else { return };
        const SMALL: u64 = 4 + 480 * 320 * 3;
        const LARGE: u64 = 4 + 480 * 360 * 3;
        let height = match meta.len() {
            SMALL => 320,
            LARGE => 360,
            _ => return,
        };
        let mut rgb = vec![0; 480 * height * 3];
        if file.read_exact(&mut rgb).is_ok() {
            if let Some(mut view) = camera.borrow_mut() {
                view.set_measured_frame(cx, (480, height), &rgb, tick, dir);
                self.frame = Some((dir.clone(), tick));
            }
        }
    }
    fn camera_title(&self, cx: &mut Cx, title: &str, streaming: bool) {
        if streaming {
            let cadence = self.root.video_player(cx, ids!(camera)).live_cadence();
            self.label(cx, ids!(camera_label), &format!("{title} · {cadence}"));
        } else {
            self.label(cx, ids!(camera_label), title);
        }
    }
    fn model(&mut self, cx: &mut Cx, w: &Workbench) {
        let p = w.robot.model();
        let viewer = self.root.robot_view(cx, ids!(robot));
        if self
            .model
            .request(Some((p.clone(), p.parent().unwrap().into())))
        {
            viewer.clear_robot(cx);
        }
        if let Some(result) = self.model.take_ready() {
            match result {
                Ok(robot) => {
                    if let Some(mut v) = viewer.borrow_mut() {
                        v.set_robot(cx, robot);
                    }
                }
                Err(error) => self.label(cx, ids!(status), &error),
            }
        }
    }
    fn fleet(&mut self, cx: &mut Cx, w: &Workbench) {
        let caps = &w.fleet["capabilities"];
        let jobs = w.fleet["jobs"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        let selected = &w.fleet["selected"];
        let observed = &w.worker_observation["job"];
        let job = if observed["id"].is_string() && observed["id"] == selected["id"] {
            observed
        } else {
            selected
        };
        let state = job["state"].as_str().unwrap_or("");
        let active = ["reserved", "running", "stopping"].contains(&state);
        let stream = selected["mode"] == "stream";
        let cap = if let Some(host) = caps["host"].as_str() {
            format!(
                "{} · {} · {}\n{} of {} slots occupied at refresh",
                w.fleet["node"].as_str().unwrap_or("local"),
                host,
                caps["backend"].as_str().unwrap_or("Nexus"),
                caps["active"].as_array().map_or(0, Vec::len),
                caps["slots"]
            )
        } else {
            "Connect to see available simulation slots.".into()
        };
        self.label(cx, ids!(capacity), &cap);
        let pos = jobs.iter().position(|j| j == &selected["id"]);
        let info = if let Some(i) = pos {
            let outcome = match job["result"]["task_success"].as_bool() {
                Some(true) => " · passed",
                Some(false) => " · failed",
                None => "",
            };
            let ticks = job["result"]["physics_ticks"]
                .as_u64()
                .or_else(|| job["result"]["steps"].as_u64())
                .or_else(|| job["progress"]["step"].as_u64())
                .unwrap_or(0);
            let mode = if stream {
                "foreground".to_owned()
            } else {
                let period = selected["observation_period_s"].as_f64().unwrap_or(1.0);
                let clock = if selected["observation_clock"] == "wall" {
                    "wall"
                } else {
                    "sim"
                };
                format!("background\nRGB every {period} s ({clock} time)")
            };
            format!(
                "{} of {} · {} · {}\n{} · {} physics steps{}",
                i + 1,
                jobs.len(),
                selected["robot"].as_str().unwrap_or("Robot"),
                mode,
                state,
                ticks,
                outcome
            )
        } else {
            "No jobs submitted to this worker.".into()
        };
        self.label(cx, ids!(job_detail), &info);
        self.root.button(cx, ids!(clock)).set_text(
            cx,
            if w.observation_clock.name() == "wall" {
                "Wall time"
            } else {
                "Sim time"
            },
        );
        self.root
            .button(cx, ids!(period))
            .set_text(cx, &format!("RGB / {} s", w.background_period_s));
        let frame = if stream {
            &w.worker_observation["frame"]
        } else {
            &w.fleet["sample"]
        };
        let directory = if stream {
            &w.worker_preview
        } else {
            &w.worker_sample
        };
        self.show_camera(cx, directory.as_ref(), true);
        self.camera_title(
            cx,
            if stream {
                "Foreground worker camera"
            } else if frame.is_object() {
                "Background RGB snapshot"
            } else {
                "Choose Get snapshot to view the latest background RGB image"
            },
            stream && active,
        );
        self.root
            .widget(cx, ids!(sample_job))
            .set_visible(cx, selected["mode"] == "observations");
        let compatible = w.prepared.is_some()
            && caps["robots"][w.robot.name()].is_string()
            && caps["robots"][w.robot.name()] == w.plan["model_hash"];
        for (button, enabled) in [
            (ids!(connect) as &[LiveId], true),
            (ids!(refresh), caps.is_object()),
            (ids!(clock), true),
            (ids!(period), true),
            (
                ids!(sample_job),
                selected["mode"] == "observations" && !state.is_empty(),
            ),
            (ids!(queue_stream), compatible),
            (ids!(queue_background), compatible),
            (ids!(older_job), pos.is_some_and(|i| i > 0)),
            (ids!(newer_job), pos.is_some_and(|i| i + 1 < jobs.len())),
            (ids!(cancel_job), active),
            (
                ids!(fetch_job),
                matches!(state, "completed" | "failed" | "cancelled"),
            ),
        ] {
            self.root
                .button(cx, button)
                .set_enabled(cx, enabled && !w.busy());
        }
    }
    fn run(&mut self, cx: &mut Cx, w: &Workbench) {
        self.root
            .button(cx, ids!(workers))
            .set_enabled(cx, !w.busy() && !w.workers_open);
        self.root
            .button(cx, ids!(local_run))
            .set_enabled(cx, !w.busy() && w.workers_open);
        self.root
            .widget(cx, ids!(fleet_controls))
            .set_visible(cx, w.workers_open);
        self.root
            .widget(cx, ids!(inspector))
            .set_visible(cx, !w.workers_open);
        if w.workers_open {
            self.fleet(cx, w);
            return;
        }
        let standing = w.progress["kind"] == "standing-inspection";
        let transfer = w.progress["kind"] == "table-transfer";
        let mobile = transfer || w.progress["kind"] == "visual-approach";
        self.show_camera(cx, w.live.as_ref(), false);
        self.camera_title(
            cx,
            if mobile {
                "Onboard head camera · RGB"
            } else {
                "External operator camera"
            },
            w.running_local_trial(),
        );
        self.label(
            cx,
            ids!(measure_label),
            if transfer {
                "Table transfer"
            } else if standing {
                "Standing shoulder tracking"
            } else if mobile {
                "Cube approach"
            } else {
                "Measured joint response"
            },
        );
        let text = if transfer {
            format!(
                "{} cube · {}\n{:.1} s simulated",
                w.progress["color"].as_str().unwrap_or(""),
                w.progress["state"].as_str().unwrap_or("Starting"),
                w.progress["step"].as_u64().unwrap_or(1).saturating_sub(1) as f64 * 0.01
            )
        } else if mobile {
            let distance = w.progress["distance_m"]
                .as_f64()
                .map(|v| format!("{v:.3} m"))
                .unwrap_or_else(|| "Target out of view".into());
            let motion = match w.progress["target_motion"].as_str() {
                Some("slide-positive") => "\nCube slides along +Y",
                Some("slide-negative") => "\nCube slides along −Y",
                _ => "",
            };
            format!(
                "{} cube · {}
RGB distance {}
Requested {:.2} m{motion}",
                w.progress["color"].as_str().unwrap_or(""),
                w.progress["state"].as_str().unwrap_or(""),
                distance,
                w.progress["standoff_m"].as_f64().unwrap_or_default()
            )
        } else if standing {
            format!(
                "Shoulder {:+.4} rad · pelvis tilt {:.1}°\n{} · {:.1} s simulated",
                w.progress["measured_rad"].as_f64().unwrap_or_default(),
                w.progress["tilt_deg"].as_f64().unwrap_or_default(),
                if w.progress["both_feet"] == true {
                    "Both feet loaded"
                } else {
                    "Checking foot contact"
                },
                w.progress["step"].as_f64().unwrap_or_default() * 0.005
            )
        } else if let Some(angle) = w.progress["measured_rad"].as_f64() {
            format!("{angle:+.5} rad\nPhysics step {} / 121", w.progress["step"])
        } else {
            "Waiting for a measurement".into()
        };
        self.label(cx, ids!(detail), &text);
        self.root
            .button(cx, ids!(start_run))
            .set_enabled(cx, !w.busy() && w.prepared.is_some());
        self.root
            .widget(cx, ids!(stop_run))
            .set_visible(cx, w.busy());
    }
}
impl PluginPanel for Panel {
    fn root(&self) -> WidgetRef {
        self.root.clone()
    }
    fn sync(&mut self, cx: &mut Cx, context: &PanelContext<'_>) {
        let Some(w) = context.backend.aisle() else {
            self.label(
                cx,
                ids!(status),
                "Open a local Studio workspace to run AISLE",
            );
            return;
        };
        let light = frame::light_mode();
        if self.light != Some(light) {
            frame::apply_light(
                cx,
                &self.root,
                &[
                    (ids!(editor), Themed::Bg),
                    (ids!(inspector), Themed::Bg),
                    (ids!(model_card), Themed::Bg),
                    (ids!(viewport), Themed::Bg),
                    (ids!(fleet_controls), Themed::Bg),
                    (ids!(photo_card), Themed::Bg),
                    (ids!(cohort), Themed::Bg),
                    (ids!(device), Themed::Bg),
                    (ids!(decision_card), Themed::Bg),
                    (ids!(survey_card), Themed::Bg),
                    (ids!(joint_card), Themed::Bg),
                    (ids!(summary), Themed::Bg),
                    (ids!(plot), Themed::Bg),
                ],
                light,
            );
            self.light = Some(light);
        }
        let kind = match self.page.as_str() {
            "aisle_evidence" => &w.evidence["kind"],
            "aisle_run" if !w.progress.is_null() => &w.progress["kind"],
            _ => &w.plan["kind"],
        };
        let standing = kind == "standing-inspection";
        let transfer = kind == "table-transfer";
        let mobile = transfer || kind == "visual-approach";
        self.label(
            cx,
            ids!(context_label),
            &format!(
                "{}   /   AISLE · Nexus   /   {}",
                if self.page == "aisle_evidence" {
                    w.evidence["robot"].as_str().unwrap_or("Run history")
                } else {
                    w.robot.name()
                },
                if transfer && self.page == "aisle_evidence" && w.evidence["visual_profile"] == "compact-v1" {
                    "Living room · compact onboard cameras"
                } else if mobile {
                    "Living room · grounded Nano"
                } else if standing {
                    "G1 · floating-base standing test"
                } else {
                    "Fixed-base simulation"
                }
            ),
        );
        self.label(cx, ids!(status), &w.status);
        match self.page.as_str() {
            "aisle_task" => {
                let capture = w.plan["scene_observation"]["files"]["onboard-head.rgb"].is_string();
                let reviewing = w.controller_diff.is_some();
                let review = self.root.button(cx, ids!(review_controller));
                review.set_enabled(cx, w.prepared.is_some() && !w.busy());
                review.set_text(cx, if reviewing { "Close review" } else { "Review changes" });
                self.root.widget(cx, ids!(diff_scroll)).set_visible(cx, reviewing);
                if let Some(report) = &w.controller_diff {
                    self.label(cx, ids!(controller_changes), report["display"].as_str().unwrap_or(""));
                }
                self.root.widget(cx, ids!(robot)).set_visible(cx, !capture && !reviewing);
                self.root
                    .widget(cx, ids!(onboard_photo))
                    .set_visible(cx, capture && !reviewing);
                self.label(
                    cx,
                    ids!(preview_title),
                    if reviewing {
                        "Controller changes"
                    } else if capture {
                        "Head camera at task authoring"
                    } else {
                        "Robot preview"
                    },
                );
                let inputs = &w.plan["scene_observation"]["author_input"];
                let label = match (w.plan["provider"].as_str(), inputs["images"].as_array()) {
                    (Some("codex"), Some(images)) if images.len() == 3 => "Sent to Codex · Head and both wrist images plus measured positions",
                    (Some("octos-deepseek"), Some(images)) if images.is_empty() => "Sent to DeepSeek · Positions measured from onboard RGB",
                    _ => "",
                };
                self.root.widget(cx, ids!(author_inputs)).set_visible(cx, !reviewing && !label.is_empty());
                self.label(cx, ids!(author_inputs), label);
                let draft = (w.robot, w.prepared.clone());
                if self.draft.as_ref() != Some(&draft) {
                    self.root
                        .text_input(cx, ids!(instruction))
                        .set_text(cx, &w.task);
                    let photo = self.root.video_player(cx, ids!(onboard_photo));
                    photo.clear(cx);
                    if capture {
                        if let Some(raw) = w
                            .prepared
                            .as_ref()
                            .and_then(|p| std::fs::read(p.with_file_name("onboard-head.rgb")).ok())
                        {
                            if raw.len() == 480 * 360 * 3 {
                                photo.set_frame(cx, 480, 360, &raw);
                            }
                        }
                    }
                    self.draft = Some(draft);
                }
                let detail = if w.plan.is_null() {
                    "Choose a reference task or use an agent to prepare a controller.".into()
                } else if transfer {
                    format!(
                        "Move floor cubes to the coffee table{}\n{} · {} · revision {}/{}",
                        if w.plan["wrist_clearance"] == true {
                            " · Experimental wrist clearance"
                        } else {
                            ""
                        },
                        w.plan["color_order"]
                            .as_str()
                            .unwrap_or("")
                            .replace(",", " → "),
                        w.plan["policy_model"].as_str().unwrap_or("Reference"),
                        w.revision_index().unwrap_or(0) + 1,
                        w.revisions.len()
                    )
                } else if mobile {
                    format!(
                        "Approach {} cube · stop at {:.2} m\n{} · revision {}/{}",
                        w.plan["color"].as_str().unwrap_or(""),
                        w.plan["standoff_m"].as_f64().unwrap_or_default(),
                        w.plan["policy_model"].as_str().unwrap_or(""),
                        w.revision_index().unwrap_or(0) + 1,
                        w.revisions.len()
                    )
                } else {
                    format!(
                        "{}   ·   {}\n{:+.3} rad   ·   {}   ·   revision {}/{}",
                        w.robot.name(),
                        w.plan["joint"].as_str().unwrap_or(""),
                        w.plan["delta_rad"].as_f64().unwrap_or_default(),
                        if w.plan["reach_checkpoint"].is_string() {
                            "Trained shoulder correction"
                        } else {
                            w.plan["policy_model"]
                                .as_str()
                                .or_else(|| w.plan["provider"].as_str())
                                .unwrap_or("")
                        },
                        w.revision_index().unwrap_or(0) + 1,
                        w.revisions.len()
                    )
                };
                let mut detail = detail;
                if w.plan["track_grasp"] == true {
                    detail.push_str("\nWrist-guided interception during descent");
                }
                if let Some(cap) = w.plan["max_forward_speed_m_s"].as_f64() {
                    detail.push_str(&format!("\nForward speed limit · {cap:.3} m/s"));
                }
                if let Some(reason) = w.plan["pickup_revision"]["rationale"].as_str() {
                    let revision = &w.plan["pickup_revision"];
                    let label = |v: &str| {
                        if v == "wrist-agreement-v1" {
                            "wrist agreement"
                        } else {
                            "unique detection per view"
                        }
                    };
                    if let (Some(before), Some(after)) = (
                        revision["previous_association"].as_str(),
                        revision["grasp_association"].as_str(),
                    ) {
                        detail.push_str(&format!(
                            "\nPickup rule · {} → {}\n{reason}",
                            label(before),
                            label(after)
                        ));
                    }
                }
                if let Some(target) = w.plan["trunk_yaw_rad"].as_f64() {
                    detail.push_str(&format!("\nWaist turn · {:.1}° left", target.to_degrees()));
                }
                if let Some(target) = w.plan["trunk_pitch_rad"].as_f64() {
                    detail.push_str(&format!(
                        "\nUpright trunk · waist pitch target {target:+.2} rad"
                    ));
                }
                if let (Some(checkpoint), Some(dataset)) = (
                    w.plan["grasp_checkpoint"].as_str(),
                    w.plan["training_dataset"].as_str(),
                ) {
                    detail.push_str(&format!(
                        "\nWrist detector {} · dataset {}",
                        checkpoint.get(..8).unwrap_or(checkpoint),
                        dataset.get(..8).unwrap_or(dataset)
                    ));
                }
                if let Some(n) = w.plan["scene_observation"]["visible"].as_u64() {
                    detail.push_str(&format!(
                        "\nAuthored from onboard RGB · {n} cubes localized"
                    ));
                }
                self.label(cx, ids!(detail), &detail);
                self.root
                    .widget(cx, ids!(onboard))
                    .set_visible(cx, w.robot == Robot::Nano);
                let onboard = self.root.button(cx, ids!(onboard));
                onboard.set_enabled(cx, !w.busy());
                onboard.set_text(
                    cx,
                    if w.onboard_context {
                        "Onboard RGB on"
                    } else {
                        "Onboard RGB off"
                    },
                );
                let index = w.revision_index();
                self.root.button(cx, ids!(older)).set_enabled(
                    cx,
                    !w.busy() && !w.revisions.is_empty() && index.is_none_or(|i| i > 0),
                );
                self.root.button(cx, ids!(newer)).set_enabled(
                    cx,
                    !w.busy()
                        && !w.revisions.is_empty()
                        && index.is_none_or(|i| i + 1 < w.revisions.len()),
                );
                self.root.widget(cx, ids!(revision_actions)).set_visible(
                    cx,
                    w.evidence["robot"].as_str() == Some(w.robot.name())
                        && w.evidence["task_success"].is_boolean(),
                );
                for id in [ids!(revise), ids!(revise_codex)] {
                    self.root.button(cx, id).set_enabled(cx, !w.busy());
                }
                self.model(cx, w);
                for id in [
                    ids!(nano),
                    ids!(g1),
                    ids!(reference),
                    ids!(codex),
                    ids!(octos),
                ] {
                    self.root.button(cx, id).set_enabled(cx, !w.busy());
                }
            }
            "aisle_scene" => {
                if self.joint_robot != Some(w.robot) {
                    self.joint_robot = None;
                }
                let joints = self.joint_robot.is_some();
                let editing = !joints && mobile && w.room_editor;
                self.root.widget(cx, ids!(room_mode)).set_visible(cx, mobile);
                self.root.button(cx, ids!(joint_mode)).set_enabled(cx, !joints && !w.busy());
                self.root
                    .button(cx, ids!(room_mode))
                    .set_enabled(cx, !w.busy() && !editing);
                self.root
                    .button(cx, ids!(robot_mode))
                    .set_enabled(cx, !w.busy() && (editing || joints));
                self.root.widget(cx, ids!(model_card)).set_visible(cx, !editing);
                self.root.widget(cx, ids!(inspector)).set_visible(cx, !editing && !joints);
                self.root.widget(cx, ids!(joint_card)).set_visible(cx, joints);
                for id in [ids!(photo_card), ids!(survey_card)] {
                    self.root.widget(cx, id).set_visible(cx, editing);
                }
                if editing {
                    let draft = w.room_draft_path.is_some();
                    self.label(
                        cx,
                        ids!(photo_label),
                        if draft {
                            "Proposed floor footprint"
                        } else {
                            "Selected floor footprint"
                        },
                    );
                    self.root
                        .widget(cx, ids!(room_edits))
                        .set_visible(cx, !draft && w.room_path.is_some());
                    let info = if draft {
                        &w.room_draft_info
                    } else {
                        &w.room_info
                    };
                    let preview = w.room_draft_path.as_ref().or(w.room_path.as_ref());
                    self.root
                        .widget(cx, ids!(draft_actions))
                        .set_visible(cx, draft);
                    for id in [
                        ids!(photo_octos),
                        ids!(photo_codex),
                        ids!(apply_draft),
                        ids!(discard_draft),
                    ] {
                        self.root.button(cx, id).set_enabled(cx, !w.busy());
                    }
                    let mut text = vec![info["description"]
                        .as_str()
                        .unwrap_or("Default living room")
                        .to_string()];
                    if draft {
                        text.insert(0, "Draft · review the footprint before use".to_owned());
                    }
                    for b in info["obstacles"].as_array().into_iter().flatten() {
                        let name = b["name"].as_str().unwrap_or("").trim_start_matches("room_");
                        text.push(format!(
                            "{name} · x {:.2}, y {:.2} m\n{:.2} × {:.2} m · height {:.2} m",
                            b["position"][0].as_f64().unwrap_or_default(),
                            b["position"][1].as_f64().unwrap_or_default(),
                            b["size"][0].as_f64().unwrap_or_default(),
                            b["size"][1].as_f64().unwrap_or_default(),
                            b["size"][2].as_f64().unwrap_or_default()
                        ));
                    }
                    if let Some(checks) = info["settings"]["camera_mount_checks"].as_object() {
                        for (name, label) in [
                            ("head", "Head"),
                            ("left_wrist", "Left wrist"),
                            ("right_wrist", "Right wrist"),
                        ] {
                            if let Some(check) = checks.get(name) {
                                if let (Some(error), Some(count)) =
                                    (check["max_check_px"].as_f64(), check["check_points"].as_u64())
                                {
                                    text.push(format!("{label} · {error:.2} px max on {count} check points"));
                                }
                            }
                        }
                    }
                    self.label(cx, ids!(room_detail), &text.join("\n"));
                    self.root
                        .button(cx, ids!(import_survey))
                        .set_enabled(cx, !w.busy());
                    for id in [ids!(save_geometry), ids!(save_environment), ids!(default_room)] {
                        self.root
                            .button(cx, id)
                            .set_enabled(cx, !w.busy() && !draft && w.room_path.is_some());
                    }
                    if self.room_preview.as_ref() != preview {
                        let settings = &info["settings"];
                        let ambient = settings["ambient_rgb"]
                            .as_array()
                            .map(|rgb| rgb.iter().map(ToString::to_string).collect::<Vec<_>>().join(" "))
                            .unwrap_or_default();
                        let friction = settings["floor_friction"]
                            .as_f64()
                            .map(|value| value.to_string())
                            .unwrap_or_default();
                        self.root.text_input(cx, ids!(room_ambient)).set_text(cx, &ambient);
                        self.root.text_input(cx, ids!(room_friction)).set_text(cx, &friction);
                        let player = self.root.video_player(cx, ids!(photo));
                        player.clear(cx);
                        if let Some(raw) = preview
                            .and_then(|p| std::fs::read(p.with_file_name("room-preview.rgb")).ok())
                        {
                            if raw.len() == 480 * 320 * 3 {
                                player.set_frame(cx, 480, 320, &raw);
                            }
                        }
                        self.room_preview = preview.cloned();
                    }
                }
                self.label(
                    cx,
                    ids!(fixture_kind),
                    if mobile {
                        "Living room · gravity 9.81 m/s²"
                    } else if standing {
                        "Floor · gravity 9.81 m/s²"
                    } else {
                        "Fixed base · zero gravity"
                    },
                );
                self.root.widget(cx, ids!(camera_profile)).set_visible(cx, transfer);
                self.root.button(cx, ids!(original_visuals)).set_enabled(cx, !w.busy() && w.compact_visuals);
                self.root.button(cx, ids!(compact_visuals)).set_enabled(cx, !w.busy() && !w.compact_visuals && w.plan["scene_observation"].is_null());
                self.root
                    .widget(cx, ids!(layouts))
                    .set_visible(cx, mobile || (w.robot == Robot::Nano && w.onboard_context));
                self.root
                    .widget(cx, ids!(rgb_delivery))
                    .set_visible(cx, mobile);
                self.root
                    .widget(cx, ids!(timing_controls))
                    .set_visible(cx, mobile);
                self.label(
                    cx,
                    ids!(timing_title),
                    if transfer {
                        "Left wrist RGB during pick"
                    } else {
                        "Onboard camera timing"
                    },
                );
                for (id, motion) in [
                    (ids!(still) as &[LiveId], TargetMotion::Stationary),
                    (ids!(slide_pos), TargetMotion::PositiveY),
                    (ids!(slide_neg), TargetMotion::NegativeY),
                ] {
                    self.root
                        .button(cx, id)
                        .set_enabled(cx, !w.busy() && w.target_motion != motion);
                }
                for (id, timing) in [
                    (ids!(on_time) as &[LiveId], RgbTiming::Normal),
                    (ids!(late_rgb), RgbTiming::Intermittent),
                    (ids!(frozen_rgb), RgbTiming::Disconnected),
                ] {
                    self.root
                        .button(cx, id)
                        .set_enabled(cx, !w.busy() && w.rgb_timing != timing);
                }
                self.root
                    .widget(cx, ids!(standing_fixtures))
                    .set_visible(cx, standing);
                self.root
                    .button(cx, ids!(level))
                    .set_enabled(cx, !w.busy() && w.impact);
                self.root
                    .button(cx, ids!(impact))
                    .set_enabled(cx, !w.busy() && !w.impact);
                self.label(cx, ids!(seed_title), &format!("Layout {}", w.layout_seed));
                self.root.button(cx, ids!(apply_seed)).set_enabled(cx, !w.busy());
                for (id, seed) in [
                    (ids!(layout0) as &[LiveId], 0),
                    (ids!(layout1), 1),
                    (ids!(layout2), 2),
                ] {
                    self.root
                        .button(cx, id)
                        .set_enabled(cx, !w.busy() && w.layout_seed != seed);
                }
                self.root
                    .widget(cx, ids!(fixed_fixtures))
                    .set_visible(cx, !mobile && !standing);
                self.label(
                    cx,
                    ids!(fixture),
                    if mobile {
                        "Three floor cubes · head and two wrist RGB cameras · wheel drive"
                    } else if standing {
                        if w.impact {
                            "0.25 kg block at 1.2 m/s · both feet and recovery monitored"
                        } else {
                            "Free pelvis · ankle feedback · both feet monitored"
                        }
                    } else if w.compliant {
                        "Compliant · position gain 18"
                    } else {
                        "Standard · position gain 28"
                    },
                );
                self.root
                    .button(cx, ids!(standard))
                    .set_enabled(cx, !w.busy() && w.compliant);
                self.root
                    .button(cx, ids!(compliant))
                    .set_enabled(cx, !w.busy() && !w.compliant);
                self.model(cx, w);
                self.root
                    .button(cx, ids!(inspect))
                    .set_enabled(cx, !w.busy());
                self.sync_joint_inspector(cx, w, joints);
                let info = if w.scene.is_null() {
                    "Inspect the URDF to verify its meshes and joint limits.".into()
                } else {
                    let mut rows = vec![format!(
                        "{} movable joints\n{} mesh assets",
                        w.scene["joint_count"], w.scene["mesh_count"]
                    )];
                    for joint in w.scene["allowed_joints"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|v| v.as_str())
                    {
                        rows.push(format!("{}\n{} rad", joint, w.scene["limits"][joint]));
                    }
                    rows.join("\n\n")
                };
                self.label(cx, ids!(detail), &info);
            }
            "aisle_run" => self.run(cx, w),
            _ => {
                let decision = w.decision.is_some();
                let can_pin = w.can_pin_room();
                let pinned = w.room_baseline.is_some();
                let show_baseline = !decision && !w.show_evaluation && !w.show_device && (can_pin || pinned);
                self.root.widget(cx, ids!(room_baseline)).set_visible(cx, show_baseline);
                let button = self.root.button(cx, ids!(room_baseline));
                button.set_enabled(cx, !w.busy() && (can_pin || pinned));
                button.set_text(cx, if pinned { "Clear room baseline" } else { "Use as room baseline" });
                let paired_room = w.transfer_comparison.as_ref().and_then(|pair| pair.room.as_ref());
                if paired_room.is_none() || self.detailed_trial.as_ref() != w.selected.as_ref() {
                    self.detailed_trial = None;
                }
                self.root.widget(cx, ids!(room_details)).set_visible(cx, show_baseline && paired_room.is_some());
                self.root.button(cx, ids!(room_details)).set_text(cx, if self.detailed_trial.is_some() { "Room comparison" } else { "Trial details" });
                let has_packets = w
                    .transfer_comparison
                    .as_ref()
                    .is_some_and(|c| c.packets.iter().any(Option::is_some));
                self.root
                    .widget(cx, ids!(decision_card))
                    .set_visible(cx, decision);
                self.root
                    .widget(cx, ids!(decision_rgb))
                    .set_visible(cx, has_packets);
                self.root
                    .button(cx, ids!(decision_rgb))
                    .set_enabled(cx, !w.busy() && has_packets);
                let has_frames = w
                    .transfer_comparison
                    .as_ref()
                    .is_some_and(|c| c.frames.iter().any(Option::is_some));
                self.root.widget(cx, ids!(change_bar)).set_visible(
                    cx,
                    (has_frames || has_packets)
                        && !decision
                        && !w.show_device
                        && !w.show_evaluation,
                );
                for id in [ids!(summary), ids!(plot), ids!(trial_bar)] {
                    self.root
                        .widget(cx, id)
                        .set_visible(cx, !decision && !w.show_evaluation && !w.show_device);
                }
                self.root
                    .widget(cx, ids!(cohort))
                    .set_visible(cx, w.show_evaluation);
                self.root.button(cx, ids!(trial_mode)).set_enabled(
                    cx,
                    !w.busy() && (decision || w.show_evaluation || w.show_device),
                );
                self.root
                    .button(cx, ids!(evaluation_mode))
                    .set_enabled(cx, !w.busy() && !w.show_evaluation);
                self.root
                    .widget(cx, ids!(device))
                    .set_visible(cx, w.show_device);
                self.root
                    .button(cx, ids!(device_mode))
                    .set_enabled(cx, !w.busy() && !w.show_device);
                if let Some((dir, receipt)) = &w.decision {
                    self.root.button(cx, ids!(trial_mode)).set_enabled(cx, true);
                    self.label(
                        cx,
                        ids!(context_label),
                        "Nano   /   Onboard policy observations",
                    );
                    self.label(
                        cx,
                        ids!(decision_title),
                        match receipt["identical_inputs"].as_bool() {
                            Some(true) => {
                                "Both revisions received identical RGB, encoders and timestamps"
                            }
                            Some(false) => "The recorded policy inputs differ",
                            None => "Recorded policy observation",
                        },
                    );
                    for (side, label, view, name) in [
                        (
                            "previous",
                            ids!(prev_label) as &[LiveId],
                            ids!(prev_rgb) as &[LiveId],
                            "Previous",
                        ),
                        ("current", ids!(curr_label), ids!(curr_rgb), "Current"),
                    ] {
                        let row = &receipt["views"][side];
                        self.root.widget(cx, view).set_visible(cx, row.is_object());
                        let content = if row.is_object() {
                            format!(
                                "{name} revision · physics tick {} · {} s",
                                row["tick"],
                                row["stamp_ns"].as_u64().unwrap_or(0) as f64 / 1e9
                            )
                        } else {
                            format!("{name} revision · no retained exposure")
                        };
                        self.label(cx, label, &content);
                        if row.is_object() && self.decision_preview.as_ref() != Some(dir) {
                            let camera = self.root.video_player(cx, view);
                            if let Some(mut player) = camera.borrow_mut() {
                                player.clear(cx);
                                if let Ok(rgb) = std::fs::read(dir.join(format!("{side}.rgb"))) {
                                    if rgb.len() == 1440 * 360 * 3 {
                                        player.set_frame(cx, 1440, 360, &rgb);
                                    }
                                }
                            };
                        }
                    }
                    self.decision_preview = Some(dir.clone());
                    self.label(cx, ids!(status), "");
                    return;
                }
                if w.show_device {
                    let g1 = w.evidence["robot"] == "G1";
                    let standing = g1 && w.evidence["kind"] == "standing-inspection";
                    let selected = w.evidence["task_success"] == true;
                    self.label(
                        cx,
                        ids!(context_label),
                        "Selected simulation trial   /   Device preflight",
                    );
                    self.label(
                        cx,
                        ids!(device_title),
                        if g1 {
                            "Unitree G1 · 29-joint LowState"
                        } else if selected {
                            "Adora Nano · Feetech base"
                        } else {
                            "Select a successful simulation trial"
                        },
                    );
                    self.label(cx,ids!(device_info), if standing {
                        "Replay encoders and IMU from the selected simulation or observe an external DDS publisher. The standing controller computes targets locally with motor output disabled."
                    } else if g1 { "Select a standing-reach trial to run the G1 telemetry preflight." }
                    else if selected { "Check which measurements the installed Feetech adapter provides for this controller." } else {""});
                    self.root
                        .widget(cx, ids!(device_endpoint))
                        .set_visible(cx, standing);
                    self.root
                        .widget(cx, ids!(device_listen))
                        .set_visible(cx, standing);
                    self.root
                        .widget(cx, ids!(device_stop))
                        .set_visible(cx, w.preflighting());
                    self.root.button(cx, ids!(device_replay)).set_text(
                        cx,
                        if g1 {
                            "Replay through DDS"
                        } else {
                            "Check adapter"
                        },
                    );
                    self.root
                        .button(cx, ids!(device_replay))
                        .set_enabled(cx, !w.busy() && selected && (standing || !g1));
                    self.root
                        .button(cx, ids!(device_listen))
                        .set_enabled(cx, !w.busy() && selected && standing);
                    let v = &w.preflight;
                    let summary = match v["state"].as_str() {
                        Some("Dry run measured") => {
                            let mut lines = vec![
                                format!(
                                    "{} · {} measurements",
                                    v["source_kind"].as_str().unwrap_or("DDS"),
                                    v["samples"]
                                ),
                                format!(
                                    "{} joints · {} dry-run steps · 0 motor commands sent",
                                    v["joints"], v["dry_run_steps"]
                                ),
                                format!(
                                    "Longest gaps · source {:.1} ms · receiver {:.1} ms",
                                    v["max_source_gap_ms"].as_f64().unwrap_or_default(),
                                    v["max_receive_gap_ms"].as_f64().unwrap_or_default()
                                ),
                                format!(
                                    "Peak target to measured joint difference {:.3} rad",
                                    v["peak_target_feedback_difference_rad"]
                                        .as_f64()
                                        .unwrap_or_default()
                                ),
                            ];
                            if let Some(difference) = v["max_nexus_target_difference_rad"].as_f64()
                            {
                                lines.push(format!(
                                    "Maximum paired target difference {difference:.6} rad"
                                ));
                            }
                            lines.push("Physical device qualification pending".into());
                            lines.join("\n")
                        }
                        Some(state) => format!("{state}\n{}", v["reason"].as_str().unwrap_or("")),
                        None => String::new(),
                    };
                    self.label(cx, ids!(device_result), &summary);
                    self.root
                        .button(cx, ids!(device_folder))
                        .set_enabled(cx, !w.busy() && v.is_object());
                    return;
                }
                if w.show_evaluation {
                    let result = &w.evaluation;
                    let comparing = result["comparison"] == true;
                    let g1 = result["profile"] == "g1-standing-reach-v1" || result["robot"] == "G1";
                    self.label(
                        cx,
                        ids!(context_label),
                        if g1 {
                            "Unitree G1   /   Nexus simulation   /   Standing reach"
                        } else if result["cases"].is_number() {
                            "Adora Nano   /   Nexus simulation   /   Batch evaluation"
                        } else {
                            "Nexus simulation   /   Batch evaluation"
                        },
                    );
                    self.root
                        .button(cx, ids!(verify_archive))
                        .set_enabled(cx, !w.busy());
                    self.root
                        .button(cx, ids!(compare_archives))
                        .set_enabled(cx, !w.busy());
                    let failures =
                        result[if comparing { "changes" } else { "failures" }].as_array();
                    self.root.button(cx, ids!(earlier_failure)).set_text(
                        cx,
                        if comparing {
                            "Previous case"
                        } else {
                            "Previous failure"
                        },
                    );
                    self.root.button(cx, ids!(later_failure)).set_text(
                        cx,
                        if comparing {
                            "Next case"
                        } else {
                            "Next failure"
                        },
                    );
                    let count = failures.map_or(0, Vec::len);
                    self.root
                        .widget(cx, ids!(failure_bar))
                        .set_visible(cx, count > 1);
                    self.root
                        .button(cx, ids!(earlier_failure))
                        .set_enabled(cx, w.failure_index > 0);
                    self.root
                        .button(cx, ids!(later_failure))
                        .set_enabled(cx, w.failure_index + 1 < count);
                    if comparing {
                        let counts = &result["counts"];
                        let archive_id = |side: &str| {
                            result[side]["archive_sha256"].as_str().and_then(|s| s.get(..8)).unwrap_or("?")
                        };
                        let mut detail = format!(
                            "Baseline {} · {} of {} passed\nCandidate {} · {} of {} passed\nBoth passed {} · gained {} · lost {} · both failed {} · unverified {}",
                            archive_id("baseline"), result["baseline"]["successes"], result["cases"],
                            archive_id("candidate"), result["candidate"]["successes"], result["cases"],
                            counts["both_passed"], counts["improved"], counts["regressed"], counts["both_failed"], counts["unverified"]
                        );
                        let receipts = [result["baseline"]["archive_integrity"].as_str(), result["candidate"]["archive_integrity"].as_str()];
                        if receipts.contains(&Some("Local output receipt")) {
                            detail.push_str("\nIncludes a locally supplied camera archive receipt.");
                        } else if receipts == [Some("Published Studio output digest"); 2] {
                            detail.push_str("\nBoth camera archives match published digests.");
                        }
                        self.label(cx, ids!(cohort_verdict), "Matched cohort comparison");
                        self.label(cx, ids!(cohort_detail), &detail);
                        let selected = failures.and_then(|v| v.get(w.failure_index));
                        let detail = selected.map(|row| {
                            let identity = if g1 {
                                let recipe = &row["recipe"];
                                format!("impact {} kg at {} m/s · offset {} m", recipe["mass_kg"], recipe["speed_m_s"], recipe["lateral_m"])
                            } else {
                                format!("seed {}", row["seed"])
                            };
                            let verdict = match row["outcome"].as_str() {
                                Some("improved") => "Candidate gained this case",
                                Some("regressed") => "Candidate lost this case",
                                _ => "Incomplete evidence",
                            };
                            let mut lines = vec![format!("{verdict} · {} of {count}\nBaseline case {} · candidate case {} · {identity}",
                                w.failure_index + 1, row["baseline_index"], row["candidate_index"])];
                            for (key, label) in [("baseline_reasons", "Baseline"), ("candidate_reasons", "Candidate")] {
                                if let Some(reasons) = row[key].as_array() {
                                    for reason in reasons.iter().filter_map(|v| v.as_str()) {
                                        lines.push(format!("{label} · {reason}"));
                                    }
                                }
                            }
                            lines.join("\n")
                        }).unwrap_or_else(|| "All paired cases have matching verified outcomes.".into());
                        self.label(cx, ids!(failure_detail), &detail);
                        return;
                    }
                    let (title, mut detail) = if result["cases"].is_number() {
                        let title = if result["split"] == "development" {
                            "Development results"
                        } else if result["split"] == "matched" {
                            "Matched evaluation"
                        } else if result["qualified"] == true {
                            "Qualified"
                        } else {
                            "Qualification failed"
                        };
                        (
                            title,
                            if g1 {
                                format!("{} of {} standing reaches · {} verified traces\n{} measured guard bypasses · all 20 required",
                                    result["successes"], result["cases"], result["verified"], result["verified_guard_bypasses"])
                            } else {
                                format!(
                                "{} of {} complete transfers · 95% Wilson interval {:.1}–{:.1}%\n{} verified trials · {} measured guard bypasses{}",
                                result["successes"],
                                result["cases"],
                                result["interval_95"][0].as_f64().unwrap_or(0.) * 100.,
                                result["interval_95"][1].as_f64().unwrap_or(0.) * 100.,
                                result["verified"],
                                result["verified_guard_bypasses"],
                                result["requirement"]
                                    .as_u64()
                                    .map(|n| format!(" · {n} required"))
                                    .unwrap_or_default()
                            )
                            },
                        )
                    } else {
                        ("No evaluation selected", String::new())
                    };
                    if let (Some(count), Some(receipt)) = (result["release_exposures"].as_u64(), result["archive_integrity"].as_str()) {
                        detail.push_str(&format!("\n{count} camera packets · {receipt}"));
                    }
                    self.label(cx, ids!(cohort_verdict), title);
                    self.label(cx, ids!(cohort_detail), &detail);
                    let failure = failures
                        .and_then(|v| v.get(w.failure_index))
                        .map(|row| {
                            let reasons = row["reasons"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter_map(|v| v.as_str())
                                .collect::<Vec<_>>()
                                .join("\n");
                            if g1 {
                                format!(
                                    "Failure {} of {count} · impact case {}\n{reasons}",
                                    w.failure_index + 1,
                                    row["index"]
                                )
                            } else {
                                format!(
                                    "Failure {} of {count} · layout {} · seed {}\n{reasons}",
                                    w.failure_index + 1,
                                    row["index"],
                                    row["seed"]
                                )
                            }
                        })
                        .unwrap_or_default();
                    self.label(cx, ids!(failure_detail), &failure);
                    return;
                }
                let verdict = match w.evidence["task_success"].as_bool() {
                    Some(true) => "Passed",
                    Some(false) => "Failed",
                    None => {
                        if w.selected.is_some() {
                            w.evidence["outcome"].as_str().unwrap_or("Unscored")
                        } else {
                            "No runs yet"
                        }
                    }
                };
                let room = paired_room.filter(|_| self.detailed_trial.is_none());
                let title = room.map(|_| format!("Room comparison · layout {}", w.evidence["seed"]));
                self.label(cx, ids!(summary_title), title.as_deref().unwrap_or("Task outcome"));
                self.label(cx, ids!(verdict), room.map_or(verdict, |r| r.outcome.as_str()));
                let text = if transfer {
                    let mut lines = vec![];
                    if w.evidence["wrist_clearance"] == true {
                        lines.push("Experimental wrist clearance".to_string());
                    }
                    let wrist = &w.evidence["wrist_timing"];
                    if let Some(fixture) = wrist["fixture"].as_str() {
                        let outcome = if wrist["received"] == 0 {
                            "no pick samples"
                        } else if wrist["timed_out"] == true {
                            "stopped on timeout"
                        } else if wrist["recoveries"].as_u64().unwrap_or(0) > 0 {
                            "resumed on fresh RGB"
                        } else {
                            "fresh"
                        };
                        lines.push(format!("Left wrist RGB · {fixture} · {outcome}"));
                    }
                    if let Some(cubes) = w.evidence["moving_grasp"]["cubes"].as_object() {
                        for (color, slide) in cubes {
                            let contact = &slide["first_contact"];
                            let verdict = if slide["passed"] == true {
                                "confirmed"
                            } else {
                                "incomplete"
                            };
                            if let Some(speed) = contact["prior_velocity_y_m_s"].as_f64() {
                                lines.push(format!("{color} moving grasp {verdict} · {:+.1} mm/s before contact · {} two-finger lift steps",
                                    1000.*speed,slide["first_attempt_lift_ticks"].as_u64().unwrap_or(0)));
                            } else {
                                lines.push(format!(
                                    "{color} moving grasp {verdict} · no finger contact observed"
                                ));
                            }
                        }
                    }
                    if let Some(checkpoint) = w.evidence["grasp_checkpoint"].as_str() {
                        lines.push(format!(
                            "Trained wrist detector {}",
                            checkpoint.get(..8).unwrap_or(checkpoint)
                        ));
                    }
                    for color in w.evidence["requested"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|s| s.as_str())
                    {
                        let record = &w.evidence["cubes"][color];
                        let contacts = record["table_contact_tail_ticks"].as_u64().unwrap_or(0);
                        let state = if record["passed"] == true {
                            "settled"
                        } else {
                            "incomplete"
                        };
                        let inset = record["nearest_table_edge_m"]
                            .as_f64()
                            .map_or_else(String::new, |m| {
                                format!(" · {:.0} mm inside rim", m * 1000.0)
                            });
                        lines.push(format!(
                            "{color} · {state} · {contacts}/100 table contacts{inset}"
                        ));
                    }
                    lines.push(format!(
                        "Layout {} · wheel support {}/{} steps",
                        w.evidence["seed"],
                        w.evidence["wheel_supported_ticks"],
                        w.evidence["physics_ticks"]
                    ));
                    if let Some(gaps) = w.evidence["unsupported_wheel_ranges"].as_array() {
                        for gap in gaps.iter().take(2) {
                            if let (Some(start), Some(end), Some(phase)) = (
                                gap["first_step"].as_u64(),
                                gap["last_step"].as_u64(),
                                gap["phase"].as_str(),
                            ) {
                                let ticks = if start == end {
                                    start.to_string()
                                } else {
                                    format!("{start}–{end}")
                                };
                                lines.push(format!("Wheel support lost · {phase} · tick {ticks}"));
                            }
                        }
                        if gaps.len() > 2 {
                            lines.push(format!("{} more intervals in result.json", gaps.len() - 2));
                        }
                    }
                    let audit = &w.evidence["actuator_audit"];
                    if audit.is_object() {
                        lines.push(format!(
                            "Motor audit {} · {} recorded writes",
                            if audit["passed"] == true {
                                "passed"
                            } else {
                                "failed"
                            },
                            audit["applied_rows"],
                        ));
                    }
                    if let Some(contacts) = w.evidence["robot_furniture_contacts"].as_object() {
                        if contacts.is_empty() {
                            lines.push("No furniture contacts".into());
                        } else {
                            for (pair, ticks) in contacts {
                                lines.push(format!(
                                    "Contact during {} · {ticks} steps",
                                    pair.replace('/', " / ")
                                ));
                            }
                        }
                    }
                    if let Some(reason) = w.evidence["error"].as_str() {
                        lines.push(reason.to_owned());
                    }
                    lines.join("\n")
                } else if mobile {
                    let mut detail = format!(
                        "{} cube · requested {:.3} m · final distance {:.3} m\nWheel support {}/{} physics steps · {} obstacle-contact steps",
                        w.evidence["color"].as_str().unwrap_or(""),
                        w.evidence["standoff_m"].as_f64().unwrap_or_default(),
                        w.evidence["final_distance_m"].as_f64().unwrap_or_default(),
                        w.evidence["wheel_supported_ticks"],
                        w.evidence["physics_ticks"],
                        w.evidence["obstacle_contact_ticks"]
                    );
                    let motion = &w.evidence["target_motion"];
                    if let Some(displacement) = motion["displacement_y_m"].as_f64() {
                        detail.push_str(&format!(
                            "\nCube travel {:+.1} cm along Y · localized in {}/{} moving exposures",
                            displacement * 100.,
                            motion["localized_moving_exposures"],
                            motion["moving_exposures"]
                        ));
                        if let Some(error) = motion["max_moving_error_m"].as_f64() {
                            detail.push_str(&format!(" · worst error {:.1} mm", error * 1000.));
                        }
                    }
                    let timing = &w.evidence["sensor_timing"];
                    if let Some(fixture) = timing["fixture"].as_str() {
                        let held: u64 = timing["rejected"]
                            .as_object()
                            .into_iter()
                            .flat_map(|m| m.values())
                            .filter_map(|n| n.as_u64())
                            .sum();
                        let recovered = timing["recoveries"].as_u64().unwrap_or_default();
                        let noun = if recovered == 1 {
                            "recovery"
                        } else {
                            "recoveries"
                        };
                        detail.push_str(&format!("\nCamera timing {fixture} · {held} withheld observations · {recovered} {noun}"));
                        if timing["timed_out"] == true {
                            detail.push_str(" · stopped after timeout");
                        }
                    }
                    detail
                } else if standing {
                    let mut summary = format!(
                        "{} · requested {:+.3} rad · measured {:+.4} rad\nBoth feet loaded {}/{} steps\nPeak tilt {:.1}° · maximum pelvis drift {:.1} cm",
                        w.evidence["joint"].as_str().unwrap_or("Shoulder"),
                        w.evidence["requested_delta"].as_f64().unwrap_or_default(),
                        w.evidence["measured_delta"].as_f64().unwrap_or_default(),
                        w.evidence["both_feet_loaded_steps"],
                        w.evidence["steps"],
                        w.evidence["peak_tilt_deg"].as_f64().unwrap_or_default(),
                        w.evidence["max_pelvis_drift_m"]
                            .as_f64()
                            .unwrap_or_default()
                            * 100.
                    );
                    if let Some(target) = w.evidence["trunk_yaw_target_rad"]
                        .as_f64()
                        .filter(|&v| v != 0.0)
                    {
                        if let Some(actual) = w.evidence["settled_trunk_yaw_rad"].as_f64() {
                            summary.push_str(&format!(
                                "\nWaist turn · {:.1}° requested · {:.1}° measured",
                                target.to_degrees(),
                                actual.to_degrees()
                            ));
                        }
                    }
                    if w.evidence["trunk_pitch_target_rad"]
                        .as_f64()
                        .is_some_and(|v| v != 0.0)
                    {
                        if let Some(measured) = w.evidence["settled_trunk_pitch_rad"].as_f64() {
                            summary.push_str(&format!(
                                "\nUpright trunk · measured waist pitch {measured:+.3} rad"
                            ));
                        }
                    }
                    if let Some(joules) = w.evidence["estimated_positive_joint_work_j"].as_f64() {
                        summary.push_str(&format!("\nEstimated positive joint work {joules:.2} J"));
                    }
                    if let Some(error) = w.evidence["wrist_reach"]["max_final_100_error_m"].as_f64()
                    {
                        summary.push_str(&format!(
                            "\nWrist error in final 0.50 s · {:.1} mm",
                            error * 1000.
                        ));
                    }
                    if w.evidence["reach_checkpoint"].is_string() {
                        summary.push_str("\nController uses the trained shoulder correction");
                    }
                    let impact = &w.evidence["disturbance"];
                    if impact.is_object() {
                        if let Some(links) = impact["links"].as_array() {
                            let labels = links
                                .iter()
                                .filter_map(|link| link.as_str())
                                .map(|name| name.trim_end_matches("_link").replace('_', " "))
                                .collect::<Vec<_>>();
                            if labels.is_empty() {
                                summary.push_str("\nNo block contact recorded");
                            } else {
                                summary
                                    .push_str(&format!("\nBlock contacted {}", labels.join(", ")));
                            }
                        }
                        let recovery = impact["recovery_s"]
                            .as_f64()
                            .map(|t| format!("stable after {t:.2} s"))
                            .unwrap_or_else(|| "recovery unconfirmed".into());
                        summary.push_str(&format!(
                            "\nImpact {:.3} N·s · {recovery}",
                            impact["normal_impulse_sum_ns"].as_f64().unwrap_or(0.0)
                        ));
                    }
                    summary
                } else if let Some(angle) = w.evidence["measured_delta"].as_f64() {
                    format!(
                        "{} · {} · {} fixture\nRequested {:+.3} rad    Measured {:+.5} rad    {} observations\nLimits {} over {} physics steps.",
                        w.evidence["robot"].as_str().unwrap_or(""),
                        w.evidence["joint"].as_str().unwrap_or(""),
                        w.evidence["fixture"].as_str().unwrap_or("standard"),
                        w.evidence["requested_delta"].as_f64().unwrap_or_default(),
                        angle,
                        w.evidence["frames"],
                        if w.evidence["within_limits"].as_bool() == Some(true) {
                            "respected"
                        } else {
                            "exceeded"
                        },
                        w.evidence["steps"]
                    )
                } else {
                    w.evidence["reason"]
                        .as_str()
                        .unwrap_or("Run a prepared controller to record its outcome.")
                        .to_owned()
                };
                let status = if let Some(comparison) = &w.transfer_comparison {
                    for (id, side, label) in [
                        (
                            ids!(previous_change) as &[LiveId],
                            RevisionSide::Previous,
                            if comparison.room.is_some() { "baseline" } else { "previous" },
                        ),
                        (ids!(current_change), RevisionSide::Current, "current"),
                    ] {
                        let frame = w.change_frame(side);
                        self.root.widget(cx, id).set_visible(cx, frame.is_some());
                        if let Some(frame) = frame {
                            let verb = if w.dataset_for(&frame.trial).is_some() {
                                "Replay"
                            } else {
                                "Export"
                            };
                            self.root.button(cx, id).set_text(
                                cx,
                                &format!("{verb} {label} · tick {}", frame.sample_tick),
                            );
                            self.root.button(cx, id).set_enabled(cx, !w.busy());
                        }
                    }
                    comparison.summary.clone()
                } else if let Some(note) = &w.room_pair_note {
                    note.clone()
                } else if let Some(diff) = w.comparison {
                    format!(
                        "Compared with previous matching trial: {diff:+.6} rad. Open the run folder for controller.json, trajectory.json and frame.png."
                    )
                } else if w.evidence["task_success"].is_boolean() {
                    "Open the run folder for controller.json, trajectory.json and frame.png."
                        .to_owned()
                } else {
                    String::new()
                };
                let msg = if w.busy() {
                    &w.status
                } else {
                    w.error.as_ref().unwrap_or(&status)
                };
                self.label(cx, ids!(status), msg);
                self.root
                    .widget(cx, ids!(export))
                    .set_visible(cx, w.dataset.is_none() && w.evidence["recording"] == true);
                self.root
                    .button(cx, ids!(export))
                    .set_enabled(cx, !w.busy());
                self.root
                    .widget(cx, ids!(replay))
                    .set_visible(cx, w.dataset.is_some());
                self.root
                    .button(cx, ids!(replay))
                    .set_enabled(cx, !w.busy());
                let train_grasp = w.robot == Robot::Nano
                    && w.evidence["kind"] == "table-transfer"
                    && w.evidence["task_success"] == true
                    && w.evidence["carry_annotations"] == true
                    && w.evidence["grasp_checkpoint"].is_null()
                    && w.dataset.is_some();
                self.root
                    .widget(cx, ids!(train_grasp))
                    .set_visible(cx, train_grasp);
                self.root
                    .button(cx, ids!(train_grasp))
                    .set_enabled(cx, !w.busy());
                let can_train = w.robot == Robot::G1
                    && w.evidence["kind"] == "standing-inspection"
                    && w.evidence["task_success"] == true
                    && w.evidence["reach_checkpoint"].is_null()
                    && w.dataset.is_some();
                self.root
                    .widget(cx, ids!(train_reach))
                    .set_visible(cx, can_train);
                self.root
                    .button(cx, ids!(train_reach))
                    .set_enabled(cx, !w.busy());
                self.label(cx, ids!(detail), room.map_or(text.as_str(), |r| r.detail.as_str()));
                let trace: Vec<_> = w.trace.iter().map(|&x| x as f64).collect();
                self.root
                    .widget(cx, ids!(trace))
                    .set_visible(cx, !trace.is_empty());
                self.label(
                    cx,
                    ids!(trace_label),
                    if transfer {
                        "Cubes in contact with the tabletop"
                    } else if mobile {
                        "RGB-estimated range · metres"
                    } else {
                        "Measured joint angle · radians"
                    },
                );
                let scale = if transfer && !trace.is_empty() {
                    format!(
                        "Final table contacts {} · {:.1} s simulated",
                        trace[trace.len() - 1],
                        w.evidence["physics_ticks"].as_f64().unwrap_or_default() * 0.01
                    )
                } else if mobile && !trace.is_empty() {
                    format!(
                        "First {:.3} m                         Last {:.3} m",
                        trace[0],
                        trace[trace.len() - 1]
                    )
                } else if standing && !trace.is_empty() {
                    format!(
                        "10 s standing trial · last shoulder angle {:+.4} rad",
                        trace[trace.len() - 1]
                    )
                } else if let (Some(start), Some(end)) = (trace.first(), trace.last()) {
                    format!(
                        "0.01 s   {start:+.5} rad                             1.21 s   {end:+.5} rad"
                    )
                } else {
                    "Awaiting completed trajectory".into()
                };
                self.label(cx, ids!(scale), &scale);
                self.root
                    .spark(cx, ids!(trace))
                    .set(cx, &trace, None, 3, light as f32);
                let pos = w
                    .history
                    .iter()
                    .position(|p| Some(p) == w.selected.as_ref());
                self.root
                    .button(cx, ids!(previous))
                    .set_enabled(cx, !w.busy() && pos.is_some_and(|i| i > 0));
                self.root.button(cx, ids!(next)).set_enabled(
                    cx,
                    !w.busy() && pos.is_some_and(|i| i + 1 < w.history.len()),
                );
                self.root
                    .button(cx, ids!(folder))
                    .set_enabled(cx, !w.busy() && w.selected.is_some());
            }
        }
    }
    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions, b: &mut dyn Backend) -> bool {
        if self.page == "aisle_scene" && self.handle_joint_inspector(cx, actions, b) {
            return true;
        }
        let Some(_) = b.aisle() else {
            return false;
        };
        if self.root.button(cx, ids!(room_details)).clicked(actions) {
            self.detailed_trial = if self.detailed_trial.is_some() {
                None
            } else {
                b.aisle().and_then(|w| w.selected.clone())
            };
            return true;
        }
        if self.root.button(cx, ids!(connect)).clicked(actions) {
            let alias = self.root.text_input(cx, ids!(node)).text();
            b.dispatch(Intent::Aisle(Action::ConnectWorker(alias)));
            return true;
        }
        for (id, side) in [
            (ids!(previous_change) as &[LiveId], RevisionSide::Previous),
            (ids!(current_change), RevisionSide::Current),
        ] {
            if self.root.button(cx, id).clicked(actions) {
                let target = b.aisle().and_then(|w| {
                    let frame = w.change_frame(side)?;
                    Some((w.dataset_for(&frame.trial)?, frame.time_s))
                });
                if let Some((dataset, time)) = target {
                    b.dispatch(Intent::OpenFolder(dataset));
                    b.dispatch(Intent::Seek(time));
                } else {
                    b.dispatch(Intent::Aisle(Action::ExportChange(side)));
                }
                return true;
            }
        }
        if self.root.button(cx, ids!(replay)).clicked(actions) {
            if let Some(path) = b.aisle().and_then(|w| w.dataset.clone()) {
                b.dispatch(Intent::OpenFolder(path));
            }
            return true;
        }
        for (id, act) in [
            (ids!(nano) as &[LiveId], Action::Select(Robot::Nano)),
            (ids!(g1), Action::Select(Robot::G1)),
            (ids!(inspect), Action::Inspect),
            (ids!(layout0), Action::Layout(0)),
            (ids!(layout1), Action::Layout(1)),
            (ids!(layout2), Action::Layout(2)),
            (ids!(standard), Action::Fixture(false)),
            (ids!(compliant), Action::Fixture(true)),
            (ids!(level), Action::Impact(false)),
            (ids!(impact), Action::Impact(true)),
            (ids!(onboard), Action::ToggleOnboardContext),
            (ids!(still), Action::TargetMotion(TargetMotion::Stationary)),
            (
                ids!(slide_pos),
                Action::TargetMotion(TargetMotion::PositiveY),
            ),
            (
                ids!(slide_neg),
                Action::TargetMotion(TargetMotion::NegativeY),
            ),
            (ids!(original_visuals), Action::CompactVisuals(false)),
            (ids!(compact_visuals), Action::CompactVisuals(true)),
            (ids!(on_time), Action::RgbTiming(RgbTiming::Normal)),
            (ids!(late_rgb), Action::RgbTiming(RgbTiming::Intermittent)),
            (ids!(frozen_rgb), Action::RgbTiming(RgbTiming::Disconnected)),
            (ids!(device_mode), Action::ShowDevice),
            (ids!(room_baseline), Action::ToggleRoomBaseline),
            (ids!(device_replay), Action::DevicePreflight(None)),
            (ids!(device_stop), Action::Stop),
            (ids!(device_folder), Action::OpenPreflight),
            (ids!(trial_mode), Action::ShowEvaluation(false)),
            (ids!(decision_rgb), Action::InspectDecision),
            (ids!(evaluation_mode), Action::ShowEvaluation(true)),
            (ids!(earlier_failure), Action::SelectFailure(false)),
            (ids!(later_failure), Action::SelectFailure(true)),
            (ids!(room_mode), Action::EditRoom(true)),
            (ids!(robot_mode), Action::EditRoom(false)),
            (ids!(default_room), Action::DefaultRoom),
            (ids!(review_controller), Action::ReviewController),
            (ids!(older), Action::OlderPlan),
            (ids!(newer), Action::NewerPlan),
            (ids!(clock), Action::ToggleObservationClock),
            (ids!(period), Action::CycleBackgroundPeriod),
            (ids!(sample_job), Action::Worker(WorkerAction::Sample)),
            (ids!(local_run), Action::Workers(false)),
            (ids!(workers), Action::Workers(true)),
            (ids!(refresh), Action::Worker(WorkerAction::Refresh)),
            (ids!(queue_stream), Action::Worker(WorkerAction::Stream)),
            (
                ids!(queue_background),
                Action::Worker(WorkerAction::Observations),
            ),
            (ids!(older_job), Action::Worker(WorkerAction::Previous)),
            (ids!(newer_job), Action::Worker(WorkerAction::Next)),
            (ids!(cancel_job), Action::Worker(WorkerAction::Cancel)),
            (ids!(fetch_job), Action::Worker(WorkerAction::Fetch)),
            (ids!(start_run), Action::Run),
            (ids!(stop_run), Action::Stop),
            (ids!(previous), Action::Previous),
            (ids!(next), Action::Next),
            (ids!(folder), Action::Open),
            (ids!(apply_draft), Action::ApplyRoomDraft),
            (ids!(discard_draft), Action::DiscardRoomDraft),
            (ids!(export), Action::Export),
            (ids!(train_reach), Action::TrainReach),
            (ids!(train_grasp), Action::TrainGrasp),
        ] {
            if self.root.button(cx, id).clicked(actions) {
                b.dispatch(Intent::Aisle(act));
                return true;
            }
        }
        if self.root.button(cx, ids!(apply_seed)).clicked(actions) {
            let seed = self.root.text_input(cx, ids!(seed_input)).text();
            b.dispatch(Intent::Aisle(Action::CustomLayout(seed)));
            return true;
        }
        if self.root.button(cx, ids!(device_listen)).clicked(actions) {
            let endpoint = self.root.text_input(cx, ids!(device_endpoint)).text();
            b.dispatch(Intent::Aisle(Action::DevicePreflight(Some(endpoint))));
            return true;
        }
        let compare = self
            .root
            .button(cx, ids!(compare_archives))
            .clicked(actions);
        if compare || self.root.button(cx, ids!(verify_archive)).clicked(actions) {
            let archive = self
                .root
                .text_input(cx, ids!(archive_path))
                .text()
                .trim()
                .to_owned();
            let baseline = compare.then(|| {
                self.root
                    .text_input(cx, ids!(baseline_path))
                    .text()
                    .trim()
                    .to_owned()
            });
            b.dispatch(Intent::Aisle(Action::Evaluate { archive, baseline }));
            return true;
        }
        for (id, planner) in [
            (ids!(photo_octos) as &[LiveId], Planner::OctosDeepSeek),
            (ids!(photo_codex), Planner::Codex),
        ] {
            if self.root.button(cx, id).clicked(actions) {
                let source = self
                    .root
                    .text_input(cx, ids!(survey_path))
                    .text()
                    .trim()
                    .to_owned();
                b.dispatch(Intent::Aisle(Action::DraftRoom { source, planner }));
                return true;
            }
        }
        if self.root.button(cx, ids!(save_environment)).clicked(actions) {
            let ambient = self.root.text_input(cx, ids!(room_ambient)).text();
            let friction = self.root.text_input(cx, ids!(room_friction)).text();
            b.dispatch(Intent::Aisle(Action::SetRoomEnvironment { ambient, friction }));
            return true;
        }
        let importing = self.root.button(cx, ids!(import_survey)).clicked(actions);
        if importing || self.root.button(cx, ids!(save_geometry)).clicked(actions) {
            let source = if importing {
                self.root.text_input(cx, ids!(survey_path)).text()
            } else {
                String::new()
            };
            let edit = if importing {
                String::new()
            } else {
                self.root.text_input(cx, ids!(geometry)).text()
            };
            b.dispatch(Intent::Aisle(Action::ImportRoom { source, edit }));
            return true;
        }
        let reference = self.root.button(cx, ids!(reference)).clicked(actions);
        let revise_deepseek = self.root.button(cx, ids!(revise)).clicked(actions);
        let revise = revise_deepseek || self.root.button(cx, ids!(revise_codex)).clicked(actions);
        let octos = self.root.button(cx, ids!(octos)).clicked(actions);
        if reference || revise || octos || self.root.button(cx, ids!(codex)).clicked(actions) {
            let input = self.root.text_input(cx, ids!(instruction));
            if reference {
                input.set_text(cx, REFERENCE);
            }
            b.dispatch(Intent::Aisle(Action::Prepare {
                task: input.text(),
                planner: if reference {
                    Planner::Reference
                } else if octos || revise_deepseek {
                    Planner::OctosDeepSeek
                } else {
                    Planner::Codex
                },
                revise,
            }));
            return true;
        }
        false
    }
}
