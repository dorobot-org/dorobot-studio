"""Localize floor targets from a recorded Nano RGB exposure and encoder vector."""

import io
import math
from pathlib import Path
import shutil

import numpy as np
from worker import digest, save

PACKET = "onboard-scene.npz"
CALIBRATION = "onboard-calibration.json"
SUMMARY = "onboard-scene.json"


def interpret(payload, calibration, model):
    from nano_rgb_policy import COLORS, VisualServo

    objects = {}
    with np.load(io.BytesIO(payload), allow_pickle=False) as packet:
        stamp = int(packet["stamp_ns"])
        for color in COLORS:
            servo = VisualServo(
                calibration | dict(model=str(model), color=color, standoff_m=0.45)
            )
            detected = servo.command(packet)
            if detected["xy"] is None:
                objects[color] = dict(status=detected["state"])
                continue
            x, y = detected["xy"]
            objects[color] = dict(
                status="localized",
                x_forward_m=round(x, 4),
                y_left_m=round(y, 4),
                distance_m=round(math.hypot(x, y), 4),
                views=detected["cameras"],
            )
    return dict(frame="base_link", capture_ns=stamp, objects=objects)


def source_hashes():
    root = Path(__file__).resolve().parents[1] / "aisle-validation"
    return {
        name: digest(root / name)
        for name in (
            "nano_living.py",
            "nano_cameras.py",
            "nano_rgb_policy.py",
            "run_rgb_approach.py",
        )
    }


def capture(output, meta, seed, room_hash, obstacles, settings):
    from run_rgb_approach import approach_scene, camera_config, sensor_packet

    model = Path(meta["model"])
    sim = approach_scene(model, seed, obstacles, settings)
    config = camera_config(sim, model)
    config.pop("model")
    payload, _ = sensor_packet(sim, output, save_frame=False)
    public = interpret(payload, config, model)
    (output / PACKET).write_bytes(payload)
    save(output / CALIBRATION, config)
    save(output / SUMMARY, public)
    binding = dict(
        layout=seed,
        source_sha256=source_hashes(),
        room_revision_sha256=room_hash,
        visible=sum(v["status"] == "localized" for v in public["objects"].values()),
        files={name: digest(output / name) for name in (PACKET, CALIBRATION, SUMMARY)},
    )
    return public, binding


def bind(source, output, binding, seed, room_hash):
    if (
        binding.get("layout") != seed
        or binding.get("room_revision_sha256") != room_hash
    ):
        raise ValueError(
            "The room or layout changed; capture a fresh onboard view before running this controller"
        )
    if binding.get("source_sha256") != source_hashes():
        raise ValueError("Scene or camera code changed; capture a fresh onboard view")
    files = binding.get("files", {})
    if set(files) != {PACKET, CALIBRATION, SUMMARY}:
        raise ValueError("Onboard observation receipt is incomplete")
    for name, expected in files.items():
        if digest(source / name) != expected:
            raise ValueError(f"Onboard observation changed: {name}")
    for name in files:
        shutil.copyfile(source / name, output / name)
