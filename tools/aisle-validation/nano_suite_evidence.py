"""Archive or independently open a disclosed Nano holdout and replay its motor records."""

import argparse
from pathlib import Path
from shutil import copyfileobj
from tempfile import TemporaryDirectory
from zipfile import ZIP_DEFLATED, ZipFile

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
                for camera in ("head", "left_wrist", "right_wrist"):
                    frames = sorted(case.glob(f"[0-9][0-9]-*-{camera}.png"))
                    if frames:
                        files.append(frames[-1])
            for path in files:
                if path.is_file():
                    archive.write(path, str(path.relative_to(directory)))
    return worker.digest(output)


def verify(archive_path, expected_manifest, include_trials=False):
    with ZipFile(archive_path) as archive, TemporaryDirectory() as name:
        tmp = Path(name)
        names = set(archive.namelist())
        if len(names) != len(archive.infolist()):
            raise ValueError("Evaluation archive has duplicate file names")
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
                for color, cube in row.get("cubes", {}).items():
                    if not cube["passed"]:
                        reasons.append(
                            f"{color} incomplete · {cube['table_contact_tail_ticks']}/100 table contacts"
                        )
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
                archive_sha256=worker.digest(archive_path),
                requirement=manifest["success_requirement"],
            )
        return summary


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    commands = cli.add_subparsers(dest="op", required=True)
    create = commands.add_parser("pack")
    create.add_argument("suite", type=Path)
    create.add_argument("archive", type=Path)
    audit = commands.add_parser("verify")
    audit.add_argument("archive", type=Path)
    audit.add_argument("--manifest-sha256", required=True)
    args = cli.parse_args()
    if args.op == "pack":
        print(pack(args.suite, args.archive))
    else:
        import json

        print(json.dumps(verify(args.archive, args.manifest_sha256), indent=2))


if __name__ == "__main__":
    main()
