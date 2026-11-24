use dorobot_studio_core::{
    api::{
        aisle::{Action, Planner, RgbTiming, Robot, Workbench, REFERENCE},
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
                Section{text:"Prepared controller"} detail := Body{}
                Bar{older := mod.widgets.ux.ToolButton{text:"Earlier revision"} newer := mod.widgets.ux.ToolButton{text:"Later revision"}}
                revision_actions := Bar{revise := mod.widgets.ux.ToolButton{text:"Revise with DeepSeek"} revise_codex := mod.widgets.ux.ToolButton{text:"Revise with Codex"}}
            }
            model_card := Card{width:Fill height:Fill preview_title := Section{text:"Robot preview"} robot := RobotView{width:Fill height:Fill keyboard_controls:false} onboard_photo := VideoPlayer{width:Fill height:Fill}}
        }
        status := Body{}
    }
    mod.widgets.AisleScene=Page{
        Title{text:"Simulation scene"} context_label := Section{}
        scene_modes := Bar{robot_mode := mod.widgets.ux.ToolButton{text:"Robot model"} room_mode := mod.widgets.ux.ToolButton{text:"Room survey"}}
        row := View{width:Fill height:Fill flow:Right spacing:12.0
            model_card := Card{width:Fill height:Fill Section{text:"URDF model"} robot := RobotView{width:Fill height:Fill keyboard_controls:false}}
            photo_card := Card{width:Fill height:Fill Section{text:"Surveyed floor footprint"} photo := VideoPlayer{width:Fill height:Fill}}
            survey_card := Card{width:295 height:Fill
                Section{text:"Room survey file"} survey_path := TextInput{width:Fill height:38 empty_text:"Absolute path to survey.json"}
                import_survey := mod.widgets.ux.PrimaryButton{text:"Import survey"}
                room_detail := Body{}
                Section{text:"Edit obstacle · metres"} geometry := TextInput{width:Fill height:38 empty_text:"name x y height"}
                save_geometry := mod.widgets.ux.ToolButton{text:"Save revision"}
                default_room := mod.widgets.ux.ToolButton{text:"Use default room"}
            }
            inspector := Card{width:285 height:Fill Section{text:"Simulation fixture"} fixture_kind := Body{}
                Bar{standard := mod.widgets.ux.ToolButton{text:"Standard"} compliant := mod.widgets.ux.ToolButton{text:"Compliant"}}
                standing_fixtures := Bar{
                    level := mod.widgets.ux.ToolButton{text:"Level floor"}
                    impact := mod.widgets.ux.ToolButton{text:"Impact test"}
                }
                fixture := Body{}
                layouts := View{width:Fill height:Fit flow:Down spacing:6.0 Section{text:"Development layout"}
                    Bar{layout0 := mod.widgets.ux.ToolButton{text:"0"} layout1 := mod.widgets.ux.ToolButton{text:"1"} layout2 := mod.widgets.ux.ToolButton{text:"2"}}
                }
                rgb_delivery := View{width:Fill height:Fit flow:Down spacing:6.0 Section{text:"Onboard camera timing"}
                    Bar{on_time := mod.widgets.ux.ToolButton{text:"On time"} late_rgb := mod.widgets.ux.ToolButton{text:"Delay"} frozen_rgb := mod.widgets.ux.ToolButton{text:"Freeze"}}
                }
                inspect := mod.widgets.ux.PrimaryButton{text:"Inspect model"} Section{text:"Model and motion limits"} detail := Body{}
            }
        }
        status := Body{}
    }
    mod.widgets.AisleRun=Page{
        Title{text:"Live simulation"} context_label := Section{}
        row := View{width:Fill height:Fill flow:Right spacing:12.0
            viewport := Card{width:Fill height:Fill camera_label := Section{} camera := VideoPlayer{width:Fill height:Fill}}
            inspector := Card{width:275 height:Fill measure_label := Section{} detail := Body{}
                start_run := mod.widgets.ux.PrimaryButton{text:"Start run"} stop_run := mod.widgets.ux.StopButton{text:"Stop"}
            }
        }
        status := Body{}
    }
    mod.widgets.AisleEvidence=Page{
        Title{text:"Run evidence"} context_label := Section{}
        Bar{trial_mode := mod.widgets.ux.ToolButton{text:"Trial"} evaluation_mode := mod.widgets.ux.ToolButton{text:"Batch evaluation"}}
        cohort := Card{width:Fill height:Fill
            Section{text:"Evaluation archive"} archive_path := TextInput{width:Fill height:38 empty_text:"Path to the ZIP beside its preregistered suite.json"}
            verify_archive := mod.widgets.ux.PrimaryButton{text:"Verify evaluation"}
            cohort_verdict := Title{} cohort_detail := Body{} failure_detail := Body{}
            failure_bar := Bar{earlier_failure := mod.widgets.ux.ToolButton{text:"Previous failure"} later_failure := mod.widgets.ux.ToolButton{text:"Next failure"}}
        }
        summary := Card{width:Fill height:Fit Section{text:"Task outcome"} verdict := Title{} detail := Body{}}
        plot := Card{width:Fill height:Fill trace_label := Section{} trace := Spark{width:Fill height:Fill} scale := Section{}}
        trial_bar := Bar{previous := mod.widgets.ux.ToolButton{text:"Previous run"} next := mod.widgets.ux.ToolButton{text:"Next run"}
            folder := mod.widgets.ux.ToolButton{text:"Open run folder"}
            export := mod.widgets.ux.PrimaryButton{text:"Export episode"} replay := mod.widgets.ux.PrimaryButton{text:"Open in Replay"} train_reach := mod.widgets.ux.ToolButton{text:"Train shoulder correction"}}
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
}
impl Panel {
    fn label(&self, cx: &mut Cx, id: &[LiveId], s: &str) {
        self.root.label(cx, id).set_text(cx, s);
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
    fn run(&mut self, cx: &mut Cx, w: &Workbench) {
        let current = w
            .live
            .as_ref()
            .zip(w.progress["step"].as_u64())
            .map(|(p, i)| (p.clone(), i));
        let standing = w.progress["kind"] == "standing-inspection";
        let transfer = w.progress["kind"] == "table-transfer";
        let mobile = transfer || w.progress["kind"] == "visual-approach";
        let height = if mobile { 360 } else { 320 };
        self.label(
            cx,
            ids!(camera_label),
            if mobile {
                "Onboard head camera · RGB"
            } else {
                "External operator camera"
            },
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
        if self.frame != current {
            let camera = self.root.video_player(cx, ids!(camera));
            if let Some((dir, step)) = &current {
                if let Ok(raw) = std::fs::read(dir.join("preview.rgb")) {
                    if raw.len() == 4 + 480 * height * 3
                        && u32::from_be_bytes(raw[..4].try_into().unwrap()) as u64 + 1 == *step
                    {
                        camera.set_frame(cx, 480, height, &raw[4..]);
                        self.frame = current;
                    }
                }
            } else {
                camera.clear(cx);
                self.frame = None;
            }
        }
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
            format!(
                "{} cube · {}
RGB distance {}
Requested {:.2} m",
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
                    (ids!(photo_card), Themed::Bg),
                    (ids!(cohort), Themed::Bg),
                    (ids!(survey_card), Themed::Bg),
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
                if mobile {
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
                self.root.widget(cx, ids!(robot)).set_visible(cx, !capture);
                self.root
                    .widget(cx, ids!(onboard_photo))
                    .set_visible(cx, capture);
                self.label(
                    cx,
                    ids!(preview_title),
                    if capture {
                        "Head camera at task authoring"
                    } else {
                        "Robot preview"
                    },
                );
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
                        "Move floor cubes to the coffee table\n{} · {} · revision {}/{}",
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
                self.root
                    .button(cx, ids!(older))
                    .set_enabled(cx, !w.busy() && index.is_some_and(|i| i > 0));
                self.root.button(cx, ids!(newer)).set_enabled(
                    cx,
                    !w.busy() && index.is_some_and(|i| i + 1 < w.revisions.len()),
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
                let editing = mobile && w.room_editor;
                self.root
                    .widget(cx, ids!(scene_modes))
                    .set_visible(cx, mobile);
                self.root
                    .button(cx, ids!(room_mode))
                    .set_enabled(cx, !w.busy() && !editing);
                self.root
                    .button(cx, ids!(robot_mode))
                    .set_enabled(cx, !w.busy() && editing);
                for id in [ids!(model_card), ids!(inspector)] {
                    self.root.widget(cx, id).set_visible(cx, !editing);
                }
                for id in [ids!(photo_card), ids!(survey_card)] {
                    self.root.widget(cx, id).set_visible(cx, editing);
                }
                if editing {
                    let mut text = vec![w.room_info["description"]
                        .as_str()
                        .unwrap_or("Default living room")
                        .to_string()];
                    for b in w.room_info["obstacles"].as_array().into_iter().flatten() {
                        let name = b["name"].as_str().unwrap_or("").trim_start_matches("room_");
                        text.push(format!(
                            "{name} · x {:.2}, y {:.2}, height {:.2}",
                            b["position"][0].as_f64().unwrap_or_default(),
                            b["position"][1].as_f64().unwrap_or_default(),
                            b["size"][2].as_f64().unwrap_or_default()
                        ));
                    }
                    self.label(cx, ids!(room_detail), &text.join("\n"));
                    self.root
                        .button(cx, ids!(import_survey))
                        .set_enabled(cx, !w.busy());
                    for id in [ids!(save_geometry), ids!(default_room)] {
                        self.root
                            .button(cx, id)
                            .set_enabled(cx, !w.busy() && w.room_path.is_some());
                    }
                    if self.room_preview != w.room_path {
                        let player = self.root.video_player(cx, ids!(photo));
                        player.clear(cx);
                        if let Some(raw) = w
                            .room_path
                            .as_ref()
                            .and_then(|p| std::fs::read(p.with_file_name("room-preview.rgb")).ok())
                        {
                            if raw.len() == 480 * 320 * 3 {
                                player.set_frame(cx, 480, 320, &raw);
                            }
                        }
                        self.room_preview = w.room_path.clone();
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
                self.root
                    .widget(cx, ids!(layouts))
                    .set_visible(cx, mobile || (w.robot == Robot::Nano && w.onboard_context));
                self.root
                    .widget(cx, ids!(rgb_delivery))
                    .set_visible(cx, kind == "visual-approach");
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
                for (id, seed) in [
                    (ids!(layout0) as &[LiveId], 0),
                    (ids!(layout1), 1),
                    (ids!(layout2), 2),
                ] {
                    self.root
                        .button(cx, id)
                        .set_enabled(cx, !w.busy() && w.layout_seed != seed);
                }
                for id in [ids!(standard), ids!(compliant)] {
                    self.root
                        .widget(cx, id)
                        .set_visible(cx, !mobile && !standing);
                }
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
                for id in [ids!(summary), ids!(plot), ids!(trial_bar)] {
                    self.root.widget(cx, id).set_visible(cx, !w.show_evaluation);
                }
                self.root
                    .widget(cx, ids!(cohort))
                    .set_visible(cx, w.show_evaluation);
                self.root
                    .button(cx, ids!(trial_mode))
                    .set_enabled(cx, !w.busy() && w.show_evaluation);
                self.root
                    .button(cx, ids!(evaluation_mode))
                    .set_enabled(cx, !w.busy() && !w.show_evaluation);
                if w.show_evaluation {
                    let result = &w.evaluation;
                    let g1 = result["profile"] == "g1-standing-reach-v1";
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
                    let failures = result["failures"].as_array();
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
                    let (title, detail) = if result["cases"].is_number() {
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
                self.label(cx, ids!(verdict), verdict);
                let text = if transfer {
                    let mut lines = vec![];
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
                    let timing = &w.evidence["sensor_timing"];
                    if let Some(fixture) = timing["fixture"].as_str() {
                        let held: u64 = timing["rejected"]
                            .as_object()
                            .into_iter()
                            .flat_map(|m| m.values())
                            .filter_map(|n| n.as_u64())
                            .sum();
                        detail.push_str(&format!("\nCamera timing {fixture} · {held} withheld observations · {} recoveries", timing["recoveries"]));
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
                    comparison.clone()
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
                self.label(cx, ids!(detail), &text);
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
        let Some(_) = b.aisle() else {
            return false;
        };
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
            (ids!(on_time), Action::RgbTiming(RgbTiming::Normal)),
            (ids!(late_rgb), Action::RgbTiming(RgbTiming::Intermittent)),
            (ids!(frozen_rgb), Action::RgbTiming(RgbTiming::Disconnected)),
            (ids!(trial_mode), Action::ShowEvaluation(false)),
            (ids!(evaluation_mode), Action::ShowEvaluation(true)),
            (ids!(earlier_failure), Action::SelectFailure(false)),
            (ids!(later_failure), Action::SelectFailure(true)),
            (ids!(room_mode), Action::EditRoom(true)),
            (ids!(robot_mode), Action::EditRoom(false)),
            (ids!(default_room), Action::DefaultRoom),
            (ids!(older), Action::OlderPlan),
            (ids!(newer), Action::NewerPlan),
            (ids!(start_run), Action::Run),
            (ids!(stop_run), Action::Stop),
            (ids!(previous), Action::Previous),
            (ids!(next), Action::Next),
            (ids!(folder), Action::Open),
            (ids!(export), Action::Export),
            (ids!(train_reach), Action::TrainReach),
        ] {
            if self.root.button(cx, id).clicked(actions) {
                b.dispatch(Intent::Aisle(act));
                return true;
            }
        }
        if self.root.button(cx, ids!(verify_archive)).clicked(actions) {
            let path = self.root.text_input(cx, ids!(archive_path)).text();
            b.dispatch(Intent::Aisle(Action::Evaluate(path.trim().to_owned())));
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
