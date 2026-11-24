"""Recalculate the camera differences, native cadence and transfer outcome from the archive."""

import hashlib
import io
import json
from pathlib import Path
from statistics import mean, median
from zipfile import ZipFile

import numpy as np


def cadence(tick_times):
    ticks, times = zip(*tick_times)
    assert all(b > a for a, b in zip(ticks, ticks[1:]))
    assert all(b > a for a, b in zip(times, times[1:]))
    span = (times[-1] - times[0]) / 1e9
    counts = [0] * int(span)
    for stamp in times:
        second = (stamp - times[0]) // 1_000_000_000
        if second < len(counts):
            counts[second] += 1
    return dict(
        frames=len(times),
        seconds=span,
        mean_fps=(len(times) - 1) / span,
        full_seconds=len(counts),
        seconds_at_least_25=sum(n >= 25 for n in counts),
        min_fps=min(counts),
        median_fps=median(counts),
        max_fps=max(counts),
    )


def audit(path):
    with ZipFile(path) as archive:

        def data(name):
            return json.loads(archive.read(name))

        def array(name):
            return np.load(io.BytesIO(archive.read(name)), allow_pickle=False)

        renders = {}
        base = "matched-original"
        configs = array(base + "/encoder.npy")
        assert len(configs) == 8
        with array(base + "/rgb.npz") as reference:
            for name in (base, "matched-mesh", "matched-compact"):
                timing = data(name + "/timing.json")
                samples = timing["rows_ns"]
                assert len(samples) == 256 and [r[0] for r in samples] == list(
                    range(65, 321)
                )
                assert np.array_equal(configs, array(name + "/encoder.npy"))
                changed = error = pixels = 0
                with array(name + "/rgb.npz") as candidate:
                    assert (
                        set(candidate.files) == set(reference.files)
                        and len(candidate.files) == 24
                    )
                    for key in candidate.files:
                        original, target = reference[key], candidate[key]
                        assert original.shape == target.shape == (360, 480, 3)
                        diff = np.abs(original.astype(np.int16) - target)
                        changed += np.count_nonzero(np.any(diff, axis=-1))
                        error += int(diff.sum())
                        pixels += diff.shape[0] * diff.shape[1]
                renders[name] = dict(
                    backend=timing["backend"],
                    nexus=timing["nexus"],
                    journal=timing["motor_sha256"],
                    mean_step_render_ms=mean(a + b for _, a, b in samples) / 1e6,
                    median_head_ms=median(b for _, _, b in samples) / 1e6,
                    changed_pixel_pct=100 * changed / pixels,
                    mean_abs_channel_error=error / (3 * pixels),
                    equal_encoder_samples=len(configs),
                )
        assert len({r["journal"] for r in renders.values()}) == 1
        source, result = data("native/source.json"), data("native/result.json")
        assert (
            result["controller_hash"]
            == source["controller_sha256"]
            == hashlib.sha256(archive.read("native/controller.json")).hexdigest()
        )
        assert source["visual_profile"] == result["visual_profile"] == "compact-v1"
        assert (
            source["visual_bundle_sha256"]
            == data("native/export.json")["visual_bundle_sha256"]
        )
        assert (
            result["requested"] == ["red"]
            and result["task_success"]
            and result["cubes"]["red"]["passed"]
        )
        assert (
            result["wheel_supported_ticks"] == result["physics_ticks"]
            and not result["rejected_motor_writes"]
        )
        assert result["max_joint_excursion_rad"] == result["root_pose_writes"] == 0
        worker_ticks = data("native/preview-timing.json")["tick_time"]
        drawn = data("native/draws.json")
        assert set(t for t, _ in drawn) <= set(t for t, _ in worker_ticks)
        return dict(
            fixed_inputs=renders,
            worker=cadence(worker_ticks),
            studio=cadence(drawn),
            supported_ticks=result["wheel_supported_ticks"],
            settled_contact_ticks=result["cubes"]["red"]["table_contact_tail_ticks"],
            requested=result["requested"],
            controller_hash=result["controller_hash"],
        )


if __name__ == "__main__":
    print(json.dumps(audit(Path(__file__).with_name("measurements.zip")), indent=2))
