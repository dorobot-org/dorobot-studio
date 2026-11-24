"""Summarize a two-lease native Makepad run on a registered Linux Nano worker."""

import argparse
from collections import defaultdict
import hashlib
import json
from pathlib import Path


def read(path):
    return json.loads(path.read_bytes())


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def summarize(folder, live_folder=None):
    verified = folder / "verified"
    proof = read(verified / "journey.json")[-1]
    result = proof["result"]
    assert isinstance(result["task_success"], bool) and result["requested"] == ["red"]
    assert result["actuator_audit"]["passed"]
    unsupported = sum(
        row["last_step"] - row["first_step"] + 1
        for row in result["unsupported_wheel_ranges"]
    )
    assert result["physics_ticks"] == result["wheel_supported_ticks"] + unsupported
    if unsupported:
        assert not result["task_success"]
    trials = {}
    for trial in (folder / "state/aisle").glob("*-Nano-run"):
        origin = read(trial / "worker-origin.json")
        assert origin["node"] != "local"
        trials[origin["job"]] = trial
    foreground = trials[proof["foreground_id"]]
    background = trials[proof["background_id"]]
    assert read(foreground / "result.json")["controller_hash"] == digest(
        foreground / "controller.json"
    )
    assert (
        digest(foreground / "motor-targets.f64") == result["actuator_audit"]["sha256"]
    )
    assert read(background / "worker-origin.json")["state"] == "cancelled"
    source = read(foreground / "source.json")
    assert (
        source["adapter"] == "aisle.sim.nexus_backend" and source["backend"] == "webgpu"
    )
    assert source["model_sha256"] == result["model_hash"]
    assert (
        read(foreground / "recording.json")["samples"][-1]["step"]
        <= result["physics_ticks"]
    )
    observations = [
        json.loads(l)
        for l in (background / "background-observations.jsonl")
        .read_bytes()
        .splitlines()
    ]
    snapshot = proof["snapshot"]
    assert any(
        o["tick"] == snapshot["tick"] and o["sha256"] == snapshot["sha256"]
        for o in observations
    )
    assert read(verified / "recovery.json")
    viewports = defaultdict(list)
    for line in (
        ((live_folder or folder) / "verified/journey-draws.jsonl")
        .read_bytes()
        .splitlines()
    ):
        draw = json.loads(line)
        if draw["source"].endswith("-fleet-watch"):
            viewports[draw["source"]].append(draw)
    # Each re-selection opens a new watch. Report only uninterrupted foreground ticks.
    windows = []
    for watch, draws in viewports.items():
        if len(draws) < 2:
            continue
        duration = (draws[-1]["elapsed_ns"] - draws[0]["elapsed_ns"]) / 1e9
        assert duration > 0
        ticks = [draw["tick"] for draw in draws]
        distinct = len(set(ticks))
        windows.append(
            dict(
                first_tick=ticks[0],
                last_tick=ticks[-1],
                distinct_draws=distinct,
                wall_s=duration,
                distinct_fps=(distinct - 1) / duration,
            )
        )
    (metric,) = (folder / "state/aisle").glob("*-Nano-prepare/provider-metrics.json")
    return dict(
        provider=read(metric),
        nexus=source["nexus"],
        backend=source["backend"],
        model_sha256=result["model_hash"],
        controller_sha256=result["controller_hash"],
        foreground=dict(
            frames=result["frames"],
            ticks=result["physics_ticks"],
            task_success=result["task_success"],
            supported_ticks=result["wheel_supported_ticks"],
            unsupported_wheel_ranges=result["unsupported_wheel_ranges"],
            max_joint_excursion_rad=result["max_joint_excursion_rad"],
            motor_sha256=result["actuator_audit"]["sha256"],
            placement=result["cubes"]["red"],
            archive=read(foreground / "worker-origin.json")["sha256"],
        ),
        background=dict(
            sampled_ticks=[o["tick"] for o in observations],
            fetched_tick=snapshot["tick"],
            fetched_sha256=snapshot["sha256"],
            archive=read(background / "worker-origin.json")["sha256"],
        ),
        selected_viewport_windows=windows,
        reopened_after_restart=True,
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("native_folder", type=Path)
    parser.add_argument(
        "--live-folder",
        type=Path,
        help="Original live run when recovering from a disconnected verifier",
    )
    args = parser.parse_args()
    print(json.dumps(summarize(args.native_folder, args.live_folder), indent=2))
