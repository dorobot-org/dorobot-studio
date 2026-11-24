"""Recompute the H100 RGB and joint-state comparisons from the retained samples."""

import argparse
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


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check", action="store_true", help="Compare with the published table"
    )
    args = parser.parse_args()
    directory = Path(__file__).resolve().parent
    rows = compare(directory)
    if args.check:
        assert rows == json.loads((directory / "render-comparison.json").read_bytes())
    print(json.dumps(rows, indent=2))
