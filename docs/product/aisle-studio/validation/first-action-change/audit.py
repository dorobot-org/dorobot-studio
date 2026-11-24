"""Verify the archived action and actuator prefixes using the standard library."""

from itertools import zip_longest
from pathlib import Path
import hashlib
import json
from zipfile import ZipFile

root = Path(__file__).resolve().parent
record = json.loads((root / "native.json").read_bytes())
archive = root / "demo-workspace.zip"
assert hashlib.sha256(archive.read_bytes()).hexdigest() == record["workspace_sha256"]
with ZipFile(archive) as z:
    actions, motors = [], []
    for trial in ("0001-Nano-run", "0002-Nano-run"):
        prefix = f"aisle/{trial}/"
        result = json.loads(z.read(prefix + "result.json"))
        motor = z.read(prefix + "motor-targets.f64")
        joints = json.loads(z.read(prefix + "calibration.json"))["joint_names"]
        assert len(motor) == 8 * len(joints) * (1 + result["physics_ticks"])
        assert hashlib.sha256(motor).hexdigest() == result["actuator_audit"]["sha256"]
        motors.append(motor)
        actions.append(
            [json.loads(row) for row in z.read(prefix + "actions.jsonl").splitlines()]
        )
    assert motors[1].startswith(motors[0])
    index, pair = next(
        (i, p) for i, p in enumerate(zip_longest(*actions), 1) if p[0] != p[1]
    )
    assert index == 1148 and pair[0] is None and pair[1]["tick"] == 17814
    assert record["offscreen"]["journey.json"]["policy_observations"][
        "identical_inputs"
    ]
print(
    "1,147 matching actions and 17,815 matching 17-joint target rows before the earlier trace ended."
)
