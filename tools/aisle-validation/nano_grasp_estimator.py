"""Color and encoder features used by a Nano wrist-camera carry checkpoint."""

import hashlib
import json
import re

import cv2
import numpy as np

METHOD = "nano-wrist-carry-logistic-v1"
FEATURES = (
    "color_fraction",
    "aperture_fraction",
    "upper_left",
    "upper_right",
    "middle_left",
    "middle_right",
    "lower_left",
    "lower_right",
    "gripper_rad",
    "wrist_roll_rad",
)


def color_mask(hsv, color):
    hue, sat, val = hsv.transpose(2, 0, 1)
    low, high = {"red": (-7, 7), "green": (45, 82), "blue": (106, 132)}[color]
    hue_ok = (
        ((hue >= low) & (hue <= high))
        if low >= 0
        else ((hue <= high) | (hue >= 180 + low))
    )
    return hue_ok & (sat > 125) & (val > 55)


def features(hsv, color, gripper, roll):
    mask = color_mask(hsv, color)
    h, w = mask.shape
    # The calibrated wrist view places the gap between the closed jaws in this rectangle.
    aperture = mask[
        round(0.61 * h) : round(0.89 * h), round(0.17 * w) : round(0.48 * w)
    ]
    cells = [
        cell.mean()
        for strip in np.array_split(aperture, 3)
        for cell in np.array_split(strip, 2, axis=1)
    ]
    return np.array([mask.mean(), aperture.mean(), *cells, gripper, roll], dtype=float)


def checkpoint_bytes(checkpoint):
    return (
        json.dumps(
            checkpoint, sort_keys=True, allow_nan=False, separators=(",", ":")
        ).encode()
        + b"\n"
    )


def parameters(plan):
    required = (
        "grasp_estimator",
        "grasp_checkpoint",
        "training_dataset",
        "parent_controller_sha256",
    )
    if not any(k in plan for k in required[:2]):
        return None
    if not all(k in plan for k in required):
        raise ValueError("Incomplete Nano grasp checkpoint")
    ck = plan["grasp_estimator"]
    if (
        not isinstance(ck, dict)
        or ck.get("method") != METHOD
        or ck.get("features") != list(FEATURES)
        or ck.get("model_hash") != plan.get("model_hash")
        or ck.get("dataset_sha256") != plan.get("training_dataset")
        or ck.get("teacher_controller_sha256") != plan.get("parent_controller_sha256")
        or not all(
            isinstance(ck.get(k), str) and re.fullmatch("[0-9a-f]{64}", ck[k])
            for k in (
                "dataset_sha256",
                "teacher_controller_sha256",
                "model_hash",
                "training_sha256",
            )
        )
        or hashlib.sha256(checkpoint_bytes(ck)).hexdigest()
        != plan.get("grasp_checkpoint")
    ):
        raise ValueError(
            "Nano grasp checkpoint differs from its robot, task lineage or digest"
        )
    mean, scale, weights = (
        np.asarray(ck[k], dtype=float) for k in ("mean", "scale", "weights")
    )
    bias = ck["bias"]
    if (
        any(
            x.shape != (len(FEATURES),) or not np.isfinite(x).all()
            for x in (mean, scale, weights)
        )
        or np.any(scale <= 0)
        or type(bias) not in (float, int)
        or not np.isfinite(bias)
    ):
        raise ValueError("Invalid Nano grasp checkpoint coefficients")
    return mean, scale, weights, bias


def predict(model, rgb, color, gripper, roll):
    mean, scale, weights, bias = model
    x = features(cv2.cvtColor(rgb, cv2.COLOR_RGB2HSV), color, gripper, roll)
    return bool(((x - mean) / scale) @ weights + bias >= 0.0)
