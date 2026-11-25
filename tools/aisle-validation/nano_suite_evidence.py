"""Archive or independently open a disclosed Nano holdout and replay its motor records."""

import argparse
from io import BytesIO
import math
import json
from pathlib import Path
from shutil import copyfileobj
from tempfile import NamedTemporaryFile, TemporaryDirectory
from zipfile import ZIP_DEFLATED, ZipFile

import numpy as np

from nano_transfer_suite import MODEL, read, signature, summarize, worker

HEADER = ("suite.json", "controller.json", "report.json")
EVIDENCE = (
    "result.json",
    "source.json",
    "calibration.json",
    "motor-targets.f64",
    "actions.jsonl",
    "controller.json",
    "phases.json",
    "phase-measurements.jsonl",
    "policy.log",
    "wrist-delivery.jsonl",
)


def pack(directory, output):
    report = read(directory / "report.json")
    if len(report["rows"]) != report["cases"]:
        raise ValueError("Archive requires a completed cohort")
    with ZipFile(output, "x", ZIP_DEFLATED, compresslevel=6) as archive:
        for name in HEADER:
            archive.write(directory / name, name)
        for i in range(report["cases"]):
            stem = f"case-{i:03}"
            files = [directory / (stem + ".log")]
            case = directory / stem
            files.extend(case / name for name in EVIDENCE)
            if case.is_dir():
                files.extend(sorted((case / "release-observations").glob("*.npz")))
                for camera in ("head", "left_wrist", "right_wrist"):
                    frames = sorted(case.glob(f"[0-9][0-9]-*-{camera}.png"))
                    if frames:
                        files.append(frames[-1])
            for path in files:
                if path.is_file():
                    archive.write(path, str(path.relative_to(directory)))
    digest = worker.digest(output)
    with NamedTemporaryFile(mode="w", dir=output.parent, delete=False) as receipt:
        receipt.write(digest + "\n")
    Path(receipt.name).replace(output.with_suffix(output.suffix + ".sha256"))
    return digest


def verify_release_packet(raw, calibration, action):
    """Check the on-board packet against the journal and calibrated sensor shapes."""
    shapes = {"stamp_ns": (), "encoders": (len(calibration["joint_names"]),)}
    for camera in ("head", "left_wrist", "right_wrist"):
        w, h = calibration["cameras"][camera]["resolution"]
        shapes[camera] = (h, w, 3)
    with ZipFile(BytesIO(raw)) as packet:
        if (
            sorted(packet.namelist()) != sorted(name + ".npy" for name in shapes)
            or sum(member.file_size for member in packet.infolist()) > 12_000_000
        ):
            raise ValueError("Unexpected release sensor channels or packet size")
        for name, expected_shape in shapes.items():
            info = packet.getinfo(name + ".npy")
            with packet.open(info) as member:
                if np.lib.format.read_magic(member) != (1, 0):
                    raise ValueError("Unexpected release sensor array format")
                shape, _, dtype = np.lib.format.read_array_header_1_0(member)
                types = {"stamp_ns": ("int64",), "encoders": ("float32", "float64")}
                size = math.prod(expected_shape) * dtype.itemsize
                if (
                    shape != expected_shape
                    or dtype.name not in types.get(name, ("uint8",))
                    or info.file_size != member.tell() + size
                ):
                    raise ValueError("Release sensor array disagrees with calibration")
                payload = member.read(size)
                if len(payload) != size:
                    raise ValueError("Incomplete release sensor data")
                if name == "stamp_ns":
                    stamp = np.frombuffer(payload, dtype=dtype).item()
                    if (
                        stamp != action["stamp_ns"]
                        or stamp != action["tick"] * 10_000_000
                    ):
                        raise ValueError(
                            "Release exposure timestamp differs from its action"
                        )
                if (
                    name == "encoders"
                    and not np.isfinite(np.frombuffer(payload, dtype=dtype)).all()
                ):
                    raise ValueError("Release encoder sample is nonfinite")


def verify(
    archive_path,
    expected_manifest,
    include_trials=False,
    *,
    comparison=False,
    expected_archive=None,
):
    with ZipFile(archive_path) as archive, TemporaryDirectory() as name:
        tmp = Path(name)
        names = set(archive.namelist())
        if len(names) != len(archive.infolist()):
            raise ValueError("Evaluation archive has duplicate file names")
        captures = {key for key in names if "/release-observations/" in key}
        receipt = archive_path.with_suffix(archive_path.suffix + ".sha256")
        pinned = read(Path(__file__).with_name("nano_published_outputs.json")).get(
            expected_manifest
        )
        integrity = "Operator-supplied archive digest"
        if pinned is not None:
            if expected_archive is not None and expected_archive != pinned:
                raise ValueError(
                    "Archive SHA-256 differs from the published output receipt"
                )
            expected_archive = pinned
            integrity = "Published Studio output digest"
        elif expected_archive is None and receipt.is_file():
            expected_archive = receipt.read_text().strip()
            integrity = "Local output receipt"
        if captures and not expected_archive:
            raise ValueError(
                "A trusted archive SHA-256 is required for the recorded release exposures"
            )
        archive_digest = None
        if expected_archive is not None:
            archive_digest = worker.digest(archive_path)
            if archive_digest != expected_archive:
                raise ValueError(
                    "Archive SHA-256 differs from the published output receipt"
                )
        marked_captures = set()
        expanded = 0

        def copy_member(key, output, limit):
            nonlocal expanded
            info = archive.getinfo(key)
            expanded += info.file_size
            if info.file_size > limit or expanded > 1024**3:
                raise ValueError(
                    "Evaluation record exceeds the batch evidence size limit"
                )
            with archive.open(info) as src, output.open("xb") as dest:
                copyfileobj(src, dest, 128 * 1024)

        for file in HEADER:
            copy_member(file, tmp / file, 8 * 1024**2)
        manifest, reported = read(tmp / "suite.json"), read(tmp / "report.json")
        cases = manifest["cases"]
        if (
            worker.digest(tmp / "suite.json") != expected_manifest
            or reported["manifest_sha256"] != expected_manifest
            or worker.digest(tmp / "controller.json") != manifest["controller_sha256"]
            or worker.digest(MODEL) != manifest["inputs"]["urdf"]
            or not isinstance(cases, int)
            or cases < 1
            or cases != len(reported["rows"])
            or cases != len(manifest["commitments"])
        ):
            raise ValueError("Manifest, controller or case count differs")
        members = []
        for i, row in enumerate(reported["rows"]):
            member = {k: row[k] for k in ("seed", "salt")}
            if i != row["index"] or signature(member) != manifest["commitments"][i]:
                raise ValueError(f"Layout commitment differs at {i}")
            members.append(member)
            case = tmp / f"case-{i:03}"
            case.mkdir()
            for file in EVIDENCE[:6]:
                key = f"{case.name}/{file}"
                if key in names:
                    copy_member(key, case / file, 16 * 1024**2)
            journal = case / "actions.jsonl"
            if journal.is_file():
                with journal.open("rb") as actions:
                    for line in actions:
                        if b'"release observation"' in line:
                            action = json.loads(line)
                            if action.get("op") == "release observation":
                                key = f"{case.name}/release-observations/{action['tick']:06}.npz"
                                if (
                                    key not in captures
                                    or archive.getinfo(key).file_size > 12_000_000
                                ):
                                    raise ValueError(
                                        "Missing or oversized release exposure"
                                    )
                                expanded += archive.getinfo(key).file_size
                                if expanded > 1024**3:
                                    raise ValueError(
                                        "Release records exceed the batch evidence size limit"
                                    )
                                verify_release_packet(
                                    archive.read(key),
                                    read(case / "calibration.json"),
                                    action,
                                )
                                marked_captures.add(key)
        if captures != marked_captures:
            raise ValueError(
                "Release exposure files differ from the marked action timestamps"
            )
        summary = summarize(tmp, expected_manifest, manifest, members)
        replayed = read(tmp / "report.json")
        if manifest["split"] == "development":
            # The first development cohort predated these two ancillary file digests.
            for actual, original in zip(
                replayed["rows"], reported["rows"], strict=True
            ):
                for field in ("calibration_sha256", "source_sha256"):
                    if field not in original:
                        actual.pop(field, None)
        if replayed != reported:
            raise ValueError("Independent per-case or cohort score differs")
        if captures:
            summary["release_exposures"] = len(captures)
            summary["archive_integrity"] = integrity
        if include_trials:
            n = summary["cases"]
            p = summary["successes"] / n
            z = 1.959963984540054
            denom = 1 + z * z / n
            center = (p + z * z / (2 * n)) / denom
            radius = z * (p * (1 - p) / n + z * z / (4 * n * n)) ** 0.5 / denom
            failures = []
            for row in reported["rows"]:
                if row["passed"]:
                    continue
                reasons = []
                if row.get("error") or row.get("issue"):
                    reasons.append(row.get("error") or row["issue"])
                if row.get("verified"):
                    measured = read(tmp / f"case-{row['index']:03}" / "result.json")
                    unsupported = (
                        measured["physics_ticks"] - measured["wheel_supported_ticks"]
                    )
                    if unsupported:
                        reasons.append(
                            f"No loaded wheel contact for {unsupported} physics steps"
                        )
                    if measured["max_joint_excursion_rad"] > 0.005:
                        reasons.append(
                            f"Joint excursion {measured['max_joint_excursion_rad']:.4f} rad"
                        )
                    if row["guard_bypasses"] or row["rejected_motor_writes"]:
                        reasons.append(
                            f"{row['guard_bypasses']} guard bypasses · {row['rejected_motor_writes']} rejected writes"
                        )
                    for color, cube in row["cubes"].items():
                        if cube["passed"]:
                            continue
                        criteria = []
                        if cube["opposing_finger_airborne_ticks"] < 20:
                            criteria.append("opposing-finger lift incomplete")
                        if cube["table_contact_tail_ticks"] < 100:
                            criteria.append(
                                f"{cube['table_contact_tail_ticks']}/100 table contacts"
                            )
                        if cube["nearest_table_edge_m"] <= 0.035:
                            criteria.append(
                                f"{1000 * cube['nearest_table_edge_m']:.1f} mm rim margin"
                            )
                        if (
                            not cube["detached"]
                            or cube["min_finger_clearance_tail_m"] <= 0.001
                        ):
                            criteria.append("gripper release incomplete")
                        height = cube["final_position_m"][2] - measured["table_top_m"]
                        if not 0.015 <= height <= 0.037:
                            criteria.append(
                                f"center {1000 * height:.1f} mm above tabletop"
                            )
                        if cube["settle_motion_m"] >= 0.003:
                            criteria.append("still moving")
                        reasons.append(color + " · " + ", ".join(criteria))
                for pair, ticks in row.get("robot_furniture_contacts", {}).items():
                    reasons.append(f"{pair} · {ticks} contact steps")
                failures.append(
                    dict(
                        index=row["index"],
                        seed=row["seed"],
                        reasons=reasons or ["Trial criteria failed"],
                    )
                )
            summary.update(
                interval_95=[max(0.0, center - radius), min(1.0, center + radius)],
                failures=failures,
                archive_sha256=archive_digest or worker.digest(archive_path),
                requirement=manifest["success_requirement"],
            )
        if comparison:
            summary.update(
                controller_sha256=manifest["controller_sha256"],
                color_order=manifest["color_order"],
                inputs=manifest["inputs"],
                rows=replayed["rows"],
                archive_sha256=archive_digest or worker.digest(archive_path),
            )
        return summary


def compare_archives(
    baseline, candidate, baseline_manifest, candidate_manifest, *, include_trials=False
):
    before = verify(baseline, baseline_manifest, include_trials, comparison=True)
    after = verify(candidate, candidate_manifest, include_trials, comparison=True)
    if before["color_order"] != after["color_order"] or any(
        before["inputs"][k] != after["inputs"][k]
        for k in ("urdf", "mesh_inventory_sha256", "recipe")
    ):
        raise ValueError(
            "Select Nano suites with the same robot, scene recipe and requested cubes"
        )
    old = {r["seed"]: r for r in before.pop("rows")}
    new = {r["seed"]: r for r in after.pop("rows")}
    if (
        old.keys() != new.keys()
        or len(old) != before["cases"]
        or len(new) != after["cases"]
    ):
        raise ValueError(
            "Each archive must contain the same set of distinct layout seeds"
        )
    pairs = []
    for seed, left in old.items():
        right = new[seed]
        has_receipts = left["verified"] and right["verified"]
        fields = {
            "outcome": "passed",
            "motor": "motor_sha256",
            "actions": "actions_sha256",
            "ticks": "physics_ticks",
        }
        matches = {
            key: bool(
                has_receipts
                and left.get(field) is not None
                and left[field] == right.get(field)
            )
            for key, field in fields.items()
        }
        pairs.append(
            dict(
                seed=seed,
                baseline_index=left["index"],
                candidate_index=right["index"],
                both_verified=has_receipts,
                baseline_passed=left["passed"],
                candidate_passed=right["passed"],
                same=matches,
                baseline_ticks=left.get("physics_ticks"),
                candidate_ticks=right.get("physics_ticks"),
                baseline_motor_sha256=left.get("motor_sha256"),
                candidate_motor_sha256=right.get("motor_sha256"),
            )
        )
    return dict(
        baseline=before,
        candidate=after,
        cases=len(pairs),
        both_verified=sum(p["both_verified"] for p in pairs),
        same={
            field: sum(p["same"][field] for p in pairs)
            for field in ("outcome", "motor", "actions", "ticks")
        },
        improved=sum(
            p["both_verified"] and p["candidate_passed"] and not p["baseline_passed"]
            for p in pairs
        ),
        regressed=sum(
            p["both_verified"] and p["baseline_passed"] and not p["candidate_passed"]
            for p in pairs
        ),
        pairs=pairs,
    )


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    commands = cli.add_subparsers(dest="op", required=True)
    create = commands.add_parser("pack")
    create.add_argument("suite", type=Path)
    create.add_argument("archive", type=Path)
    audit = commands.add_parser("verify")
    audit.add_argument("archive", type=Path)
    audit.add_argument("--manifest-sha256", required=True)
    audit.add_argument(
        "--archive-sha256",
        help="Trusted digest of the completed archive including camera captures",
    )
    pair = commands.add_parser(
        "compare", help="Independently replay and pair two Nano archives by seed"
    )
    pair.add_argument("baseline", type=Path)
    pair.add_argument("candidate", type=Path)
    pair.add_argument("--baseline-manifest", required=True)
    pair.add_argument("--candidate-manifest", required=True)
    args = cli.parse_args()
    if args.op == "pack":
        print(pack(args.suite, args.archive))
    elif args.op == "compare":
        print(
            json.dumps(
                compare_archives(
                    args.baseline,
                    args.candidate,
                    args.baseline_manifest,
                    args.candidate_manifest,
                ),
                indent=2,
            )
        )
    else:
        print(
            json.dumps(
                verify(
                    args.archive,
                    args.manifest_sha256,
                    expected_archive=args.archive_sha256,
                ),
                indent=2,
            )
        )


if __name__ == "__main__":
    main()
