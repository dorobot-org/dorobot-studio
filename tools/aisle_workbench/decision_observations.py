"""Read the retained Nano policy input for a specific measured action tick."""

import hashlib
from io import BytesIO
import json
from pathlib import Path

import numpy as np

CAMERAS = ("head", "left_wrist", "right_wrist")


def channels(raw, stamp, calibration):
    with np.load(BytesIO(raw), allow_pickle=False) as packet:
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
        return dict(zip(CAMERAS, images)) | {
            "encoders": encoders,
            "stamp_ns": packet["stamp_ns"],
        }


def stopped_packet(trial, failure):
    path = trial / "last-pickup-observation.npz"
    raw = path.read_bytes()
    if failure.get("file") != path.name or hashlib.sha256(
        raw
    ).hexdigest() != failure.get("sha256"):
        raise ValueError("Stopped pickup packet does not match its receipt")
    return raw


def pickup_feedback(trial, robot, failure, output):
    """Measure visible alternatives using only calibrated onboard RGB and encoders."""
    from nano_rgb_policy import VisualServo

    calibration = json.loads((trial / "calibration.json").read_bytes())
    raw = stopped_packet(trial, failure)
    stamp = failure["stamp_ns"]
    stop = failure["stopped_tick"]
    if (
        type(stamp) is not int
        or type(stop) is not int
        or not 0 <= stamp <= round(stop * calibration["physics_dt_s"] * 1e9)
    ):
        raise ValueError("Invalid stopped pickup timestamp")
    packet = channels(raw, stamp, calibration)
    servo = VisualServo(
        dict(
            model=robot["model"],
            joint_names=calibration["joint_names"],
            cameras=calibration["cameras"],
            color=failure["color"],
            standoff_m=0.45,
        )
    )
    candidates = {
        c: [
            xy.tolist()
            for xy in servo.camera_candidates(c, packet[c], packet["encoders"])
        ]
        for c in CAMERAS
    }
    if output is not None:
        (output / "revision-observation.npz").write_bytes(raw)
    return dict(
        color=failure["color"],
        stamp_ns=stamp,
        stopped_tick=stop,
        packet_sha256=failure["sha256"],
        association=calibration.get("grasp_rgb_association", "all-views-unique-v1"),
        candidate_xy_m=candidates,
        coordinate_frame="base_link",
        unit="metres",
    )


def observation(trial, tick, output):
    def read(name):
        return json.loads((trial / name).read_bytes())

    result = read("result.json")
    calibration = read("calibration.json")
    if result["kind"] != "table-transfer" or not 0 <= tick <= result["physics_ticks"]:
        raise ValueError("Decision tick does not belong to this Nano transfer")
    stamp = round(tick * calibration["physics_dt_s"] * 1e9)
    failure = result.get("last_pickup_observation") or {}
    if failure.get("stopped_tick") == tick and failure.get("stamp_ns") == stamp:
        file = trial / "last-pickup-observation.npz"
        raw = stopped_packet(trial, failure)
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
        raw = file.read_bytes()
    packet = channels(raw, stamp, calibration)
    images = [packet[c] for c in CAMERAS]
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
        packet_sha256=hashlib.sha256(raw).hexdigest(),
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
