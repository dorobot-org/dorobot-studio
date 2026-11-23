"""URDF kinematics used to plan the Nano's five-joint left arm."""

from pathlib import Path
import xml.etree.ElementTree as ET

import numpy as np
from scipy.optimize import least_squares
from scipy.spatial.transform import Rotation

MODEL = Path(__file__).resolve().parents[2] / "data/adora_nano/adora_nano.urdf"
# Mid-gap for a 40 mm cube: the Nano has one fixed and one hinged finger.
# This is offset from the wrist axis, not the empty tool-mount origin.
GRIP_POINT = np.array([0.00971944, 0.00235211, -0.108])


def vector(element, key, default="0 0 0"):
    return np.fromstring(element.get(key, default), sep=" ")


class Kinematics:
    def __init__(self, path=MODEL):
        self.root = ET.parse(path).getroot()
        self.joints = []
        self.names = []
        self.limits = {}
        for j in self.root.findall("joint"):
            origin = j.find("origin")
            t = np.eye(4)
            if origin is not None:
                t[:3, :3] = Rotation.from_euler(
                    "xyz", vector(origin, "rpy")
                ).as_matrix()
                t[:3, 3] = vector(origin, "xyz")
            axis = j.find("axis")
            name, kind = j.get("name"), j.get("type")
            self.joints.append(
                (
                    name,
                    kind,
                    j.find("parent").get("link"),
                    j.find("child").get("link"),
                    t,
                    vector(axis, "xyz") if axis is not None else np.zeros(3),
                )
            )
            if kind != "fixed":
                self.names.append(name)
                limit = j.find("limit")
                self.limits[name] = (
                    (-np.inf, np.inf)
                    if kind == "continuous"
                    else (float(limit.get("lower")), float(limit.get("upper")))
                )
        self.arm = [
            n for n in self.names if n.startswith("left_j") and "gripper" not in n
        ]

    def fk(self, q):
        values = dict(zip(self.names, q)) if not isinstance(q, dict) else q
        result = {"base_link": np.eye(4)}
        pending = self.joints.copy()
        while pending:
            progress = False
            for joint in pending.copy():
                name, kind, parent, child, origin, axis = joint
                if parent not in result:
                    continue
                move = np.eye(4)
                if kind in ("revolute", "continuous"):
                    move[:3, :3] = Rotation.from_rotvec(
                        axis * values.get(name, 0.0)
                    ).as_matrix()
                result[child] = result[parent] @ origin @ move
                pending.remove(joint)
                progress = True
            if not progress:
                raise ValueError("Disconnected URDF")
        return result

    def grip(self, q):
        t = self.fk(q)["left_wrist_housing"]
        return t[:3, 3] + t[:3, :3] @ GRIP_POINT, t[:3, :3]

    def solve(self, pos, initial, down=True, attempts=8):
        initial = np.asarray(initial, dtype=float)
        indices = [self.names.index(n) for n in self.arm]
        lower, upper = np.array([self.limits[n] for n in self.arm]).T
        # Hold wrist roll: otherwise the five-DOF solve can rotate the narrow
        # finger pads around the cube while satisfying position/approach.
        lower[4] = max(lower[4], initial[indices[4]] - 0.0001)
        upper[4] = min(upper[4], initial[indices[4]] + 0.0001)
        pos = np.asarray(pos)

        def residual(x):
            q = initial.copy()
            q[indices] = x
            p, r = self.grip(q)
            # The fingers extend along -Z of the wrist housing. Align +Z up.
            return (
                np.r_[4 * (p - pos), 0.4 * (r[:, 2] - [0, 0, 1])]
                if down
                else 4 * (p - pos)
            )

        rng = np.random.default_rng(3)
        best = None
        for seed in [initial[indices], *rng.uniform(lower, upper, (attempts - 1, 5))]:
            sol = least_squares(
                residual,
                np.clip(seed, lower + 1e-7, upper - 1e-7),
                bounds=(lower, upper),
                max_nfev=150,
                ftol=1e-9,
                xtol=1e-9,
                gtol=1e-9,
            )
            score = np.linalg.norm(sol.fun) + 0.0001 * np.linalg.norm(
                sol.x - initial[indices]
            )
            if best is None or score < best[0]:
                best = score, sol.x
            if np.linalg.norm(sol.fun) < 1e-5:
                break
        q = initial.copy()
        q[indices] = best[1]
        p, r = self.grip(q)
        if np.linalg.norm(p - pos) > 0.005 or (
            down and np.linalg.norm(r[:, 2] - [0, 0, 1]) > 0.04
        ):
            raise ValueError(
                f"Unreachable target {pos.tolist()}; realized {p.tolist()}"
            )
        return q
