"""Recompute the Nano wrist interruption, motor hold, encoder response and placement outcomes."""

import argparse
import hashlib
import io
import json
from pathlib import Path
import sys
import subprocess
import tempfile
from zipfile import ZipFile

import numpy as np

from archive import FILES, MODES

ROOT = Path(__file__).resolve().parents[5]
sys.path.insert(0, str(ROOT / "tools/aisle_workbench"))
from wrist_timing_check import audit


def checked_folder(folder, requested):
    report = audit(folder)

    def load(name):
        return json.loads((folder / name).read_bytes())

    result, record = load("result.json"), load("recording.json")
    assert result["requested"] == requested and set(result["cubes"]) == set(requested)
    assert (
        result["max_joint_excursion_rad"] <= 0.005
        and not result["robot_furniture_contacts"]
    )
    settled = []
    for name in requested:
        c = result["cubes"][name]
        passed = (
            c["opposing_finger_airborne_ticks"] >= 20
            and c["table_contact_tail_ticks"] == 100
            and c["detached"]
            and c["min_finger_clearance_tail_m"] > 0.001
            and c["nearest_table_edge_m"] > 0.035
            and result["table_top_m"] + 0.015
            <= c["final_position_m"][2]
            <= result["table_top_m"] + 0.037
            and c["settle_motion_m"] < 0.003
        )
        assert passed == c["passed"]
        if passed:
            settled.append(name)
    assert (settled == requested and result["error"] is None) == report["task_success"]
    source = load("source.json")
    assert source["wrist_rgb_timing"] == report["fixture"]
    assert source["controller_sha256"] == report["controller"]
    assert len(source["support_code_hashes"]["nano_wrist_timing.py"]) == 64
    report["policy_sha256"] = source["policy_sha256"]
    report["settled"] = settled
    if report["first_hold_tick"] is not None:
        start = report["first_hold_tick"]
        end = start + report["held_ticks"]
        joints = [i for i, j in enumerate(record["joints"]) if j.startswith("left_j")]
        positions = np.frombuffer(
            (folder / "motor-targets.f64").read_bytes(), dtype="<f8"
        ).reshape(-1, len(record["joints"]))
        samples = [r for r in record["samples"] if start <= r["step"] <= end]
        assert len(samples) >= 2
        for sample in samples:
            assert np.array_equal(sample["action"], positions[sample["step"]])
        measured = np.asarray([r["state"] for r in samples])[:, joints]
        report["left_arm_encoder_span_rad"] = float(np.ptp(measured, axis=0).max())
        report["last_hold_encoder_error_rad"] = float(
            abs(measured[-1] - positions[start, joints]).max()
        )
    return report


def exported(z, mode, temp, decode):
    prefix = f"exports/{mode}/"
    info = json.loads(z.read(prefix + "meta/info.json"))
    meta = json.loads(z.read(prefix + "meta/aisle.json"))
    archive_trace = z.read(f"revised/{mode}/wrist-delivery.jsonl")
    assert z.read(prefix + meta["wrist_delivery_log"]) == archive_trace
    assert hashlib.sha256(archive_trace).hexdigest() == meta["wrist_delivery_sha256"]
    record = json.loads(z.read(f"revised/{mode}/recording.json"))
    assert info["total_frames"] == len(record["samples"])
    assert meta["wrist_timing"]["fixture"] == mode
    assert "annotation.wrist_rgb_hold" in info["features"]
    trace = [json.loads(row) for row in archive_trace.splitlines()]
    index = 0
    held = False
    expected = []
    for sample in record["samples"]:
        while index < len(trace) and trace[index]["tick"] <= sample["step"]:
            held = bool(trace[index]["fault"])
            index += 1
        expected.append(int(held))
    if decode:
        import pyarrow.parquet as pq

        data = z.read(prefix + "data/chunk-000/episode_000000.parquet")
        table = pq.read_table(io.BytesIO(data))
        assert table["annotation.wrist_rgb_hold"].to_pylist() == expected
        for field, name in [("observation.state", "state"), ("action", "action")]:
            measured = np.asarray(
                [r[name] for r in record["samples"]], dtype=np.float32
            )
            assert np.array_equal(measured, table[field].to_pylist())
        for camera in record["cameras"]:
            video = Path(temp) / f"{mode}-{camera}.mp4"
            video.write_bytes(
                z.read(
                    prefix
                    + f"videos/chunk-000/observation.images.{camera}/episode_000000.mp4"
                )
            )
            count = subprocess.check_output(
                [
                    "ffprobe",
                    "-v",
                    "error",
                    "-select_streams",
                    "v:0",
                    "-count_frames",
                    "-show_entries",
                    "stream=nb_read_frames",
                    "-of",
                    "csv=p=0",
                    str(video),
                ]
            )
            assert int(count) == len(expected)
    return dict(
        frames=len(expected),
        held_steps=[
            row["step"]
            for row, flag in zip(record["samples"], expected, strict=True)
            if flag
        ],
        delivery_log_sha256=meta["wrist_delivery_sha256"],
    )


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("archive", type=Path)
    cli.add_argument("--output", type=Path)
    cli.add_argument(
        "--git-sources",
        action="store_true",
        help="Check the native source manifests against the measured Git commits",
    )
    cli.add_argument(
        "--datasets",
        action="store_true",
        help="Decode every camera video and check the Parquet observations and hold annotations",
    )
    a = cli.parse_args()
    result = {}
    checked_sources = {}
    with ZipFile(a.archive) as z, tempfile.TemporaryDirectory() as temp:
        assert len(z.namelist()) == len(set(z.namelist()))
        for tag in (
            "codex",
            "deepseek",
            "three",
            "revised",
            "codex-three",
            "deepseek-three",
        ):
            trials = []
            for mode in MODES:
                prefix = tag + "/" + mode + "/"
                folder = Path(temp) / tag / mode
                folder.mkdir(parents=True)
                for name in FILES:
                    (folder / name).write_bytes(z.read(prefix + name))
                exposures = folder / "wrist-exposures"
                exposures.mkdir()
                for info in z.infolist():
                    if info.filename.startswith(prefix + "wrist-exposures/"):
                        name = Path(info.filename).name
                        assert name.removesuffix(".png").isdigit()
                        (exposures / name).write_bytes(z.read(info))
                report = checked_folder(
                    folder,
                    (
                        ["red"]
                        if tag in ("codex", "deepseek")
                        else ["red", "green", "blue"]
                    ),
                )
                if tag in ("three", "revised") and mode != "disconnected":
                    sample = json.loads(z.read(prefix + "release-sample.json"))
                    record = json.loads((folder / "recording.json").read_bytes())
                    assert (
                        record["samples"][sample["index"]]["step"]
                        == sample["sample_tick"]
                    )
                    assert abs(sample["sample_tick"] - sample["decision_tick"]) <= 10
                    actions = [
                        json.loads(line)
                        for line in (folder / "actions.jsonl").read_bytes().splitlines()
                    ]
                    start = next(
                        i
                        for i, row in enumerate(actions)
                        if row.get("name") == "place" and row["color"] == "blue"
                    )
                    assert (
                        next(
                            row["tick"]
                            for row in actions[start + 1 :]
                            if row["op"] == "sense"
                        )
                        == sample["decision_tick"]
                    )
                    release = next(
                        row for row in actions[start + 1 :] if "release_xy_m" in row
                    )
                    report["blue_release"] = dict(**sample, **release)
                assert report["fixture"] == mode
                trials.append(report)
            assert len({r["controller"] for r in trials}) == 1
            result[tag] = dict(trials=trials)
            if tag not in ("three", "revised"):
                manifest = json.loads(z.read(tag + "/mode-manifest.json"))
                assert manifest["backend"] == (
                    "headless" if tag.startswith("codex") else "offscreen"
                )
                assert manifest["provider"] == (
                    "codex" if tag.startswith("codex") else "octos-deepseek"
                )
                actor = json.loads(z.read(tag + "/author/provider-metrics.json"))
                prepared = z.read(tag + "/author/controller.json")
                assert (
                    actor["provider"] == manifest["provider"]
                    and actor["wall_seconds"] > 0
                )
                assert actor["model"] == json.loads(prepared)["policy_model"]
                assert hashlib.sha256(prepared).hexdigest() == trials[0]["controller"]
                result[tag]["author"] = actor
                journey = json.loads(z.read(tag + "/journey.json"))[-1]
                for native, checked in zip(journey["trials"], trials, strict=True):
                    assert all(checked[k] == v for k, v in native.items())
                recovered = json.loads(z.read(tag + "/recovery.json"))
                assert any(
                    r["check"] == "Wrist timeout recovered after restart"
                    for r in recovered
                )
                commit = manifest["studio_source_commit"]
                sources = manifest["source_files_sha256"]
                if commit in checked_sources:
                    assert checked_sources[commit] == sources
                elif a.git_sources:
                    for name, expected in sources.items():
                        raw = subprocess.check_output(
                            ["git", "cat-file", "blob", f"{commit}:{name}"], cwd=ROOT
                        )
                        assert hashlib.sha256(raw).hexdigest() == expected, (
                            commit,
                            name,
                        )
                checked_sources[commit] = sources
                for mode in MODES:
                    source = json.loads(z.read(f"{tag}/{mode}/source.json"))
                    hashes = manifest["source_files_sha256"]
                    assert (
                        hashes["tools/aisle-validation/nano_rgb_transfer_policy.py"]
                        == source["policy_sha256"]
                    )
                    for name, digest in source["support_code_hashes"].items():
                        prefix = (
                            "tools/nano-three-cubes/"
                            if name == "model.py"
                            else "tools/aisle-validation/"
                        )
                        assert hashes[prefix + name] == digest
                result[tag]["source_commit"] = manifest["studio_source_commit"]
                result[tag]["binary_sha256"] = manifest["native_binary_sha256"]
        previous_archive = (
            Path(__file__).resolve().parent.parent / "wall-clock-rgb/measurements.zip"
        )
        with ZipFile(previous_archive) as previous:
            old_motor = previous.read("10-half-codex/foreground/motor-targets.f64")
            assert (
                old_motor
                == z.read("codex/normal/motor-targets.f64")
                == z.read("deepseek/normal/motor-targets.f64")
            )
        original = result["three"]["trials"][1]
        revised = result["revised"]["trials"][1]
        assert original["controller"] == revised["controller"]
        assert original["settled"] == ["red", "green"] and not original["task_success"]
        assert (
            revised["settled"] == ["red", "green", "blue"] and revised["task_success"]
        )
        assert (
            original["blue_release"]["observed_occupants"] == 0
            and revised["blue_release"]["observed_occupants"] >= 2
        )
        for group in ("codex-three", "deepseek-three"):
            for plain, native in zip(
                result["revised"]["trials"], result[group]["trials"], strict=True
            ):
                assert (
                    plain["task_success"] == native["task_success"]
                    and plain["motor_sha256"] == native["motor_sha256"]
                )
        receipt = json.loads(z.read("fleet/receipt.json"))
        blob = z.read("fleet/wrist-timeout.zip")
        assert (
            receipt["older_rejected_before_submission"]
            and receipt["state"] == "completed"
        )
        assert hashlib.sha256(blob).hexdigest() == receipt["archive_sha256"]
        with ZipFile(io.BytesIO(blob)) as trial:
            timeout = json.loads(trial.read("result.json"))
            assert timeout["wrist_timing"]["timed_out"] and not timeout["task_success"]
            assert (
                hashlib.sha256(trial.read("motor-targets.f64")).hexdigest()
                == receipt["motor_sha256"]
            )
        approach = json.loads(z.read("fleet/legacy-approach.json"))
        legacy = approach["result"]
        assert approach["state"] == "completed" and approach["worker"] is None
        assert legacy["kind"] == "visual-approach" and legacy["task_success"]
        assert legacy["wheel_supported_ticks"] == legacy["physics_ticks"]
        assert legacy["controller_hash"] == approach["controller_sha256"]
        assert legacy["sensor_timing"]["fixture"] == "intermittent"
        result["legacy_approach"] = dict(
            physics_ticks=legacy["physics_ticks"],
            final_range_m=legacy["final_distance_m"],
            controller=legacy["controller_hash"],
        )
        result["fleet"] = receipt
        result["exports"] = {
            mode: exported(z, mode, temp, a.datasets)
            for mode in ("intermittent", "disconnected")
        }
    data = json.dumps(result, indent=2) + "\n"
    if a.output:
        a.output.write_text(data)
    else:
        print(data, end="")


if __name__ == "__main__":
    main()
