"""dora node (offline replay): sends ONE goal as soon as loc_trust/status reports status == 0 (trusted), then idles
until Stop. Replaces navcmd + goal_resume in the replay (those bind UDP 9998 / need the gate).
Input: trust. Output: goal (UInt8 JSON {"id","source","x","y","yaw"}, map frame, yaw in rad).
Env GOAL="x y yaw_deg" (default "1.5 1.5 0" = map point Q of garage_20261121), GOAL_ID (default REPLAY1)."""
import json
import math
import os

import numpy as np
import pyarrow as pa


def parse_goal(spec, goal_id):
    x, y, yaw_deg = (float(v) for v in spec.split())
    return {"id": goal_id, "source": "user", "x": x, "y": y, "yaw": math.radians(yaw_deg)}


def is_trusted(raw):
    try:
        return json.loads(bytes(raw).decode()).get("status") == 0
    except (ValueError, AttributeError):
        return False


def main():
    from dora import Node

    goal = parse_goal(os.environ.get("GOAL", "1.5 1.5 0"), os.environ.get("GOAL_ID", "REPLAY1"))
    node = Node()
    sent = False
    for ev in node:
        if ev["type"] == "INPUT" and ev["id"] == "trust" and not sent:
            if is_trusted(ev["value"].to_numpy()):
                node.send_output("goal", pa.array(np.frombuffer(json.dumps(goal).encode(), np.uint8)))
                sent = True
                print(f"goal_once: trusted, sent goal {goal}", flush=True)
        elif ev["type"] == "STOP":
            break


if __name__ == "__main__":
    main()
