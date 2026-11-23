"""Preregister, execute and independently score a bounded G1 standing-reach cohort."""

import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
from itertools import product
import json
from pathlib import Path
import secrets
import subprocess
import sys
from tempfile import TemporaryDirectory
import xml.etree.ElementTree as ET
from zipfile import ZIP_DEFLATED, ZipFile

import numpy as np

from g1_stand import impact_parameters
from worker import describe, digest, save, validate

HERE = Path(__file__).resolve().parent
FILES = (
    "impact.json",
    "source.json",
    "result.json",
    "controller.json",
    "impact-contacts.json",
    "wrist-trace.npz",
    "motor-work.npz",
    "reach-checkpoint.json",
    "training.json",
)
MODEL = Path(describe("G1")["model"])


def read(path):
    return json.loads(path.read_bytes())


def commitment(opening):
    return hashlib.sha256(
        json.dumps(opening, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def inputs():
    from aisle import kinematics
    from aisle.sim import nexus_backend
    from importlib.metadata import version

    return {
        "urdf": digest(MODEL),
        "source": {
            p.name: digest(p)
            for p in (
                Path(__file__),
                HERE / "g1_stand.py",
                HERE / "worker.py",
                HERE / "g1_reach_checkpoint.py",
                Path(kinematics.__file__),
                Path(nexus_backend.__file__),
            )
        },
        "nexus": version("dimforge-nexus3d"),
        "meshes": {
            m: digest(MODEL.parent / m)
            for m in sorted(
                {x.attrib["filename"] for x in ET.parse(MODEL).iter("mesh")}
            )
        },
    }


def random_openings():
    # Eight corners exercise all combinations of endpoints. Twelve interiors stay sealed.
    samples = list(product((200, 300), (1100, 1300), (-15, 15)))
    while len(samples) < 20:
        sample = (
            201 + secrets.randbelow(99),
            1101 + secrets.randbelow(199),
            secrets.randbelow(29) - 14,
        )
        if sample not in samples:
            samples.append(sample)
    return [
        dict(
            salt=secrets.token_hex(24),
            recipe=dict(mass_kg=m / 1000, speed_m_s=v / 1000, lateral_m=y / 1000),
        )
        for m, v, y in samples
    ]


def prepare(directory, controller, match=None):
    plan = read(controller)
    meta = describe("G1")
    if (
        plan.get("robot") != "G1"
        or plan.get("model_hash") != meta["model_hash"]
        or validate(plan, meta).get("kind") != "standing-inspection"
    ):
        raise ValueError("Select a G1 standing reach controller")
    if "reach_checkpoint" in plan:
        from g1_reach_checkpoint import load

        load(controller.parent, plan)
    if match:
        old = read(match / "suite.json")
        referenced = digest(match / "suite.json")
        report = read(match / "report.json")
        openings = [{k: r[k] for k in ("salt", "recipe")} for r in report["rows"]]
        if (
            old.get("profile") != "g1-standing-reach-v1"
            or len(openings) != 20
            or report["manifest_sha256"] != referenced
            or [commitment(x) for x in openings] != old["commitments"]
        ):
            raise ValueError("The matching report does not open the published G1 cases")
    else:
        openings = random_openings()
    directory.mkdir(parents=True, exist_ok=False)
    private = directory / "openings.json"
    private.touch(mode=0o600, exist_ok=False)
    save(private, openings)
    private.chmod(0o600)
    (directory / "controller.json").write_bytes(controller.read_bytes())
    if "reach_checkpoint" in plan:
        for name in ("reach-checkpoint.json", "training.json"):
            (directory / name).write_bytes((controller.parent / name).read_bytes())
    save(
        directory / "suite.json",
        dict(
            profile="g1-standing-reach-v1",
            **({"matched_manifest_sha256": referenced} if match else {}),
            cases=20,
            required=20,
            boundary_cases=8,
            interior_cases=12,
            inputs=inputs(),
            controller_sha256=digest(controller),
            commitments=[commitment(x) for x in openings],
            criteria=dict(
                scored_steps=2000,
                loaded_feet_steps=2000,
                maximum_settled_wrist_error_m=0.010,
                minimum_requested_travel_m=0.015,
                maximum_recovery_s=2.0,
                stable_steps=100,
                maximum_drift_m=0.10,
                maximum_tilt_deg=12,
                maximum_excursion_rad=0.005,
            ),
        ),
    )
    return digest(directory / "suite.json")


def manifest(directory, expected):
    path = directory / "suite.json"
    m = read(path)
    if (
        digest(path) != expected
        or m["profile"] != "g1-standing-reach-v1"
        or m["cases"] != 20
        or m["required"] != 20
        or len(m["commitments"]) != 20
        or digest(directory / "controller.json") != m["controller_sha256"]
    ):
        raise ValueError("G1 manifest or controller differs from the preregistration")
    return m


def safe_npz(path, shapes):
    # Inspect the embedded ZIP before allowing NumPy to allocate arrays from a user file.
    with ZipFile(path) as z:
        if (
            len(z.infolist()) != len(shapes)
            or {x.filename for x in z.infolist()} != {k + ".npy" for k in shapes}
            or any(x.file_size > 2_000_000 for x in z.infolist())
        ):
            raise ValueError("Unexpected G1 trace members or dimensions")
        for name, expected in shapes.items():
            with z.open(name + ".npy") as stream:
                if np.lib.format.read_magic(stream) != (1, 0):
                    raise ValueError("Unexpected G1 array encoding")
                shape, _, dtype = np.lib.format.read_array_header_1_0(stream)
                if (
                    shape != expected
                    or (
                        name == "joints" and (dtype.kind != "U" or dtype.itemsize > 512)
                    )
                    or (name != "joints" and dtype.kind not in "ifu")
                ):
                    raise ValueError(f"Invalid G1 trace array header {name}")
    with np.load(path, allow_pickle=False) as data:
        result = {name: data[name] for name in shapes}
    for name, shape in shapes.items():
        a = result[name]
        if a.shape != shape or (
            name != "joints" and (a.dtype.kind not in "fiu" or not np.isfinite(a).all())
        ):
            raise ValueError(f"Invalid G1 trace field {name}")
    return result


def score(case, m, opening):
    from aisle.kinematics import load_urdf_chain
    from aisle.sim.nexus_backend import quat_wxyz_to_matrix

    recipe = impact_parameters(opening["recipe"])
    src, result, plan = (
        read(case / n) for n in ("source.json", "result.json", "controller.json")
    )
    disturbance = src["disturbance"]
    if (
        read(case / "impact.json") != recipe
        or digest(case / "controller.json") != m["controller_sha256"]
        or src["controller_sha256"] != m["controller_sha256"]
        or result["model_hash"] != m["inputs"]["urdf"]
        or src["control_sha256"] != m["inputs"]["source"]["g1_stand.py"]
        or src["adapter_hash"] != m["inputs"]["source"]["nexus_backend.py"]
        or src["nexus"] != m["inputs"]["nexus"]
        or disturbance["mass_kg"] != recipe["mass_kg"]
        or disturbance["initial_velocity_m_s"] != [-recipe["speed_m_s"], 0, 0]
        or disturbance["initial_position_m"] != [0.29, recipe["lateral_m"], 0.16]
        or disturbance["launch_step"] != 601
        or result["wrist_reach"]["trace_sha256"] != digest(case / "wrist-trace.npz")
        or result["motor_work_sha256"] != digest(case / "motor-work.npz")
    ):
        raise ValueError("Trial source, controller, trace or impact differs")
    w = safe_npz(
        case / "wrist-trace.npz",
        dict(
            measured_m=(2000, 3),
            start_m=(3,),
            target_m=(3,),
            balance=(2000, 5),
            initial_root_quat=(4,),
            dt=(),
        ),
    )
    a = safe_npz(
        case / "motor-work.npz",
        dict(
            joints=(29,),
            q_before=(2000, 29),
            q_after=(2000, 29),
            target=(2000, 29),
            velocity_before=(2000, 29),
            kp=(29,),
            kd=(29,),
            effort_limit=(29,),
            dt=(),
        ),
    )
    names = a["joints"].tolist()
    if len(set(names)) != 29 or float(a["dt"]) != 0.005 or float(w["dt"]) != 0.005:
        raise ValueError("Unexpected G1 joints or physical clock")
    xml = ET.parse(MODEL)
    limits = [xml.find(f"joint[@name='{n}']/limit") for n in names]
    lo, hi, speed, effort = np.array(
        [
            [float(x.get(k)) for k in ("lower", "upper", "velocity", "effort")]
            for x in limits
        ]
    ).T
    if not np.array_equal(a["effort_limit"], effort):
        raise ValueError("G1 motor-force configuration differs from URDF")
    chain = load_urdf_chain(
        MODEL, "pelvis", plan["joint"].split("_", 1)[0] + "_wrist_yaw_link"
    )
    q = a["q_before"][0, [names.index(n) for n in chain.joint_names]].copy()
    start = chain.forward(q)[0]
    q[chain.joint_names.index(plan["joint"])] += plan["delta_rad"]
    target = w["start_m"] + quat_wxyz_to_matrix(w["initial_root_quat"]) @ (
        chain.forward(q)[0] - start
    )
    if not np.allclose(target, w["target_m"], atol=1e-7, rtol=0):
        raise ValueError("G1 world-frame wrist target differs from robot kinematics")
    wrist_error = np.linalg.norm(w["measured_m"] - target, axis=1)
    feet, other, tilt, drift, angle = w["balance"].T
    if not np.isin(feet, [0, 1]).all() or not np.isin(other, [0, 1]).all():
        raise ValueError("Invalid G1 ground-contact samples")
    contacts = read(case / "impact-contacts.json")
    if any(
        not 601 <= c["step"] <= 2000
        or not np.isfinite(c["normal_impulse_ns"])
        or c["normal_impulse_ns"] <= 0
        for c in contacts
    ):
        raise ValueError("Invalid G1 block contact")
    last = max((c["step"] for c in contacts), default=2000)
    stable = (
        (feet == 1)
        & (other == 0)
        & (tilt < np.deg2rad(4))
        & (drift < 0.03)
        & (wrist_error <= 0.010)
        & (abs(angle - plan["delta_rad"]) < 0.015)
    )
    stable_run = 0
    recovery = None
    for step, ok in enumerate(stable[last:], start=last + 1):
        stable_run = stable_run + 1 if ok else 0
        if stable_run == 100:
            recovery = (step - last) * 0.005
            break
    targets = a["target"]
    bypasses = int(
        np.count_nonzero((targets < lo - 1e-6) | (targets > hi + 1e-6))
    ) + int(np.count_nonzero(abs(np.diff(targets, axis=0)) > speed * 0.005 + 1e-6))
    if "reach_checkpoint" in plan:
        from g1_reach_checkpoint import features, load

        weights = load(case, plan)
        if (
            result.get("reach_checkpoint") != plan["reach_checkpoint"]
            or result.get("training_dataset") != plan["training_dataset"]
        ):
            raise ValueError("G1 measured trial has different checkpoint lineage")
        j = names.index(plan["joint"])
        initial = a["q_before"][0, j]
        phase = np.minimum((np.arange(2000) + 1) * 0.005, 1)
        ref = initial + phase * plan["delta_rad"]
        corrections = np.array(
            [
                weights @ features(t, e)
                for t, e in zip(phase, ref - a["q_before"][:, j], strict=True)
            ]
        )
        goal = np.clip(ref + np.clip(corrections, -0.04, 0.04), lo[j], hi[j])
        previous = np.r_[0.0, targets[:-1, j]]
        expected = previous + np.clip(
            goal - previous, -speed[j] * 0.005, speed[j] * 0.005
        )
        if not np.allclose(expected, targets[:, j], atol=2e-7, rtol=0):
            raise ValueError("Recorded motor targets differ from checkpoint inference")
    torque = np.clip(
        a["kp"] * (targets - a["q_before"]) - a["kd"] * a["velocity_before"],
        -effort,
        effort,
    )
    work = float(np.maximum(torque * (a["q_after"] - a["q_before"]), 0).sum())
    tracking = abs(angle[-1] - plan["delta_rad"])
    if not np.isclose(
        work, result["estimated_positive_joint_work_j"], atol=1e-7
    ) or not np.isclose(angle[-1], result["measured_delta"]):
        raise ValueError("G1 work or shoulder readback differs")
    conditions = {
        "Both feet must carry load at every step": np.all(feet == 1),
        "Unexpected body-floor contact": not other.any(),
        "Wrist did not settle within 10 mm": wrist_error[-100:].max() <= 0.010,
        "Requested wrist travel below 15 mm": np.linalg.norm(target - w["start_m"])
        >= 0.015,
        "Shoulder angle target not tracked": tracking
        < min(0.025, 0.15 * abs(plan["delta_rad"])),
        "Missing block contact or stable two-second recovery": recovery is not None
        and recovery <= 2,
        "Tilt or pelvis drift exceeded envelope": tilt.max() < np.deg2rad(12)
        and drift.max() < 0.10,
        "Measured joints exceeded envelope": np.maximum(
            lo - a["q_after"], a["q_after"] - hi
        ).max()
        <= 0.005,
        "Actuator target exceeded position or rate limit": bypasses == 0,
    }
    reasons = [name for name, ok in conditions.items() if not ok]
    if bool(result["task_success"]) != (not reasons):
        raise ValueError("Stored G1 verdict differs from the trace")
    return dict(
        passed=not reasons,
        verified=True,
        reasons=reasons,
        loaded_steps=int(feet.sum()),
        impacts=len(contacts),
        wrist_error_mm=float(wrist_error[-100:].max() * 1000),
        recovery_s=recovery,
        positive_work_j=work,
        guard_bypasses=bypasses,
    )


def summarize(directory, expected, openings):
    m = manifest(directory, expected)
    if (
        len(openings) != m["cases"]
        or [commitment(x) for x in openings] != m["commitments"]
        or digest(MODEL) != m["inputs"]["urdf"]
    ):
        raise ValueError("G1 openings, assets or count differ")
    rows = []
    for i, opening in enumerate(openings):
        case = directory / f"case-{i:03}"
        if (case / "result.json").is_file():
            row = score(case, m, opening)
        else:
            row = dict(
                passed=False,
                verified=False,
                reasons=["Missing completed physics result"],
                guard_bypasses=0,
            )
        rows.append(dict(index=i, **opening, **row))
    return dict(
        profile=m["profile"],
        manifest_sha256=expected,
        rows=rows,
        cases=len(rows),
        successes=sum(r["passed"] for r in rows),
        verified=sum(r["verified"] for r in rows),
        verified_guard_bypasses=sum(r["guard_bypasses"] for r in rows),
        qualified=all(r["passed"] for r in rows),
    )


def run(directory, expected, jobs):
    m = manifest(directory, expected)
    openings = read(directory / "openings.json")
    if m["inputs"] != inputs() or [commitment(x) for x in openings] != m["commitments"]:
        raise ValueError("G1 environment or sealed cases differ")
    with (directory / "run.lock").open("x"):

        def launch(item):
            i, opening = item
            case = directory / f"case-{i:03}"
            recipe = directory / f"impact-{i:03}.json"
            save(recipe, opening["recipe"])
            with (directory / f"case-{i:03}.log").open("xb") as log:
                subprocess.run(
                    [
                        sys.executable,
                        str(HERE / "worker.py"),
                        "run",
                        "--robot",
                        "G1",
                        "--fixture",
                        "impact",
                        "--controller",
                        str(directory / "controller.json"),
                        "--impact-recipe",
                        str(recipe),
                        "--out",
                        str(case),
                    ],
                    stdout=log,
                    stderr=subprocess.STDOUT,
                )
            if case.is_dir():
                (case / "impact.json").write_bytes(recipe.read_bytes())

        with ThreadPoolExecutor(max_workers=jobs) as pool:
            list(pool.map(launch, enumerate(openings)))
    report = summarize(directory, expected, openings)
    save(directory / "report.json", report)
    return report


def pack(directory, archive):
    report = read(directory / "report.json")
    with ZipFile(archive, "x", ZIP_DEFLATED) as z:
        for n in ("suite.json", "controller.json", "report.json"):
            z.write(directory / n, n)
        for i in range(report["cases"]):
            for n in FILES:
                p = directory / f"case-{i:03}" / n
                if p.is_file():
                    z.write(p, str(p.relative_to(directory)))


def verify(archive, expected):
    with TemporaryDirectory() as scratch, ZipFile(archive) as z:
        directory = Path(scratch)
        names = z.namelist()
        allowed = {"suite.json", "controller.json", "report.json"} | {
            f"case-{i:03}/{n}" for i in range(20) for n in FILES
        }
        if (
            len(set(names)) != len(names)
            or not set(names) <= allowed
            or any(x.file_size > 3_000_000 for x in z.infolist())
        ):
            raise ValueError("Unexpected G1 evidence archive contents")
        for name in names:
            p = directory / name
            p.parent.mkdir(exist_ok=True)
            p.write_bytes(z.read(name))
        read_matched = "matched_manifest_sha256" in read(directory / "suite.json")
        reported = read(directory / "report.json")
        openings = [{k: r[k] for k in ("salt", "recipe")} for r in reported["rows"]]
        actual = summarize(directory, expected, openings)
        if reported != actual:
            raise ValueError("Recomputed G1 results differ")
    rows = actual.pop("rows")
    return dict(
        **actual,
        split="matched" if read_matched else "bounded-impact",
        requirement=20,
        archive_sha256=digest(archive),
        failures=[r for r in rows if not r["passed"]],
    )


def main():
    p = argparse.ArgumentParser(description=__doc__)
    sub = p.add_subparsers(dest="operation", required=True)
    prep = sub.add_parser("prepare")
    prep.add_argument("directory", type=Path)
    prep.add_argument("controller", type=Path)
    prep.add_argument(
        "--match",
        type=Path,
        help="Use the disclosed recipes from a prior suite and report",
    )
    trial = sub.add_parser("run")
    trial.add_argument("directory", type=Path)
    trial.add_argument("manifest")
    trial.add_argument("--jobs", type=int, choices=range(1, 9), default=2)
    bundler = sub.add_parser("pack")
    bundler.add_argument("directory", type=Path)
    bundler.add_argument("archive", type=Path)
    audit = sub.add_parser("verify")
    audit.add_argument("archive", type=Path)
    audit.add_argument("manifest")
    args = p.parse_args()
    if args.operation == "prepare":
        print(prepare(args.directory, args.controller, args.match))
    elif args.operation == "run":
        result = run(args.directory.resolve(), args.manifest, args.jobs)
        print(json.dumps({k: v for k, v in result.items() if k != "rows"}))
        sys.exit(0 if result["qualified"] else 1)
    elif args.operation == "pack":
        pack(args.directory, args.archive)
    else:
        print(json.dumps(verify(args.archive, args.manifest), indent=2))


if __name__ == "__main__":
    main()
