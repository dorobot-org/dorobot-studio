"""Read the retained Nano policy input for a specific measured action tick."""

import hashlib
import json
from pathlib import Path

import numpy as np

CAMERAS = ("head", "left_wrist", "right_wrist")


def observation(trial, tick, output):
    def read(name):
        return json.loads((trial / name).read_bytes())

    result = read("result.json")
    calibration = read("calibration.json")
    if result["kind"] != "table-transfer" or not 0 <= tick <= result["physics_ticks"]:
        raise ValueError("Decision tick does not belong to this Nano transfer")
    stamp = round(tick * calibration["physics_dt_s"] * 1e9)
    failure = result.get("last_pickup_observation", {})
    if failure.get("stopped_tick") == tick and failure.get("stamp_ns") == stamp:
        file = trial / "last-pickup-observation.npz"
        if (
            failure.get("file") != file.name
            or hashlib.sha256(file.read_bytes()).hexdigest() != failure["sha256"]
        ):
            raise ValueError("Stopped pickup packet does not match its receipt")
    else:
        file = trial / "grasp-observations" / f"{tick:06d}.npz"
        with (trial / "actions.jsonl").open() as actions:
            if not any(
                row.get("tick") == tick
                and row.get("op") == "grasp observation"
                and row.get("stamp_ns") == stamp
                for row in map(json.loads, actions)
            ):
                raise ValueError("No recorded pickup observation at this tick")
    with np.load(file, allow_pickle=False) as packet:
        if (
            set(packet.files) != {*CAMERAS, "encoders", "stamp_ns"}
            or packet["stamp_ns"].shape != ()
            or int(packet["stamp_ns"]) != stamp
        ):
            raise ValueError("Unexpected policy input channels or exposure timestamp")
        encoders = packet["encoders"]
        if (
            encoders.shape != (len(calibration["joint_names"]),)
            or not np.isfinite(encoders).all()
        ):
            raise ValueError("Invalid encoder observation")
        images = [packet[c] for c in CAMERAS]
        if any(
            image.shape != (360, 480, 3) or image.dtype != np.uint8 for image in images
        ):
            raise ValueError("Invalid onboard RGB geometry")
        output.write_bytes(np.concatenate(images, axis=1).tobytes())
        hashes = {
            k: hashlib.sha256(packet[k].tobytes()).hexdigest()
            for k in (*CAMERAS, "encoders")
        }
    return dict(
        trial=trial.name,
        tick=tick,
        stamp_ns=stamp,
        channels=hashes,
        file=file.relative_to(trial).as_posix(),
        packet_sha256=hashlib.sha256(file.read_bytes()).hexdigest(),
    )


def compare(args):
    rows = {}
    for side, pair in (
        ("previous", args.previous_packet),
        ("current", args.current_packet),
    ):
        if pair is not None:
            trial, tick = pair
            rows[side] = observation(Path(trial), int(tick), args.out / (side + ".rgb"))
    if not rows:
        raise ValueError("No retained policy exposure at the changed action")
    same = None
    if len(rows) == 2:
        a, b = rows.values()
        same = a["channels"] == b["channels"] and a["stamp_ns"] == b["stamp_ns"]
    return dict(cameras=CAMERAS, views=rows, identical_inputs=same)
