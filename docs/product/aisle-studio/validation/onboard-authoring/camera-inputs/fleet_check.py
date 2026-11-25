"""Submit one observed Nano transfer to a local fleet worker and verify its archive."""

import argparse
import base64
import json
from pathlib import Path
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parents[6] / "tools/aisle_workbench"))
from fleet_bridge import import_trial, nodes, reconcile, record, submit
from fleet_client import send, submission
from fleet_worker import ACTIVE, initialize
from onboard_scene import verify_observation_files
from worker import digest


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("controller", type=Path)
    cli.add_argument("--out", required=True, type=Path)
    args = cli.parse_args()
    root = args.out.resolve()
    root.mkdir(parents=True, exist_ok=False)
    node = nodes(root)["local"]
    initialize(Path(node["spool"]), 1)
    assert send(node, {"op": "hello"})["onboard_author_images"]
    plan = json.loads(args.controller.read_bytes())
    binding = plan["scene_observation"]
    images = binding["author_input"]["images"]
    assert len(images) == 3 and plan["kind"] == "table-transfer"
    request = submission(args.controller, seed=binding["layout"])
    broken = dict(request, files=dict(request["files"]))
    broken["files"][images[0]["file"]] = base64.b64encode(b"changed").decode()
    try:
        send(node, broken)
    except ValueError as exc:
        assert "captured observation file has changed" in str(exc)
    else:
        raise AssertionError("Worker accepted a changed exposure")
    info = submit(root, {"local": node}, "local", request)
    row = record(root, request["id"])
    deadline = time.monotonic() + 360
    while info["state"] in ACTIVE and time.monotonic() < deadline:
        time.sleep(2)
        info = reconcile(root, row, node)
    assert info["state"] == "completed", info.get("message")
    trial = import_trial(root, node, row, info)
    assert import_trial(root, node, row, info) == trial
    files = verify_observation_files(trial, binding)
    for name in files:
        assert (args.controller.parent / name).read_bytes() == (
            trial / name
        ).read_bytes()
    result = json.loads((trial / "result.json").read_bytes())
    assert result["task_success"] and result["controller_hash"] == digest(
        args.controller
    )
    assert result["physics_ticks"] == result["wheel_supported_ticks"]
    summary = dict(
        controller_sha256=result["controller_hash"],
        files=files,
        archive_sha256=json.loads((trial / "worker-origin.json").read_bytes())[
            "sha256"
        ],
        target=result["requested"],
        physics_ticks=result["physics_ticks"],
        wheel_supported_ticks=result["wheel_supported_ticks"],
        motor_journal_sha256=digest(trial / "motor-targets.f64"),
        altered_image_rejected=True,
        reopened=True,
    )
    (root / "verification.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps(summary))


if __name__ == "__main__":
    main()
