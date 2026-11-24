"""Measure raw onboard light and a repeatable floor slide in three saved Nano rooms."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys

import numpy as np
from PIL import Image


def read_room(path):
    raw = path.read_bytes()
    return json.loads(raw), hashlib.sha256(raw).hexdigest()


def measure(path, seed, out):
    repo = Path(__file__).resolve().parents[5]
    sys.path.insert(0, str(repo / "tools/aisle-validation"))
    sys.path.insert(0, str(repo / "tools/aisle_workbench"))
    from run_rgb_transfer import LivingSimulation, MODEL, POSTURE, scene_recipe
    from room_revision import load_revision

    _, room_hash = read_room(path)
    recipe = scene_recipe(seed)
    revision = load_revision(
        path, room_hash, recipe, hashlib.sha256(MODEL.read_bytes()).hexdigest()
    )
    sim = LivingSimulation(
        MODEL,
        specs=recipe + revision["obstacles"],
        resolution=(480, 360),
        posture=POSTURE,
        room_settings=revision["settings"],
    )
    frame = np.asarray(
        sim.rig["head"].render(rgb=True, depth=False, segmentation=False)[0]
    )
    out.mkdir(parents=True, exist_ok=True)
    Image.fromarray(frame).save(out / "head.png")
    cube = next(body for body in sim.bodies if body.name == "red")
    start = cube.get_pos().copy()
    # Evaluator-only material probe. The task controller does not participate.
    sim.scene.set_body_velocity(0, cube.handles[0], [0.8, 0, 0])
    sim.advance([0, 0, 0], steps=50)
    end = cube.get_pos()
    measurements = dict(
        room_sha256=room_hash,
        seed=seed,
        rgb_sha256=hashlib.sha256(frame.tobytes()).hexdigest(),
        ambient_rgb=sim.scene.ambient,
        floor_friction=revision["settings"]["floor_friction"],
        slide_x_mm=float((end[0] - start[0]) * 1000),
    )
    (out / "measurement.json").write_text(json.dumps(measurements, indent=2) + "\n")


def compare(paths, seed, out):
    rooms = [read_room(path) for path in paths]
    for (earlier, sha), (later, _), setting in zip(
        rooms, rooms[1:], ("ambient_rgb", "floor_friction")
    ):
        if later["parent_sha256"] != sha:
            raise ValueError("Rooms do not share the selected lineage")
        if (
            later["settings"][setting] == earlier["settings"][setting]
            or later["settings"]
            != {**earlier["settings"], setting: later["settings"][setting]}
            or later["obstacles"] != earlier["obstacles"]
            or later["reference_sha256"] != earlier["reference_sha256"]
        ):
            raise ValueError(f"Expected only {setting} to change")

    rows, images = [], []
    for path, name in zip(paths, ("baseline", "light", "floor"), strict=True):
        target = out / name
        # Each run starts with a fresh Nexus renderer and physics state.
        subprocess.run(
            [
                sys.executable,
                __file__,
                "--one",
                str(path),
                "--seed",
                str(seed),
                "--out",
                str(target),
            ],
            check=True,
        )
        rows.append(json.loads((target / "measurement.json").read_bytes()))
        images.append(np.asarray(Image.open(target / "head.png"), dtype=float))
    rgb_mae = float(np.mean(np.abs(images[0] - images[1])))
    light_slide_delta = rows[1]["slide_x_mm"] - rows[0]["slide_x_mm"]
    floor_slide_delta = rows[2]["slide_x_mm"] - rows[1]["slide_x_mm"]
    result = dict(
        samples=rows,
        light_rgb_mean_absolute_delta_8bit=rgb_mae,
        light_slide_delta_mm=light_slide_delta,
        floor_slide_delta_mm=floor_slide_delta,
        floor_rgb_mean_absolute_delta_8bit=float(
            np.mean(np.abs(images[2] - images[1]))
        ),
    )
    (out / "comparison.json").write_text(json.dumps(result, indent=2) + "\n")
    if rgb_mae <= 1 or abs(light_slide_delta) >= 0.1 or abs(floor_slide_delta) <= 0.1:
        raise ValueError(f"No isolated visual or material response: {result}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    inputs = parser.add_mutually_exclusive_group(required=True)
    inputs.add_argument(
        "--rooms", nargs=3, type=Path, metavar=("BASE", "LIGHT", "FLOOR")
    )
    inputs.add_argument("--one", type=Path, help=argparse.SUPPRESS)
    parser.add_argument("--seed", type=int, default=0)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    if args.one:
        measure(args.one, args.seed, args.out)
    else:
        compare(args.rooms, args.seed, args.out)
