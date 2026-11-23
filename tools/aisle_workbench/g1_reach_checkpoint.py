"""Fit a bounded shoulder position correction to one measured G1 standing episode."""

import hashlib
import json

import numpy as np

from worker import digest, save

METHOD = "g1-shoulder-ridge-v1"
PARQUET = "data/chunk-000/episode_000000.parquet"


def read(path):
    return json.loads(path.read_bytes())


def features(phase, error):
    # The target ramps for one second. The second feature is joint feedback in radians.
    return np.array([1.0, phase, error / 0.04])


def train(dataset, trial, out):
    import pyarrow.parquet as pq

    meta = read(dataset / "meta/aisle.json")
    info = read(dataset / "meta/info.json")
    plan = read(trial / "controller.json")
    result = read(trial / "result.json")
    if (
        result.get("kind") != "standing-inspection"
        or not result.get("task_success")
        or plan.get("robot") != "G1"
        or "reach_checkpoint" in plan
        or meta["trial"] != trial.name
        or meta["controller_hash"] != digest(trial / "controller.json")
        or meta["model_hash"] != plan["model_hash"]
        or info["total_frames"] != 100
        or info["fps"] != 10
        or info["robot_type"] != "g1"
    ):
        raise ValueError(
            "Select the exported episode of a successful G1 standing teacher trial"
        )
    names = info["features"]["observation.state"]["names"]
    if len(names) != 29 or names != info["features"]["action"]["names"]:
        raise ValueError("G1 dataset joint order differs between actions and states")
    j = names.index(plan["joint"])
    data = pq.read_table(dataset / PARQUET).to_pydict()
    state, action = (
        np.asarray(data[k], dtype=float) for k in ("observation.state", "action")
    )
    if (
        state.shape != (100, 29)
        or action.shape != state.shape
        or not np.isfinite(state).all()
        or not np.isfinite(action).all()
        or list(data["frame_index"]) != list(range(100))
    ):
        raise ValueError("Incomplete G1 training episode")
    if digest(trial / "motor-work.npz") != result["motor_work_sha256"]:
        raise ValueError("Teacher motor trace digest differs")
    with np.load(trial / "motor-work.npz", allow_pickle=False) as motors:
        if (
            names != motors["joints"].tolist()
            or not np.allclose(action, motors["target"][19::20], rtol=0, atol=1e-7)
            or not np.allclose(state, motors["q_after"][19::20], rtol=0, atol=1e-7)
        ):
            raise ValueError("Selected dataset differs from the measured teacher trial")
        initial = float(motors["q_before"][0, j])
        # The exported frame shows the pose after this tick. At inference the
        # joint encoder is available before the command. Fit with that same exposure.
        observed_before = motors["q_before"][19::20, j].copy()
    phase = np.minimum(1.0, (np.arange(100) + 1) / info["fps"])
    reference = initial + plan["delta_rad"] * phase
    x = np.array(
        [
            features(t, e)
            for t, e in zip(phase, reference - observed_before, strict=True)
        ]
    )
    y = action[:, j] - reference
    ridge = 1e-5
    coef = np.linalg.solve(x.T @ x + ridge * np.eye(x.shape[1]), x.T @ y)
    if not np.isfinite(coef).all():
        raise ValueError("G1 fit produced invalid coefficients")
    files = {
        str(p.relative_to(dataset)): digest(p)
        for p in sorted(dataset.rglob("*"))
        if p.is_file()
    }
    dataset_hash = hashlib.sha256(
        json.dumps(files, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()
    metadata = dict(
        method=METHOD,
        robot="G1",
        model_hash=plan["model_hash"],
        joint=plan["joint"],
        delta_rad=plan["delta_rad"],
        parent_controller_sha256=meta["controller_hash"],
        dataset_sha256=dataset_hash,
        teacher_trial=trial.name,
        teacher_motor_sha256=result["motor_work_sha256"],
        encoder_timing="before issuing the sampled motor target",
        frames=100,
    )
    save(
        out / "training.json",
        dict(
            **metadata,
            files=files,
            ridge=ridge,
            training_rmse_rad=float(np.sqrt(np.mean((x @ coef - y) ** 2)))
        ),
    )
    save(
        out / "reach-checkpoint.json",
        dict(
            **metadata,
            weights=coef.tolist(),
            training_sha256=digest(out / "training.json")
        ),
    )
    saved = plan | dict(
        reach_checkpoint=digest(out / "reach-checkpoint.json"),
        training_dataset=dataset_hash,
        parent_controller_sha256=meta["controller_hash"],
    )
    save(out / "controller.json", saved)


def load(directory, plan):
    checkpoint = directory / "reach-checkpoint.json"
    if digest(checkpoint) != plan["reach_checkpoint"]:
        raise ValueError(
            "Standing checkpoint digest differs from the selected revision"
        )
    ck = read(checkpoint)
    if (
        ck.get("method") != METHOD
        or any(
            ck.get(k) != plan[k]
            for k in (
                "model_hash",
                "joint",
                "robot",
                "delta_rad",
                "parent_controller_sha256",
            )
        )
        or ck.get("dataset_sha256") != plan.get("training_dataset")
        or ck["training_sha256"] != digest(directory / "training.json")
    ):
        raise ValueError(
            "Standing checkpoint does not match the task, dataset or robot"
        )
    weights = np.asarray(ck["weights"], dtype=float)
    if weights.shape != (3,) or not np.isfinite(weights).all():
        raise ValueError("Invalid standing checkpoint weights")
    return weights
