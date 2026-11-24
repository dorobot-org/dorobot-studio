"""Train a bounded wrist-image carry decision from an annotated Nano LeRobot episode."""

import hashlib
import json
import sys

import cv2
import numpy as np
import pyarrow.parquet as pq
from scipy.optimize import minimize
from scipy.special import expit

from worker import ROOT, digest, save

sys.path.insert(0, str(ROOT / "tools/aisle-validation"))
from nano_grasp_estimator import (
    FEATURES,
    METHOD,
    checkpoint_bytes,
    features,
    parameters,
)


def train(dataset, trial, out):
    def read(path):
        return json.loads(path.read_bytes())

    plan = read(trial / "controller.json")
    result = read(trial / "result.json")
    metadata = read(dataset / "meta/aisle.json")
    info = read(dataset / "meta/info.json")
    record = read(trial / "recording.json")
    fields = info["features"]
    if (
        plan.get("kind") != "table-transfer"
        or plan.get("robot") != "Nano"
        or "grasp_checkpoint" in plan
        or not result.get("task_success")
        or result.get("kind") != "table-transfer"
        or info["robot_type"] != "adora_nano"
        or metadata["trial"] != trial.name
        or metadata["controller_hash"] != digest(trial / "controller.json")
        or metadata["controller_hash"] != result["controller_hash"]
        or metadata["model_hash"] != plan["model_hash"]
        or info["fps"] != 5
        or info["total_episodes"] != 1
        or "annotation.gripper_carry" not in fields
    ):
        raise ValueError(
            "Select the exported episode of a Nano transfer with synchronized physical carry labels"
        )
    names = fields["observation.state"]["names"]
    colors = fields["annotation.gripper_carry"]["names"]
    rows = record["samples"]
    count = len(rows)
    if (
        names != record["joints"]
        or names != fields["action"]["names"]
        or len(names) != 17
        or colors != record.get("label_colors")
        or colors != plan["color_order"].split(",")
        or count != info["total_frames"]
    ):
        raise ValueError(
            "Nano training joints, color labels or frame count differ from the teacher"
        )
    data = pq.read_table(dataset / "data/chunk-000/episode_000000.parquet").to_pydict()
    state = np.asarray(data["observation.state"], dtype=float)
    labels = np.asarray(data["annotation.gripper_carry"])
    if (
        list(data["frame_index"]) != list(range(count))
        or state.shape != (count, 17)
        or labels.shape != (count, len(colors))
        or not np.isfinite(state).all()
        or not np.array_equal(labels, np.asarray([r["airborne_in_jaw"] for r in rows]))
        or not np.allclose(
            state,
            np.asarray([r["state"] for r in rows], dtype=np.float32),
            rtol=0,
            atol=0,
        )
    ):
        raise ValueError(
            "Nano training labels or encoders disagree with the recorded trial"
        )
    video = cv2.VideoCapture(
        str(
            dataset
            / "videos/chunk-000/observation.images.left_wrist/episode_000000.mp4"
        )
    )
    vectors = []
    grip, roll = (names.index(n) for n in ("left_j6_gripper", "left_j5_roll"))
    try:
        for q in state:
            ok, bgr = video.read()
            if not ok or bgr.shape != (360, 480, 3):
                raise ValueError("Incomplete Nano wrist video")
            hsv = cv2.cvtColor(bgr, cv2.COLOR_BGR2HSV)
            vectors.extend(features(hsv, c, q[grip], q[roll]) for c in colors)
        if video.read()[0]:
            raise ValueError("Nano wrist video contains extra frames")
    finally:
        video.release()
    x, y = np.asarray(vectors), labels.reshape(-1).astype(float)
    positives = int(y.sum())
    negatives = len(y) - positives
    if min(positives, negatives) < 10:
        raise ValueError(
            "Nano carry training needs at least ten physically held and ten unheld examples"
        )
    mean, scale = x.mean(axis=0), np.maximum(x.std(axis=0), 0.01)
    x = (x - mean) / scale
    sample_weight = np.where(y == 1, 0.5 / positives, 0.5 / negatives)
    ridge = 0.01

    def objective(theta):
        score = x @ theta[:-1] + theta[-1]
        residual = sample_weight * (expit(score) - y)
        loss = (
            np.sum(sample_weight * (np.logaddexp(0, score) - y * score))
            + ridge * (theta[:-1] @ theta[:-1]) / 2
        )
        return loss, np.r_[x.T @ residual + ridge * theta[:-1], residual.sum()]

    fit = minimize(objective, np.zeros(x.shape[1] + 1), jac=True, method="L-BFGS-B")
    if not fit.success or not np.isfinite(fit.x).all():
        raise ValueError("Nano grasp checkpoint training did not converge")
    predicted = x @ fit.x[:-1] + fit.x[-1] >= 0
    files = {
        str(p.relative_to(dataset)): digest(p)
        for p in sorted(dataset.rglob("*"))
        if p.is_file()
    }
    dataset_hash = hashlib.sha256(
        json.dumps(files, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()
    lineage = dict(
        method=METHOD,
        model_hash=plan["model_hash"],
        teacher_controller_sha256=metadata["controller_hash"],
        dataset_sha256=dataset_hash,
    )
    save(
        out / "training.json",
        dict(
            **lineage,
            teacher_trial=trial.name,
            frames=count,
            colors=colors,
            positives=positives,
            negatives=negatives,
            features=list(FEATURES),
            ridge=ridge,
            files=files,
            training_confusion=dict(
                tp=int(sum(predicted & (y == 1))),
                fp=int(sum(predicted & (y == 0))),
                tn=int(sum(~predicted & (y == 0))),
                fn=int(sum(~predicted & (y == 1))),
            ),
        ),
    )
    ck = dict(
        **lineage,
        features=list(FEATURES),
        mean=mean.tolist(),
        scale=scale.tolist(),
        weights=fit.x[:-1].tolist(),
        bias=float(fit.x[-1]),
        training_sha256=digest(out / "training.json"),
    )
    (out / "grasp-checkpoint.json").write_bytes(checkpoint_bytes(ck))
    revised = plan | dict(
        grasp_estimator=ck,
        grasp_checkpoint=digest(out / "grasp-checkpoint.json"),
        training_dataset=dataset_hash,
        parent_controller_sha256=metadata["controller_hash"],
    )
    parameters(revised)
    save(out / "controller.json", revised)
