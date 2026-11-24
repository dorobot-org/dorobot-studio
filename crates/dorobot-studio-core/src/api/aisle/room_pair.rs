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

pub(super) fn summary(previous: &Path, current: &Path, after: &Value) -> Result<String, String> {
    let before = read(previous, "result.json").ok_or("Missing baseline verdict")?;
    if before["seed"].as_u64().is_none() || before["seed"] != after["seed"] {
        return Err("Layout seeds differ or are missing".into());
    }
    if !["kind", "robot", "model_hash", "requested", "fixture"]
        .iter()
        .all(|k| !before[*k].is_null() && before[*k] == after[*k])
    {
        return Err("Task, robot or fixture differs".into());
    }
    let mut old = read(previous, "source.json").ok_or("Missing baseline source")?;
    let mut new = read(current, "source.json").ok_or("Missing current source")?;
    if before["controller_hash"] != after["controller_hash"] {
        return Err("Controllers differ".into());
    }
    for (p, result, source) in [(previous, &before, &old), (current, after, &new)] {
        if result["controller_hash"] != source["controller_sha256"]
            || result["model_hash"] != source["model_sha256"]
            || result["seed"] != source["seed"]
        {
            return Err("Controller, model or seed differs from its source record".into());
        }
        verified(p, "controller.json", &source["controller_sha256"])?;
        if source["policy_sha256"]
            .as_str()
            .is_none_or(|s| s.len() != 64)
        {
            return Err("Missing policy identity".into());
        }
    }
    let a = Room::load(previous, &before, &old)?;
    let b = Room::load(current, after, &new)?;
    old.as_object_mut()
        .ok_or("Missing baseline source")?
        .remove("room_revision_sha256");
    new.as_object_mut()
        .ok_or("Missing current source")?
        .remove("room_revision_sha256");
    if old != new {
        return Err("Execution settings, policy or simulator build differs".into());
    }
    if a.inputs != b.inputs || a.survey != b.survey {
        return Err("Geometry, survey or camera settings differ".into());
    }
    let optics = |p| -> Result<Value, String> {
        let mut cal = read(p, "calibration.json").ok_or("Missing run calibration")?;
        // The local model path may differ between hosts. The URDF hash above identifies it.
        cal.as_object_mut()
            .ok_or("Invalid run calibration")?
            .remove("model");
        Ok(cal)
    };
    if optics(previous)? != optics(current)? {
        return Err("Recorded calibrations differ".into());
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
            "policy_sha256":"a".repeat(64)}),
        );
        put(dir, "calibration.json", json!({"cameras":{"head":"mount"}}));
        let result = json!({"seed":0,"controller_hash":controller,"room_revision_sha256":room,"model_hash":"urdf",
            "fixture":"living-room","requested":["red"],"robot":"Nano","kind":"table-transfer"});
        put(dir, "result.json", result.clone());
        result
    }

    #[test]
    fn pairs_only_controlled_room_changes() {
        let temp = tempfile::tempdir().unwrap();
        let (a, b) = (temp.path().join("a"), temp.path().join("b"));
        trial(&a, json!([0.6, 0.6, 0.6]), 0.65);
        let result = trial(&b, json!([0.4, 0.4, 0.4]), 0.8);
        let label = summary(&a, &b, &result).unwrap();
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
            assert!(summary(&a, &b, &result).is_err(), "{file}");
            put(&b, file, original);
        }
        let mut bad = result.clone();
        bad["seed"] = json!(1);
        assert!(summary(&a, &b, &bad).unwrap_err().contains("seed"));
        // A fully rehashed geometry edit is still an uncontrolled change.
        let mut room = read(&b, "room.json").unwrap();
        room["obstacles"][0]["height"] = json!(0.8);
        let hash = put(&b, "room.json", room);
        let mut source = read(&b, "source.json").unwrap();
        source["room_revision_sha256"] = hash.clone();
        put(&b, "source.json", source);
        let mut new_result = result;
        new_result["room_revision_sha256"] = hash;
        assert!(summary(&a, &b, &new_result)
            .unwrap_err()
            .contains("Geometry"));
    }
}
