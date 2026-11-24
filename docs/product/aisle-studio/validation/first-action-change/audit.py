"""Verify the measured Nano decision against either the published bundle or fresh trials."""

import argparse
import hashlib
from io import BytesIO
from itertools import zip_longest
import json
from pathlib import Path
import struct
from zipfile import ZipFile


def verify_pair(read):
    actions, motors, exposures = [], [], []
    for name, ticks, frames, placed, success in (
        ("0001-Nano-run", 17814, 890, 2, False),
        ("0002-Nano-run", 23596, 1179, 3, True),
    ):

        def raw(file):
            return read(f"{name}/{file}")

        result = json.loads(raw("result.json"))
        assert result["task_success"] == success
        assert sum(cube["passed"] for cube in result["cubes"].values()) == placed
        assert result["wheel_supported_ticks"] == result["physics_ticks"] == ticks
        assert result["max_joint_excursion_rad"] == 0
        samples = json.loads(raw("recording.json"))["samples"]
        assert (
            len(samples) == result["frames"] == frames and samples[889]["step"] == 17800
        )
        motor = raw("motor-targets.f64")
        joints = json.loads(raw("calibration.json"))["joint_names"]
        assert len(joints) == 17 and len(motor) == 8 * len(joints) * (1 + ticks)
        assert hashlib.sha256(motor).hexdigest() == result["actuator_audit"]["sha256"]
        motors.append(motor)
        actions.append([json.loads(row) for row in raw("actions.jsonl").splitlines()])
        if success:
            packet = raw("grasp-observations/017814.npz")
        else:
            receipt = result["last_pickup_observation"]
            packet = raw("last-pickup-observation.npz")
            assert hashlib.sha256(packet).hexdigest() == receipt["sha256"]
            assert receipt["stamp_ns"] == 178140000000
        # Compare the typed array members independently of ZIP framing or compression.
        with ZipFile(BytesIO(packet)) as seen:
            assert set(seen.namelist()) == {
                f"{key}.npy"
                for key in ("head", "left_wrist", "right_wrist", "encoders", "stamp_ns")
            }
            channels = {key: seen.read(key) for key in seen.namelist()}
            assert struct.unpack("<q", channels["stamp_ns.npy"][-8:])[0] == 178140000000
            exposures.append(channels)
    assert exposures[0] == exposures[1]
    assert motors[1].startswith(motors[0])
    index, pair = next(
        (i, p) for i, p in enumerate(zip_longest(*actions), 1) if p[0] != p[1]
    )
    assert index == 1148 and pair[0] is None and pair[1]["tick"] == 17814
    print(
        "Verified matched sensor inputs, 1,147 common actions, the complete motor prefix and both outcomes."
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--recordings",
        type=Path,
        help="Directory containing two freshly recorded Nano runs",
    )
    args = parser.parse_args()
    if args.recordings:
        verify_pair(lambda name: (args.recordings / name).read_bytes())
    else:
        root = Path(__file__).resolve().parent
        record = json.loads((root / "native.json").read_bytes())
        archive = root / "demo-workspace.zip"
        assert (
            hashlib.sha256(archive.read_bytes()).hexdigest()
            == record["workspace_sha256"]
        )
        with ZipFile(archive) as bundle:
            verify_pair(lambda name: bundle.read("aisle/" + name))
