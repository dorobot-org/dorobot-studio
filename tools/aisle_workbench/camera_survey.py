"""Fit Nano camera extrinsics from surveyed points on rectified RGB images."""

import base64
import hashlib
import io

import cv2
import numpy as np
from PIL import Image

from nano_cameras import MOUNTS, validate_rig


def fit_mount(name, row):
    marks = row["landmarks"]
    use = np.array([p["use"] for p in marks])
    fit, check = use == "fit", use == "check"
    if not (fit | check).all() or fit.sum() < 6 or check.sum() < 3:
        raise ValueError(
            f"{name} needs at least six fit and three independent check landmarks"
        )
    xyz = np.array([p["link_m"] for p in marks], dtype=np.float64)
    uv = np.array([p["pixel"] for p in marks], dtype=np.float64)
    if (
        xyz.shape != (len(marks), 3)
        or uv.shape != (len(marks), 2)
        or not np.isfinite(xyz).all()
        or not np.isfinite(uv).all()
        or np.any(uv < 0)
        or np.any(uv >= row["resolution"])
        or len(np.unique(xyz, axis=0)) != len(marks)
    ):
        raise ValueError(
            f"{name} needs distinct finite link-frame points and pixels inside the image"
        )
    for group in (fit, check):
        if (
            np.linalg.matrix_rank(xyz[group] - xyz[group].mean(0)) < 2
            or np.linalg.matrix_rank(uv[group] - uv[group].mean(0)) < 2
        ):
            raise ValueError(f"{name} landmarks are collinear")
    intr = np.asarray(row["intrinsics"], dtype=float)
    ok, rvec, tvec = cv2.solvePnP(
        xyz[fit], uv[fit], intr, None, flags=cv2.SOLVEPNP_ITERATIVE
    )
    if not ok:
        raise ValueError(f"{name} camera pose could not be fitted")
    rotation = cv2.Rodrigues(rvec)[0]
    camera_xyz = xyz @ rotation.T + tvec.ravel()
    if not np.isfinite(camera_xyz).all() or np.any(camera_xyz[:, 2] <= 0):
        raise ValueError(f"{name} landmarks must lie in front of the fitted camera")
    projected, _ = cv2.projectPoints(xyz, rvec, tvec, intr, None)
    errors = np.linalg.norm(projected.reshape(-1, 2) - uv, axis=1)
    for label, group in (("fit", fit), ("check", check)):
        if not np.isfinite(errors[group]).all() or errors[group].max() > 2.0:
            raise ValueError(
                f"{name} {label} landmark reprojection error {errors[group].max():.2f} px exceeds 2 px"
            )
    # PnP returns camera_cv_from_link. Nexus mounts camera_gl in the link.
    pose = np.eye(4)
    pose[:3, :3] = rotation.T @ np.diag([1.0, -1.0, -1.0])
    pose[:3, 3] = -rotation.T @ tvec.ravel()
    return pose.tolist(), dict(
        fit_points=int(fit.sum()),
        check_points=int(check.sum()),
        max_fit_px=float(errors[fit].max()),
        max_check_px=float(errors[check].max()),
    )


def resolve_mounts(spec, directory):
    """Return runtime camera inputs and preserve the survey and images for a later edit."""
    original = spec["cameras"]
    optics = {}
    reports = {}
    for name, row in original.items():
        if "landmarks" in row and "link_from_camera_gl" in row:
            raise ValueError(
                f"{name} needs either a declared mount or surveyed landmarks"
            )
        optics[name] = {k: v for k, v in row.items() if k != "landmarks"}
        if "landmarks" in row:
            optics[name]["link_from_camera_gl"] = np.eye(4).tolist()
    validate_rig(optics, (480, 360))
    embedded = spec.get("images", {})
    encoded = {}
    sources = {}
    for name in MOUNTS:
        row = original[name]
        if "landmarks" not in row:
            continue
        pose, reports[name] = fit_mount(name, row)
        optics[name]["link_from_camera_gl"] = pose
        for point in row["landmarks"]:
            ref = point["image"]
            if ref not in sources:
                if ref in embedded:
                    raw = base64.b64decode(embedded[ref], validate=True)
                    if hashlib.sha256(raw).hexdigest() != ref:
                        raise ValueError(
                            f"{name} archived landmark image digest differs"
                        )
                else:
                    raw = (directory / ref).read_bytes()
                with Image.open(io.BytesIO(raw)) as img:
                    if (
                        img.format not in ("JPEG", "PNG")
                        or img.size != (480, 360)
                        or img.getexif().get(274, 1) != 1
                    ):
                        raise ValueError(
                            f"{name} needs an upright 480 × 360 landmark image"
                        )
                    img.load()
                digest = hashlib.sha256(raw).hexdigest()
                encoded[digest] = base64.b64encode(raw).decode("ascii")
                sources[ref] = digest
            point["image"] = sources[ref]
    if encoded:
        spec["images"] = encoded
    validate_rig(optics, (480, 360))
    return optics, reports
