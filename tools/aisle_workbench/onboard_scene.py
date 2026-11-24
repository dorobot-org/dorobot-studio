"""Localize floor targets from a recorded Nano RGB exposure and encoder vector."""

import io
import math
from pathlib import Path
import shutil

from worker import digest, save

PACKET = "onboard-scene.npz"
CALIBRATION = "onboard-calibration.json"
SUMMARY = "onboard-scene.json"
PREVIEW = "onboard-head.rgb"
AUTHOR_CAMERAS = ("head", "left_wrist", "right_wrist")


def interpret(payload, calibration, model):
    import numpy as np
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
    payload, head = sensor_packet(sim, output, save_frame=False)
    public = interpret(payload, config, model)
    (output / PACKET).write_bytes(payload)
    (output / PREVIEW).write_bytes(head.tobytes())
    save(output / CALIBRATION, config)
    save(output / SUMMARY, public)
    binding = dict(
        layout=seed,
        source_sha256=source_hashes(),
        room_revision_sha256=room_hash,
        visible=sum(v["status"] == "localized" for v in public["objects"].values()),
        files={
            name: digest(output / name)
            for name in (PACKET, CALIBRATION, SUMMARY, PREVIEW)
        },
    )
    return public, binding


def author_input(output, binding, provider):
    """Prepare the selected provider's inputs from the retained sensor exposure."""
    receipt = {
        "provider": provider,
        "packet_sha256": binding["files"][PACKET],
        "calibration_sha256": binding["files"][CALIBRATION],
        "measurements_sha256": binding["files"][SUMMARY],
        "images": [],
    }
    if provider == "octos-deepseek":
        return receipt
    if provider != "codex":
        raise ValueError("Choose a supported author for onboard perception")

    import numpy as np
    from PIL import Image

    with np.load(output / PACKET, allow_pickle=False) as packet:
        for name in AUTHOR_CAMERAS:
            rgb = packet[name]
            if rgb.shape != (360, 480, 3) or rgb.dtype != np.uint8:
                raise ValueError(f"Invalid onboard RGB capture for {name}")
            path = output / f"author-{name}.png"
            Image.fromarray(rgb).save(path)
            attachment_hash = digest(path)
            binding["files"][path.name] = attachment_hash
            receipt["images"].append(
                dict(camera=name, file=path.name, sha256=attachment_hash)
            )
    return receipt


def observation_files(binding):
    """Return only the recognized files for this version of the capture receipt."""
    files = binding.get("files")
    base = {PACKET, CALIBRATION, SUMMARY}
    if not isinstance(files, dict):
        raise ValueError("Onboard observation receipt is incomplete")
    if "author_input" not in binding:
        if set(files) not in (base, base | {PREVIEW}):
            raise ValueError("Unexpected legacy onboard observation files")
        return files

    receipt = binding["author_input"]
    if not isinstance(receipt, dict) or receipt.get("provider") not in (
        "codex",
        "octos-deepseek",
    ):
        raise ValueError("Unknown onboard author input provider")
    for key, name in (
        ("packet_sha256", PACKET),
        ("calibration_sha256", CALIBRATION),
        ("measurements_sha256", SUMMARY),
    ):
        if name not in files or receipt.get(key) != files[name]:
            raise ValueError(f"Invalid onboard author input digest for {name}")
    required = base | {PREVIEW}
    expected = []
    if receipt["provider"] == "codex":
        for camera in AUTHOR_CAMERAS:
            name = f"author-{camera}.png"
            required.add(name)
            expected.append(dict(camera=camera, file=name, sha256=files.get(name)))
    if set(files) != required or receipt.get("images") != expected:
        raise ValueError("Onboard author attachments differ from their capture receipt")
    if any(
        not isinstance(h, str)
        or len(h) != 64
        or any(c not in "0123456789abcdef" for c in h)
        for h in files.values()
    ):
        raise ValueError("Invalid onboard capture digest")
    return files


def verify_observation_files(source, binding):
    files = observation_files(binding)
    for name, expected in files.items():
        if digest(source / name) != expected:
            raise ValueError(f"Onboard observation changed: {name}")
    images = binding.get("author_input", {}).get("images", [])
    if images:
        from PIL import Image
        import numpy as np

        with np.load(source / PACKET, allow_pickle=False) as packet:
            for entry in images:
                with Image.open(source / entry["file"]) as image:
                    if (
                        image.format != "PNG"
                        or image.mode != "RGB"
                        or image.size != (480, 360)
                        or not np.array_equal(
                            np.asarray(image), packet[entry["camera"]]
                        )
                    ):
                        raise ValueError(
                            f"Author image differs from onboard camera {entry['camera']}"
                        )
    return files


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
    for name in verify_observation_files(source, binding):
        shutil.copyfile(source / name, output / name)
