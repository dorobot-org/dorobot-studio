"""Measure upright, room-aligned box proposals in a calibrated reference photograph."""

import cv2
import numpy as np
from scipy.optimize import least_squares


def camera(marks, width, height, focal_px=None):
    floor = np.asarray([mark["metres"] for mark in marks], dtype=float)
    pixels = np.asarray([mark["pixel"] for mark in marks], dtype=float)
    if len(floor) < 5 or floor.shape != pixels.shape or floor.shape[1:] != (2,):
        raise ValueError(
            "Provide at least five measured floor marks in the reference photograph"
        )
    if not np.isfinite(floor).all() or not np.isfinite(pixels).all():
        raise ValueError("Floor marks must have finite coordinates")
    h, _ = cv2.findHomography(floor, pixels)
    if h is None:
        raise ValueError("The floor marks do not determine a camera")
    center = np.array([width / 2, height / 2])
    column = h[:2] - center[:, None] * h[2]
    if focal_px is None:
        denominator = h[2, 0] * h[2, 1]
        if abs(denominator) < 1e-6:
            raise ValueError(
                "Provide focal_px for a reference with a frontal floor axis"
            )
        square = -(column[:, 0] @ column[:, 1]) / denominator
        if square <= 0:
            raise ValueError(
                "Floor marks do not resolve a positive camera focal length"
            )
        focal_px = np.sqrt(square)
    if not 0.15 * width <= focal_px <= 8 * width:
        raise ValueError("Check the camera focal length and floor marks")
    k = np.array([[focal_px, 0, center[0]], [0, focal_px, center[1]], [0, 0, 1.0]])
    rot = np.linalg.solve(k, h[:, :2])
    scale = np.linalg.norm(rot, axis=0).mean()
    vertical = k @ np.cross(rot[:, 0], rot[:, 1]) / scale
    return h, np.column_stack((h[:, 0], h[:, 1], vertical, h[:, 2])), float(focal_px)


CORNERS = np.array(
    [[x, y, z] for z in (0, 1) for x, y in ((-1, -1), (1, -1), (1, 1), (-1, 1))]
)


def project_box(box, projection):
    x, y, width, depth, height = box
    xyz = CORNERS * [width / 2, depth / 2, height] + [x, y, 0]
    rays = np.column_stack((xyz, np.ones(8))) @ projection.T
    return rays[:, :2] / rays[:, 2, None]


def fit_box(contour, h, projection):
    hull = cv2.convexHull(contour).reshape(-1, 2)
    observed = cv2.approxPolyDP(
        hull, max(1.2, cv2.arcLength(hull, True) * 0.009), True
    ).reshape(-1, 2)
    if len(observed) != 6:
        return None
    ground = cv2.perspectiveTransform(observed[None].astype(float), np.linalg.inv(h))[0]
    x, y = ground[observed[:, 1].argsort()[-3:]].mean(axis=0)
    span = np.maximum(0.05, np.ptp(ground, axis=0))
    initial = [x, y, min(span[0], 3), min(span[1], 3), 0.30]
    xy = project_box(initial, projection)
    indices = cv2.convexHull(xy.astype(np.float32), returnPoints=False).ravel()
    if len(indices) != 6:
        return None
    orders = [
        np.roll(v, i, axis=0) for v in (observed, observed[::-1]) for i in range(6)
    ]
    # A box and its observed silhouette share consecutive convex boundary vertices.
    # Solve each possible cyclic correspondence so the starting estimate cannot select a wrong face.
    best = None
    for target in orders:

        def residual(b):
            return (project_box(b, projection)[indices] - target).ravel()

        fit = least_squares(
            residual,
            initial,
            bounds=([-10, -10, 0.02, 0.02, 0.02], [10, 10, 4, 4, 4]),
            max_nfev=80,
        )
        if not fit.success:
            continue
        corners = project_box(fit.x, projection)
        actual = cv2.convexHull(corners.astype(np.float32), returnPoints=False).ravel()
        if set(actual) != set(indices):
            continue
        error = np.linalg.norm(residual(fit.x).reshape(-1, 2), axis=1).max()
        if best is None or error < best[0]:
            best = error, fit.x, corners
    if best is None or best[0] > max(
        2.5, np.linalg.norm(np.ptp(observed, axis=0)) * 0.025
    ):
        return None
    return best


def proposals(rgb, sheet):
    height, width = rgb.shape[:2]
    h, projection, focal = camera(
        sheet["floor_marks"], width, height, sheet.get("focal_px")
    )
    hsv = cv2.cvtColor(rgb, cv2.COLOR_RGB2HSV)
    hue, saturation, value = hsv.transpose(2, 0, 1)
    chromatic = (saturation > 24) & (value > 36)
    hist = np.bincount(hue[chromatic], minlength=180)
    peaks = []
    for color in np.argsort(-hist):
        if hist[color] < 40:
            break
        if any(min(abs(int(color) - c), 180 - abs(int(color) - c)) < 10 for c in peaks):
            continue
        peaks.append(int(color))
    items = []
    floor_hull = cv2.convexHull(
        np.array([m["pixel"] for m in sheet["floor_marks"]], dtype=np.float32)
    )
    for color in peaks[:20]:
        delta = abs(hue.astype(np.int16) - color)
        mask = (chromatic & (np.minimum(delta, 180 - delta) < 9)).astype(np.uint8)
        contours, _ = cv2.findContours(mask, cv2.RETR_EXTERNAL, cv2.CHAIN_APPROX_SIMPLE)
        for contour in contours:
            area = cv2.contourArea(contour)
            if (
                not 180 <= area <= height * width * 0.40
                or area < 0.92 * cv2.contourArea(cv2.convexHull(contour))
            ):
                continue
            fit = fit_box(contour, h, projection)
            if fit is None:
                continue
            error, box, corners = fit
            if any(
                cv2.pointPolygonTest(floor_hull, tuple(xy), False) < 0
                for xy in corners[:4]
            ):
                continue
            region = np.zeros_like(mask)
            cv2.drawContours(region, [contour], -1, 1, cv2.FILLED)
            mean_rgb = (
                np.median(rgb[region.astype(bool)], axis=0).round().astype(int).tolist()
            )
            items.append(
                dict(
                    center_xy_m=box[:2].tolist(),
                    size_m=box[2:].tolist(),
                    rgb=mean_rgb,
                    footprint_pixels=corners[:4].tolist(),
                    max_vertex_error_px=float(error),
                    image_bounds_xywh=list(cv2.boundingRect(contour)),
                )
            )
    items.sort(key=lambda obj: tuple(obj["center_xy_m"]))
    for i, obj in enumerate(items):
        obj["id"] = i + 1
    return dict(focal_px=focal, objects=items)


def draft_room(source, out, scene, model_sha, provider):
    import json
    from pathlib import Path
    import tempfile

    from PIL import Image
    from room_revision import import_room, sensor_settings
    from worker import digest, request_controller, save

    sheet = json.loads(source.read_bytes())
    if (
        not isinstance(sheet, dict)
        or sheet.get("version") != 1
        or not isinstance(sheet.get("description"), str)
        or not sheet["description"].strip()
        or not set(sheet)
        <= {
            "version",
            "description",
            "image",
            "floor_marks",
            "focal_px",
            "environment",
            "camera_calibration",
        }
    ):
        raise ValueError(
            "Use a photo input with a description, image and measured floor marks"
        )
    if provider not in ("octos-deepseek", "codex"):
        raise ValueError("Select DeepSeek via Octos or Codex to draft the room")
    sensor_settings(sheet, source, model_sha)
    photo = source.parent / sheet["image"]
    with Image.open(photo) as image:
        if image.format not in ("JPEG", "PNG") or image.getexif().get(274, 1) != 1:
            raise ValueError("Use an upright reference JPEG or PNG")
        rgb = np.asarray(image.convert("RGB"))
    measured = proposals(rgb, sheet)
    if not measured["objects"]:
        raise ValueError(
            "No colored room-aligned box has a measurable silhouette inside the calibrated floor area"
        )
    save(out / "photo-measurements.json", measured)
    geometry = {item["id"]: item for item in measured["objects"]}
    options = [
        {k: v for k, v in item.items() if k != "footprint_pixels"}
        for item in geometry.values()
    ]
    schema = dict(
        type="object",
        additionalProperties=False,
        required=["supported", "objects"],
        properties=dict(
            supported=dict(type="boolean"),
            objects=dict(
                type="array",
                items=dict(
                    type="object",
                    additionalProperties=False,
                    required=["id", "name"],
                    properties=dict(id=dict(type="integer"), name=dict(type="string")),
                ),
            ),
        ),
    )
    prompt = (
        "Select the photographed objects the operator wants to add to an existing room. "
        "The calibrated image measurements give ID, RGB, image bounds, floor location and dimensions in metres. "
        "Use only the measured IDs. Assign each a short lowercase ASCII name, using underscores between words. "
        "If the description cannot distinguish the requested objects, return supported=false and objects=[]. "
        "Operator description: "
        + sheet["description"]
        + "\nPhotographic measurements: "
        + json.dumps(options)
    )
    choice, model = request_controller(provider, prompt, schema, out)
    selected = choice.get("objects")
    if (
        choice.get("supported") is not True
        or not isinstance(selected, list)
        or not selected
    ):
        raise ValueError("The operator description did not identify a measured object")
    used = set()
    obstacles = []
    for obj in selected:
        if (
            not isinstance(obj, dict)
            or type(obj.get("id")) is not int
            or obj["id"] in used
            or obj["id"] not in geometry
        ):
            raise ValueError(
                "The proposed object does not match a unique image measurement"
            )
        used.add(obj["id"])
        item = geometry[obj["id"]]
        obstacles.append(
            dict(
                name=obj["name"],
                height_m=item["size_m"][2],
                rgb=item["rgb"],
                footprint_pixels=item["footprint_pixels"],
            )
        )
    provenance = dict(
        reference_sha256=digest(photo),
        input_sha256=digest(source),
        measurements_sha256=digest(out / "photo-measurements.json"),
        provider=provider,
        model=model,
        selected=selected,
        reply_sha256=digest(out / "reply.json"),
    )
    with tempfile.TemporaryDirectory(dir=out) as tmp:
        inputs = Path(tmp)
        (inputs / "reference-image").write_bytes(photo.read_bytes())
        save(
            inputs / "input.json",
            dict(
                sheet,
                image="reference-image",
                obstacles=obstacles,
                photo_draft=provenance,
            ),
        )
        import_room(inputs / "input.json", out, scene, model_sha=model_sha)
    save(out / "photo-input.json", sheet)
