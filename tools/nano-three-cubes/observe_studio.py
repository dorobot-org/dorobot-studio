"""Save read-only Studio UI telemetry while a live simulation is running."""

import argparse
import json
import time
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--report", type=Path, required=True)
parser.add_argument("--run", type=Path, required=True)
args = parser.parse_args()
last = None
with (args.run / "studio-observations.jsonl").open("x") as output:
    while True:
        try:
            state = json.loads(args.report.read_text())["state"]
            live = state.get("live") or {}
            sequence = live.get("sequence")
            if sequence is not None and sequence != last:
                probe = state.get("robot_probe") or {}
                sample = dict(
                    wall_time=time.time(),
                    sim_time=state["time"],
                    live=live,
                    connection=state["connection"],
                    base=state["base_pose"],
                    joints=state["recorded_joints"],
                    scene_objects=state["scene_objects"],
                    scene_transforms=probe.get("scene_transforms"),
                )
                output.write(json.dumps(sample) + "\n")
                output.flush()
                last = sequence
            if live.get("finished") or (args.run / "verification.json").exists():
                (args.run / "studio-final.json").write_text(
                    json.dumps(state, indent=2) + "\n"
                )
                break
        except (FileNotFoundError, json.JSONDecodeError):
            pass
        time.sleep(0.1)
