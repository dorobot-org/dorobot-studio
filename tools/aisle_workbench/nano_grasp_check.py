"""Replay the trained Nano carry decision at every online grasp check."""

import json
from pathlib import Path
import sys

import numpy as np
from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "aisle-validation"))
from nano_grasp_estimator import parameters, predict


def verify_decisions(trial):
    plan = json.loads((trial / "controller.json").read_bytes())
    model = parameters(plan)
    if model is None:
        raise ValueError("Select a trial with a trained Nano grasp detector")
    result = json.loads((trial / "result.json").read_bytes())
    if result["grasp_checkpoint"] != plan["grasp_checkpoint"]:
        raise ValueError("Trial and controller grasp checkpoints differ")
    actions = [
        (row["tick"], row["color"], row["visible"])
        for line in (trial / "actions.jsonl").read_text().splitlines()
        if (row := json.loads(line))["op"] == "jaw observation"
    ]
    diagnostics = [
        json.loads(line)
        for line in (trial / "jaw-diagnostics.jsonl").read_text().splitlines()
    ]
    if not actions or len(actions) != len(diagnostics):
        raise ValueError("Incomplete Nano grasp decision log")
    counts = {"tp": 0, "tn": 0, "fp": 0, "fn": 0}
    for action, row in zip(actions, diagnostics, strict=True):
        tick, color, visible = action
        if action != (row["tick"], row["color"], row["visible"]):
            raise ValueError("Grasp decision disagrees with actuator log")
        with Image.open(trial / f"jaw-{tick:05}-{color}.png") as image:
            decision = predict(
                model,
                np.asarray(image.convert("RGB")),
                color,
                row["gripper_rad"],
                row["roll_rad"],
            )
        if decision != visible:
            raise ValueError(
                "Saved wrist exposure did not reproduce the online decision"
            )
        label = row["airborne_in_jaw"]
        counts[
            {
                (True, True): "tp",
                (False, False): "tn",
                (True, False): "fp",
                (False, True): "fn",
            }[decision, label]
        ] += 1
    return dict(
        decisions=len(actions), confusion=counts, checkpoint=plan["grasp_checkpoint"]
    )


def native_journey(args, teacher, result, checks, page, click, done, grab):
    click("aisle_evidence.train_grasp")
    trained = done(
        lambda w: not w["busy"] and w["controller"].get("grasp_checkpoint"),
        "Trained Nano wrist detector from the exported episode",
        180,
    )
    plan = trained["controller"]
    assert plan["parent_controller_sha256"] == result["controller_hash"]
    assert plan["training_dataset"] == plan["grasp_estimator"]["dataset_sha256"]
    page("task")
    grab("trained-grasp-task")
    page("scene")
    evaluation_seed = (args.layout + 1) % 3
    click("aisle_scene.layout" + str(evaluation_seed))
    done(
        lambda w: w["layout_seed"] == evaluation_seed,
        "Selected a different transfer layout",
    )
    page("run")
    click("aisle_run.start_run")
    learned = done(
        lambda w: not w["busy"]
        and w["result"].get("grasp_checkpoint") == plan["grasp_checkpoint"],
        "Ran trained wrist detector on a different Nexus layout",
        1200,
    )
    trial = Path(learned["selected"])
    assert trial != teacher and learned["result"]["seed"] == evaluation_seed
    assert learned["result"]["training_dataset"] == plan["training_dataset"]
    checks[-1].update(replay=verify_decisions(trial), measured=learned["result"])
    page("evidence")
    grab("trained-grasp-evidence")
    click("aisle_evidence.previous")
    done(lambda w: w["selected"] == str(teacher), "Teacher export reopened")
