"""Compare Studio's displayed live samples with independent physics evidence."""

import argparse
import json
from pathlib import Path

import numpy as np


def verify(root):
    ui = [
        json.loads(line)
        for line in (root / "studio-observations.jsonl").read_text().splitlines()
    ]
    physics = {
        round(r["time_s"], 2): r
        for r in json.loads((root / "trajectory.json").read_text())
    }
    matched, joint_error, cube_error, base_error = 0, 0.0, 0.0, 0.0
    for sample in ui:
        observed = physics.get(round(sample["sim_time"], 2))
        if observed is None:
            continue
        matched += 1
        joint_error = max(
            joint_error,
            float(np.max(np.abs(np.array(sample["joints"]) - observed["q"]))),
        )
        base_error = max(
            base_error,
            float(
                np.max(
                    np.abs(
                        np.array(sample["base"]["position"][:2]) - observed["base"][:2]
                    )
                )
            ),
        )
        for name, transform in zip(
            ("red", "green", "blue"), sample["scene_transforms"][-3:], strict=True
        ):
            cube_error = max(
                cube_error,
                float(
                    np.max(np.abs(np.array(transform[12:15]) - observed["cubes"][name]))
                ),
            )
    checks = dict(
        received_many_live_samples=sum(s["connection"]["phase"] == "Live" for s in ui)
        > 100,
        source_clock_advanced=ui[-1]["sim_time"] - ui[0]["sim_time"] > 60,
        all_three_cameras=all(
            set(s["live"]["cameras"]) == {"head", "left_wrist", "right_wrist"}
            for s in ui
        ),
        complete_scene=all(
            s["scene_objects"] == 16 and len(s["scene_transforms"]) == 16 for s in ui
        ),
        source_samples_matched=matched >= len(ui) - 1 and matched > 100,
        displayed_joints_match=joint_error < 1e-5,
        displayed_cubes_match=cube_error < 1e-5,
        displayed_base_matches=base_error < 1e-5,
        stream_completed=bool(
            json.loads((root / "studio-final.json").read_text())["live"]["finished"]
        ),
    )
    result = dict(
        ok=all(checks.values()),
        checks=checks,
        ui_samples=len(ui),
        matched_samples=matched,
        max_joint_error_rad=joint_error,
        max_cube_error_m=cube_error,
        max_base_error_m=base_error,
        observed_from_sim_s=ui[0]["sim_time"],
        observed_to_sim_s=ui[-1]["sim_time"],
    )
    (root / "studio-verification.json").write_text(json.dumps(result, indent=2) + "\n")
    return result


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run", type=Path)
    report = verify(parser.parse_args().run)
    print(json.dumps(report, indent=2))
    raise SystemExit(0 if report["ok"] else 1)
