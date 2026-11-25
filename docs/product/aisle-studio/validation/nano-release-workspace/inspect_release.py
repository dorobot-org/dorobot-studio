"""Inspect the recorded RGB, encoders and release decisions in a Nano cohort archive."""

import argparse
import io
import json
from pathlib import Path
import sys
from zipfile import ZipFile

import numpy as np

ROOT = Path(__file__).resolve().parents[5]
sys.path.insert(0, str(ROOT / "tools/aisle-validation"))
from nano_rgb_table import occupied_table_points
from nano_rgb_transfer_policy import CAMERAS, Controller


def inspect(archive_path):
    layouts = []
    with ZipFile(archive_path) as archive:
        report = json.loads(archive.read("report.json"))
        for row in report["rows"]:
            folder = f"case-{row['index']:03}/"
            if not row.get("verified"):
                layouts.append({"seed": row["seed"], "verified": False})
                continue
            config = json.loads(archive.read(folder + "calibration.json"))
            config["model"] = str(ROOT / "data/adora_nano/adora_nano.urdf")
            controller = Controller(config)
            motors = np.frombuffer(
                archive.read(folder + "motor-targets.f64"), dtype="<f8"
            )
            motors = motors.reshape(-1, len(controller.names))
            actions = [
                json.loads(line)
                for line in archive.read(folder + "actions.jsonl").splitlines()
            ]
            observations = []
            for action in actions:
                if action["op"] != "release observation":
                    continue
                tick = action["tick"]
                with np.load(
                    io.BytesIO(
                        archive.read(folder + f"release-observations/{tick:06}.npz")
                    ),
                    allow_pickle=False,
                ) as packet:
                    assert set(packet.files) == {*CAMERAS, "encoders", "stamp_ns"}
                    assert packet["stamp_ns"] == action["stamp_ns"] == tick * 10_000_000
                    assert packet["encoders"].shape == (len(controller.names),)
                    for camera in CAMERAS:
                        w, h = config["cameras"][camera]["resolution"]
                        assert (
                            packet[camera].shape == (h, w, 3)
                            and packet[camera].dtype == np.uint8
                        )
                    edge = dict(
                        normal=np.array(action["normal"]),
                        along=np.array(action["along"]),
                        distance=action["distance_m"],
                        height=action["height_m"],
                        span=action["span_m"],
                    )
                    prior = controller.colors[
                        : controller.colors.index(action["color"])
                    ]
                    occupied = occupied_table_points(
                        controller.table_views(packet, CAMERAS),
                        edge,
                        prior,
                        include_clipped=True,
                    )
                # release_paths uses commanded arm angles. The camera transforms
                # above use measured encoders from the same exposure as the RGB.
                controller.targets = motors[tick].copy()
                choices = []
                for offset in (0.0, -0.08):
                    shift = np.array([0.0, offset])
                    predicted = edge | {
                        "distance": edge["distance"] - edge["normal"] @ shift,
                        "span": np.asarray(edge["span"]) - edge["along"] @ shift,
                    }
                    choice = controller.release_slot(
                        predicted, [point - shift for point in occupied]
                    )
                    clearance = None
                    if choice is not None and occupied:
                        clearance = min(
                            float(np.linalg.norm(choice[0] + shift - point))
                            for point in occupied
                        )
                    choices.append(
                        dict(
                            offset_m=offset,
                            reachable=choice is not None,
                            nearest_observed_cube_m=clearance,
                        )
                    )
                observations.append(
                    dict(
                        tick=tick,
                        color=action["color"],
                        occupants=len(occupied),
                        choices=choices,
                    )
                )
            turns = [
                dict(
                    tick=a["tick"],
                    color=a["color"],
                    yaw_rad=a["placement_yaw_goal_rad"],
                )
                for a in actions
                if "placement_yaw_goal_rad" in a
            ]
            layouts.append(
                dict(
                    seed=row["seed"],
                    passed=row["passed"],
                    error=row.get("error"),
                    release_exposures=observations,
                    measured_turns=turns,
                )
            )
    return layouts


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    print(json.dumps(inspect(parser.parse_args().archive), indent=2))
