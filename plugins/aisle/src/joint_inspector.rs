use super::*;
use serde_json::Value;

fn joint_details(joint: &Value) -> String {
    let kind = joint["type"].as_str().unwrap_or_default();
    let limits = &joint["limits"];
    let (position_unit, speed_unit, effort_unit) = if kind == "prismatic" {
        ("m", "m/s", "N")
    } else {
        ("rad", "rad/s", "N·m")
    };
    let bound = |key: &str, unit: &str| {
        limits[key]
            .as_f64()
            .map(|v| format!("{v} {unit}"))
            .unwrap_or_else(|| "Unspecified".into())
    };
    let position = if kind == "continuous" {
        "Position · continuous rotation".into()
    } else {
        format!(
            "Lower · {}\nUpper · {}",
            bound("lower", position_unit),
            bound("upper", position_unit)
        )
    };
    format!(
        "{kind}\n\n{position}\nSpeed limit · {}\nEffort limit · {}\n\nParent · {}\nChild · {}",
        bound("velocity", speed_unit),
        bound("effort", effort_unit),
        joint["parent"].as_str().unwrap_or_default(),
        joint["child"].as_str().unwrap_or_default()
    )
}

impl Panel {
    pub(super) fn handle_joint_inspector(
        &mut self,
        cx: &mut Cx,
        actions: &Actions,
        b: &mut dyn Backend,
    ) -> bool {
        if self.root.button(cx, ids!(joint_mode)).clicked(actions) {
            let Some(w) = b.aisle() else { return false };
            self.joint_robot = Some(w.robot);
            self.joint_index = 0;
            self.root.text_input(cx, ids!(joint_query)).set_text(cx, "");
            b.dispatch(Intent::Aisle(Action::Inspect));
            return true;
        }
        if self.joint_robot.is_none() {
            return false;
        }
        if self.root.button(cx, ids!(robot_mode)).clicked(actions)
            || self.root.button(cx, ids!(room_mode)).clicked(actions)
        {
            self.joint_robot = None;
            return false; // Let the regular scene action select the requested mode.
        }
        if self
            .root
            .text_input(cx, ids!(joint_query))
            .changed(actions)
            .is_some()
        {
            self.joint_index = 0;
        } else if self.root.button(cx, ids!(joint_previous)).clicked(actions) {
            self.joint_index = self.joint_index.saturating_sub(1);
        } else if self.root.button(cx, ids!(joint_next)).clicked(actions) {
            self.joint_index += 1;
        } else {
            return false;
        }
        true
    }

    pub(super) fn sync_joint_inspector(&mut self, cx: &mut Cx, w: &Workbench, open: bool) {
        let mut selected = None;
        if open {
            let catalog = w.scene["joints"].as_array();
            let query = self
                .root
                .text_input(cx, ids!(joint_query))
                .text()
                .to_lowercase();
            let matches: Vec<_> = catalog
                .into_iter()
                .flatten()
                .filter(|joint| {
                    let name = joint["name"].as_str().unwrap_or_default().to_lowercase();
                    query.split_whitespace().all(|word| name.contains(word))
                })
                .collect();
            self.joint_index = self.joint_index.min(matches.len().saturating_sub(1));
            self.root
                .button(cx, ids!(joint_previous))
                .set_enabled(cx, self.joint_index > 0);
            self.root
                .button(cx, ids!(joint_next))
                .set_enabled(cx, self.joint_index + 1 < matches.len());
            let total = catalog.map_or(0, Vec::len);
            let count = if let Some(joint) = matches.get(self.joint_index) {
                selected = joint["name"].as_str();
                self.label(cx, ids!(joint_detail), &joint_details(joint));
                if matches.len() == total {
                    format!("Joint {} of {total}", self.joint_index + 1)
                } else {
                    format!(
                        "Match {} of {} · {total} joints",
                        self.joint_index + 1,
                        matches.len()
                    )
                }
            } else {
                self.label(cx, ids!(joint_detail), "");
                if catalog.is_some() {
                    format!("No matching joints · {total} joints")
                } else if w.busy() {
                    "Reading URDF…".into()
                } else {
                    "Joint limits unavailable".into()
                }
            };
            self.label(cx, ids!(joint_count), &count);
            self.label(cx, ids!(joint_name), selected.unwrap_or_default());
        }
        if let Some(mut view) = self.root.robot_view(cx, ids!(robot)).borrow_mut() {
            let visible = view.control_highlights();
            if visible.first().map(|(name, _)| name.as_str()) != selected {
                let joints: Vec<String> = selected.into_iter().map(str::to_owned).collect();
                view.set_joint_highlights(cx, &joints, &[]);
            }
        }
    }
}
