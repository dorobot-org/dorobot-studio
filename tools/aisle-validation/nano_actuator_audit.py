"""Guard and independently replay each Nano motor setpoint in joint-name order."""

import hashlib
import xml.etree.ElementTree as ET

import numpy as np

from nano_living import MAX_WHEEL_RATE


class MotorEnvelope:
    def __init__(self, model, names, dt=0.01):
        tree = ET.parse(model)
        actuated = {
            j.get("name")
            for j in tree.findall("joint")
            if j.get("type") in ("revolute", "continuous")
        }
        if len(names) != len(actuated) or set(names) != actuated:
            raise ValueError("Actuator journal must cover each URDF motor exactly once")
        bounds = []
        for name in names:
            joint = tree.find(f"joint[@name='{name}']")
            limit = joint.find("limit")
            if joint.get("type") == "continuous":
                bounds.append(
                    (-np.inf, np.inf, min(float(limit.get("velocity")), MAX_WHEEL_RATE))
                )
            else:
                bounds.append(
                    tuple(float(limit.get(k)) for k in ("lower", "upper", "velocity"))
                )
        self.low, self.high, speed = np.array(bounds).T
        self.max_step = speed * dt

    def valid(self, targets, previous):
        return (
            np.isfinite(targets)
            & (targets >= self.low - 1e-9)
            & (targets <= self.high + 1e-9)
            & (np.abs(targets - previous) <= self.max_step + 1e-9)
        ).all(axis=-1)


class MotorWriter:
    def __init__(self, stream, envelope, initial):
        self.stream = stream
        self.envelope = envelope
        self.previous = np.asarray(initial, dtype="<f8").copy()
        if self.previous.shape != envelope.low.shape or not envelope.valid(
            self.previous, self.previous
        ):
            raise ValueError("Invalid starting motor targets")
        stream.write(self.previous.tobytes())
        self.rejected = 0

    def apply(self, robot, dofs, targets):
        row = np.asarray(targets, dtype="<f8")
        if row.shape != self.previous.shape or not self.envelope.valid(
            row, self.previous
        ):
            self.rejected += 1
            raise ValueError(
                "Motor setpoint violates the measured step's position or rate envelope"
            )
        robot.control_dofs_position(row.tolist(), dofs)
        self.stream.write(row.tobytes())
        self.previous[:] = row


def replay(path, envelope, physics_ticks):
    """Recheck the journal independently of the writer. Row zero is the starting target."""
    width = len(envelope.low)
    raw = path.read_bytes()
    if len(raw) % (8 * width) or len(raw) < 8 * width:
        raise ValueError("Truncated Nano actuator journal")
    rows = np.frombuffer(raw, dtype="<f8").reshape(-1, width)
    applied = len(rows) - 1
    invalid = int(np.count_nonzero(~envelope.valid(rows[1:], rows[:-1])))
    missing_or_extra = abs(applied - physics_ticks)
    valid_initial = bool(envelope.valid(rows[0], rows[0]))
    return dict(
        format="little-endian float64, initial targets then one row per physics tick",
        applied_rows=applied,
        invalid_rows=invalid,
        unmatched_ticks=missing_or_extra,
        initial_targets_valid=valid_initial,
        sha256=hashlib.sha256(raw).hexdigest(),
        passed=valid_initial and not (invalid or missing_or_extra),
    )
