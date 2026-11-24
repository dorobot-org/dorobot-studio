"""Recompute the H100 RGB and joint-state comparisons from the retained samples."""

import argparse
import gzip
import hashlib
import io
import json
from pathlib import Path
from statistics import mean, median
from zipfile import ZipFile

import numpy as np


def compare(directory):
    with ZipFile(directory.parent / "eight-h100/lossless-rgb/measurements.zip") as old:
        motor_hash = hashlib.sha256(
            old.read("codex/gpu1/motor-targets.f64")
        ).hexdigest()
    rows = []
    with ZipFile(directory / "render-samples.zip") as archive:

        def array(name):
            return np.load(io.BytesIO(archive.read(name)), allow_pickle=False)

        with array("reference/rgb.npz") as reference_file:
            reference = dict(reference_file)
        ref_joints = array("reference/encoder.npy")
        assert len(reference) == 24 and ref_joints.shape[0] == 8
        order_study = json.loads(
            gzip.decompress((directory / "stage-order.json.gz").read_bytes())
        )
        assert len(order_study) == 3
        for run in order_study.values():
            assert run["motor_sha256"] == motor_hash
            assert [row[0] for row in run["rows_ns"]] == list(range(65, 321))
            assert (
                mean(a + b for _, a, b in run["rows_ns"]) / 1e6 == run["mean_total_ms"]
            )
            for name, digest in run["sample_sha256"].items():
                assert (
                    hashlib.sha256(archive.read("reference/" + name)).hexdigest()
                    == digest
                )
        for mode in ("reference", "repeat", "floor", "2048", "1024", "nexus-0.3.0"):
            timing = json.loads(archive.read(f"{mode}/timing.json"))
            ticks, simulation, camera = zip(*timing["rows_ns"])
            assert list(ticks) == list(range(65, 321))
            assert timing["motor_sha256"] == motor_hash
            total = mean(a + b for a, b in zip(simulation, camera)) / 1e6
            rgb_ms = median(camera) / 1e6
            assert (
                total == timing["mean_total_ms"] and rgb_ms == timing["median_rgb_ms"]
            )
            joints = array(f"{mode}/encoder.npy")
            assert joints.shape == ref_joints.shape
            identical = changed = pixels = 0
            with array(f"{mode}/rgb.npz") as sample:
                assert set(reference) == set(sample.files)
                for key, original in reference.items():
                    image = sample[key]
                    assert original.shape == image.shape == (360, 480, 3)
                    assert original.dtype == image.dtype == np.uint8
                    count = np.any(image != original, axis=-1).sum().item()
                    changed += count
                    pixels += image.shape[0] * image.shape[1]
                    identical += int(count == 0)
            rows.append(
                dict(
                    setting=mode,
                    nexus=timing["nexus"],
                    mean_step_ms=round(total, 3),
                    median_head_ms=round(rgb_ms, 3),
                    identical_images=identical,
                    changed_pixels=changed,
                    total_pixels=pixels,
                    changed_percent=round(100 * changed / pixels, 5),
                    encoder_exact=bool(np.array_equal(ref_joints, joints)),
                    max_qpos_difference=float(np.max(np.abs(ref_joints - joints))),
                )
            )
    return rows


def verify_native(directory):
    runs = json.loads((directory / "native-draws.json").read_bytes())
    assert set(runs) == {"mac-native", "h100-native"}
    for run in runs.values():
        draws = run["draw_tick_and_nanoseconds"]
        ticks, times = zip(*draws)
        assert all(a < b for a, b in zip(ticks, ticks[1:]))
        assert all(a < b for a, b in zip(times, times[1:]))
        assert run["label_draw_ns"] == times[-1]
        rate = (len(draws) - 1) * 1e9 / (times[-1] - times[0])
        assert run["label"] == f"Foreground worker camera · {rate:.1f} FPS"
        assert (
            run["completed"]
            and run["supported_ticks"] > 0
            and run["frames_per_camera"] > 0
        )
        assert any("recover" in step.lower() for step in run["recovery"])
    provenance = json.loads((directory / "provenance.json").read_bytes())
    assert all(
        run["binary_sha256"] == provenance["studio_sha256"] for run in runs.values()
    )
    samples = json.loads((directory / "worker-stall.json").read_bytes())
    assert {r["job"] for r in samples} == {runs["h100-native"]["job"]}
    assert all(r["state"] == "running" for r in samples)
    stalled = next(
        i for i, r in enumerate(samples) if r["label"].endswith("Stream stalled")
    )
    assert samples[stalled]["seconds"] >= 2
    assert any(
        r["label"].endswith("Waiting for frames") for r in samples[stalled + 1 :]
    )
    assert (
        samples[-1]["label"].endswith(" FPS")
        and samples[-1]["step"] > samples[0]["step"]
    )

    local = json.loads((directory / "g1-local.json").read_bytes())
    assert local["native_binary_sha256"] == runs["mac-native"]["binary_sha256"]
    assert not local["trace_file_exists"] and local["task_success"]
    assert (
        local["disturbance"]["passed"]
        and local["both_feet_loaded_steps"] == local["steps"] == 2000
    )
    assert any(r["busy"] and r["label"].endswith(" FPS") for r in local["labels"])
    assert local["labels"][-1]["label"] == "External operator camera"


def verify_mixed_hosts(directory):
    report = json.loads((directory / "mixed-hosts.json").read_bytes())
    foreground, observer = report["foreground"], report["observer"]
    assert (
        foreground["worker_backend"] == "webgpu"
        and observer["worker_backend"] == "metal"
    )
    assert foreground["job"] != observer["job"] and report["physical_hosts"] == 2
    with ZipFile(directory / "mixed-host-trace.zip") as trace:
        start, end = json.loads(trace.read("overlap-endpoints.json"))
        draws = json.loads(trace.read("foreground-draws.json"))
        received = json.loads(trace.read("mac-received.json"))
        assert (
            start[2] == end[2] == "running" and start[3] == end[3] == foreground["job"]
        )
        assert (start[5], end[5]) == (draws[0][1], draws[-1][1])
        assert round((end[0] - start[0]) / 1e9, 3) == report["shared_interval_s"]
        assert all(start[0] <= received_at <= end[0] for _, received_at, _ in received)
        for rows, published, rate, count in (
            (draws, foreground, "fps", "draws"),
            (received, observer, "wall_clock_hz", "received"),
        ):
            assert all(a[0] < b[0] and a[1] < b[1] for a, b in zip(rows, rows[1:]))
            assert len(rows) == published[count]
            assert (
                round((len(rows) - 1) * 1e9 / (rows[-1][1] - rows[0][1]), 3)
                == published[rate]
            )
        captured = [
            json.loads(line)
            for line in trace.read("mac/background-observations.jsonl").splitlines()
        ]
        hashes = {r["tick"]: r["sha256"] for r in captured}
        assert all(hashes[tick] == digest for tick, _, digest in received)
        verdict = json.loads(trace.read("mac/result.json"))
        assert verdict["task_success"] and verdict["recording"] is False
        assert (
            hashlib.sha256(trace.read("mac/motor-targets.f64")).hexdigest()
            == verdict["actuator_audit"]["sha256"]
        )
        runs = json.loads((directory / "native-draws.json").read_bytes())
        assert (
            runs["h100-native"]["job"] == foreground["job"]
            and runs["h100-native"]["completed"]
        )


def verify_mac_foreground(directory):
    report = json.loads((directory / "mac-foreground.json").read_bytes())
    assert report["foreground"]["worker_backend"] == "metal"
    assert report["observer"]["worker_backend"] == "webgpu"
    bins = [{"draws": 0, "received": 0} for _ in range(30)]
    with ZipFile(directory / "mac-foreground-trace.zip") as trace:
        draws = json.loads(trace.read("draws.json"))
        received = json.loads(trace.read("receiver.json"))
        assert report["window_start_ns"] == received[0][1]
        assert all(a[2] < b[2] and a[1] < b[1] for a, b in zip(draws, draws[1:]))
        for rows, column, field in ((draws, 0, "draws"), (received, 1, "received")):
            for row in rows:
                idx = (row[column] - report["window_start_ns"]) // 1_000_000_000
                assert 0 <= idx < len(bins)
                bins[idx][field] += 1
        assert bins == report["per_second"]
        assert (
            len(draws) / 30 == report["draw_fps"]
            and len(received) / 30 == report["received_hz"]
        )
        assert (
            sum(b["draws"] >= 25 and b["received"] >= 1 for b in bins)
            == report["passed_seconds"]
        )
        captures = [
            json.loads(l)
            for l in trace.read("h100/background-observations.jsonl").splitlines()
        ]
        hashes = {r["tick"]: r["sha256"] for r in captures}
        assert all(hashes[tick] == digest for tick, _, digest in received)
        observer = json.loads(trace.read("h100/result.json"))
        assert observer["task_success"] and observer["recording"] is False
        assert (
            hashlib.sha256(trace.read("h100/motor-targets.f64")).hexdigest()
            == observer["actuator_audit"]["sha256"]
        )
        assert json.loads(trace.read("native-result.json"))["task_success"]


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check", action="store_true", help="Compare with the published table"
    )
    args = parser.parse_args()
    directory = Path(__file__).resolve().parent
    rows = compare(directory)
    if args.check:
        verify_native(directory)
        verify_mixed_hosts(directory)
        verify_mac_foreground(directory)
        assert rows == json.loads((directory / "render-comparison.json").read_bytes())
    print(json.dumps(rows, indent=2))
