"""Project the recorded RGB pixels into Nano base coordinates using its URDF and encoders."""

import argparse
from collections import defaultdict
import io
import json
from pathlib import Path
import sys
from zipfile import ZipFile

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parents[5]
sys.path.insert(0, str(ROOT / "tools/aisle-validation"))
from nano_rgb_policy import CAMERAS
from nano_rgb_table import table_edge, occupied_table_points
from nano_rgb_transfer_policy import Controller


def project(z, prefix):
    def read(name):
        return json.loads(z.read(f"{prefix}/{name}"))

    config = read("calibration.json")
    config["model"] = str(ROOT / "data/adora_nano/adora_nano.urdf")
    config["colors"] = ["red", "green", "blue"]
    controller = Controller(config)
    decision = read("release-sample.json")
    record = read("recording.json")["samples"][decision["index"]]
    assert record["step"] == decision["sample_tick"]
    packet = dict(encoders=np.asarray(record["state"]))
    for camera in CAMERAS:
        with Image.open(io.BytesIO(z.read(f"{prefix}/release-{camera}.png"))) as img:
            packet[camera] = np.asarray(img.convert("RGB"))
    edge = table_edge(controller.table_views(packet))
    assert edge is not None
    releases = [
        json.loads(line) for line in z.read(f"{prefix}/actions.jsonl").splitlines()
    ]
    chosen = next(
        np.asarray(r["release_xy_m"])
        for r in releases
        if "release_xy_m" in r and r["tick"] >= decision["decision_tick"]
    )
    distances = defaultdict(dict)
    for camera, view in zip(
        CAMERAS, controller.table_views(packet, CAMERAS), strict=True
    ):
        for color in ["red", "green"]:
            points = occupied_table_points([view], edge, [color], include_clipped=True)
            distances[camera][color] = [
                round(float(np.linalg.norm(chosen - p)) * 1000, 1) for p in points
            ]
    return dict(**decision, distance_to_blue_release_mm=distances)


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("archive", type=Path)
    args = cli.parse_args()
    with ZipFile(args.archive) as z:
        result = {
            tag: project(z, f"{tag}/intermittent") for tag in ("three", "revised")
        }
    before = result["three"]["distance_to_blue_release_mm"]
    after = result["revised"]["distance_to_blue_release_mm"]
    assert not before["head"]["green"] and not before["right_wrist"]["green"]
    assert min(before["left_wrist"]["green"]) < 78 <= min(after["left_wrist"]["green"])
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
