//! Compare recorded Nano rooms while keeping the controller and scene inputs matched.
use super::{read, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path};

fn verified(path: &Path, file: &str, hash: &Value) -> Result<Vec<u8>, String> {
    let bytes = fs::read(path.join(file)).map_err(|_| format!("Missing {file}"))?;
    if hash.as_str() != Some(&format!("{:x}", Sha256::digest(&bytes))) {
        return Err(format!("{file} differs from its recorded hash"));
    }
    Ok(bytes)
}

struct Room {
    inputs: Value,
    survey: Value,
    ambient: Value,
    friction: Value,
}

impl Room {
    fn load(path: &Path, result: &Value, source: &Value) -> Result<Self, String> {
        let hash = &source["room_revision_sha256"];
        if hash != &result["room_revision_sha256"] {
            return Err("Room identity differs between the run and its verdict".into());
        }
        let decode = |bytes: &[u8]| {
            serde_json::from_slice::<Value>(bytes)
                .map_err(|_| "Invalid archived room JSON".to_owned())
        };
        let mut inputs = decode(&verified(path, "room.json", hash)?)?;
        if inputs["version"] != 1
            || inputs["model_sha256"] != result["model_hash"]
            || inputs["obstacles"].as_array().is_none_or(Vec::is_empty)
        {
            return Err("Missing matching Nano room geometry or model".into());
        }
        let mut survey = decode(&verified(
            path,
            "survey.json",
            &inputs["archived_survey_sha256"],
        )?)?;
        verified(path, "reference-image", &inputs["reference_sha256"])?;
        let fields = inputs.as_object_mut().ok_or("Missing room revision")?;
        for key in [
            "survey_sha256",
            "archived_survey_sha256",
            "parent_sha256",
            "edit",
        ] {
            fields.remove(key);
        }
        let settings = fields
            .get_mut("settings")
            .and_then(Value::as_object_mut)
            .ok_or("Missing room settings")?;
        let ambient = settings.remove("ambient_rgb").unwrap_or(Value::Null);
        let friction = settings.remove("floor_friction").unwrap_or(Value::Null);
        let sheet = survey.as_object_mut().ok_or("Missing survey")?;
        let mut environment = sheet
            .remove("environment")
            .unwrap_or_else(|| serde_json::json!({}));
        let values = environment
            .as_object_mut()
            .ok_or("Invalid survey environment")?;
        if values.remove("ambient_rgb").unwrap_or(Value::Null) != ambient
            || values.remove("floor_friction").unwrap_or(Value::Null) != friction
            || !values.is_empty()
        {
            return Err("Survey environment differs from the simulated room".into());
        }
        Ok(Self {
            inputs,
            survey,
            ambient,
            friction,
        })
    }
}

pub(super) fn eligible(result: &Value) -> bool {
    result["robot"] == "Nano"
        && result["kind"] == "table-transfer"
        && result["task_success"].is_boolean()
        && result["room_revision_sha256"].is_string()
}

struct TrialInputs {
    room: Room,
    source: Value,
    calibration: Value,
}

impl TrialInputs {
    fn load(path: &Path, result: &Value) -> Result<Self, String> {
        if !eligible(result) {
            return Err("Select a scored Nano transfer in a surveyed room".into());
        }
        let mut source = read(path, "source.json").ok_or("Missing run source")?;
        if result["controller_hash"] != source["controller_sha256"]
            || result["model_hash"] != source["model_sha256"]
            || result["seed"] != source["seed"]
        {
            return Err("Controller, model or seed differs from its source record".into());
        }
        verified(path, "controller.json", &source["controller_sha256"])?;
        if source["policy_sha256"]
            .as_str()
            .is_none_or(|s| s.len() != 64)
        {
            return Err("Missing policy identity".into());
        }
        let required_files = [
            "nano_living.py",
            "run_rgb_approach.py",
            "run_rgb_transfer.py",
            "nano_cameras.py",
            "nano_visuals.py",
        ];
        if ["recipe", "nexus", "adapter_hash", "backend"]
            .iter()
            .any(|k| !source[*k].is_string())
            || required_files
                .iter()
                .any(|k| !source["support_code_hashes"][*k].is_string())
        {
            return Err("Missing simulator or support-code identity".into());
        }
        let room = Room::load(path, result, &source)?;
        source
            .as_object_mut()
            .ok_or("Invalid run source")?
            .remove("room_revision_sha256");
        let mut calibration = read(path, "calibration.json").ok_or("Missing run calibration")?;
        if calibration["cameras"]
            .as_object()
            .is_none_or(|rig| rig.is_empty())
        {
            return Err("Missing recorded camera calibration".into());
        }
        // Runs on different hosts can locate the same verified URDF under different paths.
        calibration
            .as_object_mut()
            .ok_or("Invalid run calibration")?
            .remove("model");
        Ok(Self {
            room,
            source,
            calibration,
        })
    }
}

pub(super) fn check_baseline(path: &Path, result: &Value) -> Result<(), String> {
    TrialInputs::load(path, result).map(|_| ())
}

pub(super) fn summary(
    previous: &Path,
    before: &Value,
    current: &Path,
    after: &Value,
) -> Result<String, String> {
    if before["seed"].as_u64().is_none() || before["seed"] != after["seed"] {
        return Err("Layout seeds differ or are missing".into());
    }
    if !["kind", "robot", "model_hash", "requested", "fixture"]
        .iter()
        .all(|k| !before[*k].is_null() && before[*k] == after[*k])
    {
        return Err("Task, robot or fixture differs".into());
    }
    if before["controller_hash"] != after["controller_hash"] {
        return Err("Controllers differ".into());
    }
    let first = TrialInputs::load(previous, before)?;
    let second = TrialInputs::load(current, after)?;
    if first.source != second.source {
        return Err("Execution settings, policy or simulator build differs".into());
    }
    if first.calibration != second.calibration {
        return Err("Recorded calibrations differ".into());
    }
    let (a, b) = (first.room, second.room);
    if a.inputs != b.inputs || a.survey != b.survey {
        return Err("Geometry, survey or camera settings differ".into());
    }
    let display = |v: &Value| {
        if v.is_null() {
            "fixture default".into()
        } else {
            v.to_string()
        }
    };
    let mut deltas = Vec::new();
    for (label, previous, current) in [
        ("Ambient RGB", &a.ambient, &b.ambient),
        ("Floor friction", &a.friction, &b.friction),
    ] {
        if previous != current {
            deltas.push(format!(
                "{label} {} → {}",
                display(previous),
                display(current)
            ));
        }
    }
    if deltas.is_empty() {
        deltas.push("Same ambient RGB and floor friction".to_owned());
    }
    Ok(deltas.join(" · "))
}

#[derive(serde::Serialize)]
pub struct Measurements {
    pub outcome: String,
    pub detail: String,
}

impl Measurements {
    pub(super) fn new(delta: &str, baseline: &Value, current: &Value) -> Self {
        let verdict = |r: &Value| {
            if r["task_success"] == true {
                "passed"
            } else {
                "failed"
            }
        };
        let outcome = format!(
            "Baseline {} · Current {}",
            verdict(baseline),
            verdict(current)
        );
        let mut rows = vec![delta.to_owned()];
        if let Some(colors) = current["requested"].as_array() {
            for color in colors.iter().filter_map(Value::as_str) {
                let measurement = |r: &Value| {
                    let cube = &r["cubes"][color];
                    let state = match cube["passed"].as_bool() {
                        Some(true) => "settled",
                        Some(false) => "failed",
                        None => "unscored",
                    };
                    cube["nearest_table_edge_m"].as_f64().map_or_else(
                        || format!("{state} (gap unmeasured)"),
                        |gap| format!("{state} ({:.1} mm)", 1000.0 * gap),
                    )
                };
                rows.push(format!(
                    "{color} · baseline {} → current {} · nearest table edge",
                    measurement(baseline),
                    measurement(current)
                ));
            }
        }
        rows.push(format!(
            "Physics steps {} → {}",
            baseline["physics_ticks"], current["physics_ticks"]
        ));
        Self {
            outcome,
            detail: rows.join("\n"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn put(dir: &Path, file: &str, value: Value) -> Value {
        let bytes = serde_json::to_vec(&value).unwrap();
        fs::write(dir.join(file), &bytes).unwrap();
        json!(format!("{:x}", Sha256::digest(bytes)))
    }

    fn trial(dir: &Path, ambient: Value, friction: f64) -> Value {
        fs::create_dir_all(dir).unwrap();
        let controller = put(dir, "controller.json", json!({"requested":["red"]}));
        let env = json!({"ambient_rgb":ambient, "floor_friction":friction});
        let survey = put(
            dir,
            "survey.json",
            json!({"image":"reference-image","environment":env}),
        );
        let photo = put(dir, "reference-image", json!("photo"));
        let room = put(
            dir,
            "room.json",
            json!({"version":1,"obstacles":[{"name":"ottoman"}],"model_sha256":"urdf",
            "settings":env,"reference_sha256":photo,"archived_survey_sha256":survey}),
        );
        put(
            dir,
            "source.json",
            json!({"room_revision_sha256":room,"seed":0,"controller_sha256":controller,"model_sha256":"urdf",
            "recipe":"r","nexus":"n","adapter_hash":"h","backend":"metal","support_code_hashes":{"nano_living.py":"1","run_rgb_approach.py":"2","run_rgb_transfer.py":"3","nano_cameras.py":"4","nano_visuals.py":"5"},"policy_sha256":"a".repeat(64)}),
        );
        put(dir, "calibration.json", json!({"cameras":{"head":"mount"}}));
        let result = json!({"seed":0,"controller_hash":controller,"room_revision_sha256":room,"model_hash":"urdf",
            "fixture":"living-room","requested":["red"],"robot":"Nano","kind":"table-transfer","task_success":true});
        put(dir, "result.json", result.clone());
        result
    }

    #[test]
    fn pin_survives_restart_and_leaves_joint_comparisons_available() {
        use super::super::{Action, Workbench};
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let baseline = root.join("01-Nano-run");
        trial(&baseline, json!([0.6, 0.6, 0.6]), 0.65);
        let mut wb = Workbench::with_root(root.to_owned());
        wb.dispatch(Action::ToggleRoomBaseline);
        let pinned = wb.room_baseline.as_ref().unwrap().result_sha256.clone();
        for (name, delta) in [("02-Nano-run", 0.01), ("03-Nano-run", 0.03)] {
            let dir = root.join(name);
            fs::create_dir(&dir).unwrap();
            put(
                &dir,
                "result.json",
                json!({"kind":"inspection","robot":"Nano","fixture":"fixed","model_hash":"urdf",
                 "joint":"shoulder","requested_delta":0.04,"measured_delta":delta,"task_success":true}),
            );
        }
        let mut restored = Workbench::with_root(root.to_owned());
        assert_eq!(
            restored.room_baseline.as_ref().unwrap().result_sha256,
            pinned
        );
        restored.select_run(root.join("03-Nano-run"));
        assert!(restored.room_pair_note.is_none());
        assert!((restored.comparison.unwrap() - 0.02).abs() < 1e-9);
        let mut bytes = fs::read(baseline.join("result.json")).unwrap();
        bytes.push(b' ');
        fs::write(baseline.join("result.json"), bytes).unwrap();
        restored.select_run(baseline.clone());
        assert!(restored
            .room_pair_note
            .as_deref()
            .unwrap()
            .contains("verdict changed"));
        restored.dispatch(Action::ToggleRoomBaseline);
        assert!(restored.room_baseline.is_none());
        fs::remove_file(baseline.join("reference-image")).unwrap();
        restored.dispatch(Action::ToggleRoomBaseline);
        assert!(restored.room_baseline.is_none());
        assert!(restored
            .error
            .as_deref()
            .unwrap()
            .contains("Missing reference-image"));
        assert!(Workbench::with_root(root.to_owned())
            .room_baseline
            .is_none());
    }

    #[test]
    fn clears_persisted_pin_after_all_runs_were_removed() {
        use super::super::{Action, Workbench};
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let base = root.join("01-Nano-run");
        trial(&base, json!([0.5, 0.5, 0.5]), 0.8);
        let mut wb = Workbench::with_root(root.to_owned());
        wb.dispatch(Action::ToggleRoomBaseline);
        assert!(wb.room_baseline.is_some());
        fs::remove_dir_all(&base).unwrap();
        let mut reopened = Workbench::with_root(root.to_owned());
        assert!(reopened.selected.is_none());
        reopened.dispatch(Action::ToggleRoomBaseline);
        assert!(reopened.room_baseline.is_none() && reopened.error.is_none());
        assert!(Workbench::with_root(root.to_owned())
            .room_baseline
            .is_none());
    }

    #[test]
    fn pairs_only_controlled_room_changes() {
        let temp = tempfile::tempdir().unwrap();
        let (a, b) = (temp.path().join("a"), temp.path().join("b"));
        let baseline = trial(&a, json!([0.6, 0.6, 0.6]), 0.65);
        let result = trial(&b, json!([0.4, 0.4, 0.4]), 0.8);
        let label = summary(&a, &baseline, &b, &result).unwrap();
        assert!(label.contains("Floor friction 0.65 → 0.8") && label.contains("Ambient RGB"));
        for (file, key, value) in [
            ("source.json", "observation_period_s", json!(1)),
            ("calibration.json", "wrist_clearance", json!("different")),
            ("room.json", "obstacles", json!([])),
            ("survey.json", "description", json!("new")),
            ("controller.json", "requested", json!(["blue"])),
        ] {
            let original = read(&b, file).unwrap();
            let mut changed = original.clone();
            changed[key] = value;
            put(&b, file, changed);
            assert!(summary(&a, &baseline, &b, &result).is_err(), "{file}");
            put(&b, file, original);
        }
        let mut bad = result.clone();
        bad["seed"] = json!(1);
        assert!(summary(&a, &baseline, &b, &bad)
            .unwrap_err()
            .contains("seed"));
        // A fully rehashed geometry edit is still an uncontrolled change.
        let mut room = read(&b, "room.json").unwrap();
        room["obstacles"][0]["height"] = json!(0.8);
        let hash = put(&b, "room.json", room);
        let mut source = read(&b, "source.json").unwrap();
        source["room_revision_sha256"] = hash.clone();
        put(&b, "source.json", source);
        let mut new_result = result;
        new_result["room_revision_sha256"] = hash;
        assert!(summary(&a, &baseline, &b, &new_result)
            .unwrap_err()
            .contains("Geometry"));
    }
}
