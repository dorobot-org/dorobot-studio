"""Import surveyed floor points from a room photograph as metric Nexus collision boxes."""

import hashlib
import io
import json
import re

import cv2
import numpy as np
from PIL import Image, ImageDraw, ImageOps

from worker import digest, save


def project(points, matrix):
    points = np.asarray(points, dtype=float)
    if points.ndim != 2 or points.shape[1] != 2 or not np.isfinite(points).all():
        raise ValueError("Each floor coordinate needs two finite numbers")
    homogeneous = np.column_stack((points, np.ones(len(points)))) @ matrix.T
    if np.any(np.abs(homogeneous[:, 2]) < 1e-8):
        raise ValueError("Surveyed point lies at the floor horizon")
    return homogeneous[:, :2] / homogeneous[:, 2, None]


def validate_spec(specs, scene):
    floor = next(b for b in scene if b["name"] == "floor")
    top = floor["position"][2] + floor["size"][2] / 2
    known = [b for b in scene if b["name"] != "floor"]
    # Include the parked robot footprint during initial scene construction.
    known.append(dict(name="Nano", position=[0, 0, top + 0.2], size=[0.58, 0.62, 0.4]))
    for b in specs:
        name = b["name"]
        size, center = (
            np.asarray(b["size"], dtype=float),
            np.asarray(b["position"], dtype=float),
        )
        if (
            not re.fullmatch(r"room_[A-Za-z0-9_-]{1,40}", name)
            or b.get("fixed") is not True
            or size.shape != (3,)
            or center.shape != (3,)
            or not np.isfinite(center).all()
            or not np.isfinite(size).all()
            or np.any(size < 0.02)
            or np.any(size > 4)
            or abs(center[2] - size[2] / 2 - top) > 0.001
        ):
            raise ValueError(f"Invalid size or floor position for {name}")
        if np.any(
            np.abs(center[:2] - floor["position"][:2]) + size[:2] / 2
            >= np.array(floor["size"][:2]) / 2
        ):
            raise ValueError(f"{name} lies outside the room")
        for other in known:
            if other["name"] == name or np.all(
                np.abs(np.asarray(other["position"]) - center)
                < (np.asarray(other["size"]) + size) / 2 - 1e-5
            ):
                raise ValueError(f"{name} conflicts with {other['name']}")
        known.append(b)


def sensor_settings(sheet, source, model_sha):
    values = dict(sheet.get("environment", {}))
    if not set(values) <= {"ambient_rgb", "floor_friction"}:
        raise ValueError("Use ambient_rgb and floor_friction for the Nexus environment")
    if "ambient_rgb" in values:
        ambient = np.asarray(values["ambient_rgb"], dtype=float)
        if (
            ambient.shape != (3,)
            or not np.isfinite(ambient).all()
            or not np.all((ambient >= 0) & (ambient <= 1))
        ):
            raise ValueError("Use three ambient intensities from zero to one")
    if "floor_friction" in values and not 0 <= values["floor_friction"] <= 2:
        raise ValueError("Choose a floor friction coefficient from zero to two")
    if "camera_calibration" in sheet:
        from camera_survey import resolve_mounts

        spec = sheet["camera_calibration"]
        calibration_dir = source.parent
        if isinstance(spec, str):
            path = source.parent / spec
            spec = json.loads(path.read_text())
            calibration_dir = path.parent
        if not model_sha or spec["model_sha256"] != model_sha or not spec.get("source"):
            raise ValueError(
                "Camera calibration needs the matching Nano model hash and source"
            )
        cameras, reports = resolve_mounts(spec, calibration_dir)
        sheet["camera_calibration"] = spec
        values["cameras"] = cameras
        if reports:
            values["camera_mount_checks"] = reports
        values["calibration_source"] = str(spec["source"])
    return values


def import_room(source, out, scene, edit=None, parent=None, model_sha=None):
    raw = source.read_bytes()
    sheet = json.loads(raw)
    if (
        sheet.get("version") != 1
        or not isinstance(sheet.get("description"), str)
        or not sheet["description"].strip()
    ):
        raise ValueError("Room survey requires version 1 and a description")
    settings = sensor_settings(sheet, source, model_sha)
    photo = (source.parent / sheet["image"]).read_bytes()
    im = Image.open(io.BytesIO(photo))
    if im.format not in ("JPEG", "PNG") or im.getexif().get(274, 1) != 1:
        raise ValueError("Use a PNG or JPEG in its surveyed upright orientation")
    image = im.convert("RGB")
    marks = sheet["floor_marks"]
    if not 5 <= len(marks) <= 100:
        raise ValueError("Survey at least five floor marks to check scale")
    pixels = np.asarray([p["pixel"] for p in marks], dtype=float)
    metres = np.asarray([p["metres"] for p in marks], dtype=float)

    def in_image(pts):
        return (
            pts.ndim == 2
            and pts.shape[1] == 2
            and np.isfinite(pts).all()
            and np.all(pts >= 0)
            and np.all(pts < image.size)
        )

    if (
        not in_image(pixels)
        or len(np.unique(pixels, axis=0)) != len(pixels)
        or metres.shape != pixels.shape
        or len(np.unique(metres, axis=0)) != len(metres)
        or not np.isfinite(metres).all()
    ):
        raise ValueError("Invalid floor survey coordinates")
    if np.linalg.matrix_rank(metres - metres.mean(axis=0)) != 2:
        raise ValueError("Survey floor marks across two independent directions")
    h, _ = cv2.findHomography(pixels, metres, method=0)
    if h is None or not np.isfinite(h).all() or abs(np.linalg.det(h)) < 1e-14:
        raise ValueError("Floor survey does not determine a homography")
    errors = np.linalg.norm(project(pixels, h) - metres, axis=1)
    if errors.max() > 0.020:
        raise ValueError(
            f"Floor calibration error {errors.max():.3f} m exceeds 0.020 m"
        )
    if not 1 <= len(sheet["obstacles"]) <= 40:
        raise ValueError("Select between one and forty floor obstacles")
    floor = next(b for b in scene if b["name"] == "floor")
    z = floor["position"][2] + floor["size"][2] / 2
    specs = []
    for item in sheet["obstacles"]:
        corners = np.array(item["footprint_pixels"], dtype=float)
        if corners.shape != (4, 2) or not in_image(corners):
            raise ValueError("Select all four floor corners inside the image")
        if any(
            cv2.pointPolygonTest(
                cv2.convexHull(pixels.astype(np.float32)), tuple(c), False
            )
            < 0
            for c in corners
        ):
            raise ValueError("Keep the footprint inside the surveyed floor area")
        world = project(corners, h)
        low, high = world.min(0), world.max(0)
        rect = np.array([[low[0], low[1]], [high[0], low[1]], high, [low[0], high[1]]])
        distances = np.linalg.norm(world[:, None, :] - rect, axis=2)
        if (
            len(set(distances.argmin(axis=1))) != 4
            or distances.min(axis=1).max() > 0.025
        ):
            raise ValueError(
                "Reference boxes must align with the room axes within 25 mm"
            )
        height = float(item["height_m"])
        color = np.asarray(item.get("rgb", [120, 132, 145]), dtype=float)
        if (
            color.shape != (3,)
            or not np.isfinite(color).all()
            or np.any(color < 0)
            or np.any(color > 255)
        ):
            raise ValueError("Obstacle RGB must be three channels between 0 and 255")
        specs.append(
            dict(
                name="room_" + item["name"],
                size=[*(high - low), height],
                position=[*((high + low) / 2), z + height / 2],
                color=[*(color / 255), 1.0],
                fixed=True,
            )
        )
    if parent is not None:
        previous = json.loads(source.with_name("room.json").read_text())
        if (
            digest(source.with_name("room.json")) != parent
            or (
                "archived_survey_sha256" in previous
                and previous["archived_survey_sha256"]
                != hashlib.sha256(raw).hexdigest()
            )
            or previous["reference_sha256"] != hashlib.sha256(photo).hexdigest()
        ):
            raise ValueError("The selected parent room changed")
        specs = previous["obstacles"]
    if edit and edit.strip():
        tokens = edit.split()
        if len(tokens) != 4:
            raise ValueError("Edit format is name x_m y_m height_m")
        obj = next((b for b in specs if b["name"] == "room_" + tokens[0]), None)
        if obj is None:
            raise ValueError("Edit names an unknown obstacle")
        x, y, height = map(float, tokens[1:])
        obj["size"][2] = height
        obj["position"] = [x, y, z + height / 2]
    validate_spec(specs, scene)
    sketch = image.copy()
    draw = ImageDraw.Draw(sketch)
    inverse = np.linalg.inv(h)
    for box in specs:
        x, y = box["position"][:2]
        dx, dy = np.array(box["size"][:2]) / 2
        ring = project(
            [[x - dx, y - dy], [x + dx, y - dy], [x + dx, y + dy], [x - dx, y + dy]],
            inverse,
        )
        pts = [tuple(p) for p in ring]
        draw.line(pts + [pts[0]], fill="#15eed5", width=3)
        draw.text(
            pts[0], box["name"][5:], stroke_width=2, stroke_fill="black", fill="white"
        )
    preview = ImageOps.pad(sketch, (480, 320), color="#202932")
    (out / "room-preview.rgb").write_bytes(preview.tobytes())
    (out / "reference-image").write_bytes(photo)
    # Normalize the input path so the revision can be moved to another machine.
    sheet["image"] = "reference-image"
    save(out / "survey.json", sheet)
    revision = dict(
        version=1,
        description=sheet["description"],
        **({"photo_draft": sheet["photo_draft"]} if "photo_draft" in sheet else {}),
        model_sha256=model_sha,
        survey_sha256=hashlib.sha256(raw).hexdigest(),
        archived_survey_sha256=digest(out / "survey.json"),
        reference_sha256=hashlib.sha256(photo).hexdigest(),
        parent_sha256=parent,
        image_to_floor=h.tolist(),
        max_calibration_error_m=float(errors.max()),
        edit=edit,
        obstacles=specs,
        settings=settings,
    )
    save(out / "room.json", revision)
    save(
        out / "room-ref.json",
        {"sha256": digest(out / "room.json"), "revision": out.name},
    )
    return revision


def load_revision(path, expected_hash, scene, model_sha=None):
    raw = path.read_bytes()
    if hashlib.sha256(raw).hexdigest() != expected_hash:
        raise ValueError("Room revision differs from the selected hash")
    data = json.loads(raw)
    if (
        data["version"] != 1
        or (
            "archived_survey_sha256" in data
            and digest(path.with_name("survey.json")) != data["archived_survey_sha256"]
        )
        or digest(path.with_name("reference-image")) != data["reference_sha256"]
    ):
        raise ValueError("Room revision reference changed")
    if data.get("settings", {}).get("cameras") is not None:
        archived = json.loads(path.with_name("survey.json").read_text())
        calibration = archived["camera_calibration"]
        if (
            not model_sha
            or calibration["model_sha256"] != model_sha
            or data.get("model_sha256", model_sha) != model_sha
        ):
            raise ValueError(
                "Selected camera revision belongs to a different Nano model"
            )
    specs = data["obstacles"]
    validate_spec(specs, scene)
    return data
