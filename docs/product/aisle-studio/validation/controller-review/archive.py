"""Retain the controller review receipts and the original Nexus trials from native verification."""

import argparse
import hashlib
import json
import shutil
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile


def read(path):
    return json.loads(path.read_bytes())


def archive(args):
    out = args.out
    out.mkdir(parents=True, exist_ok=True)
    roots = {"g1": args.handoff, "independent": args.selection, "nano": args.pickup}
    cases = (
        ("g1", "g1-handoff-changes"),
        ("independent", "G1-earlier-changes"),
        ("independent", "Nano-earlier-changes"),
        ("nano", "nano-pickup-changes"),
    )
    summary = {}
    with ZipFile(out / "trials.zip", "w", ZIP_DEFLATED) as bundle:
        for folder, case in cases:
            root = roots[folder]
            verified = root / "verified"
            report = read(verified / (case + ".json"))
            entry = dict(
                robot=report["robot"],
                relation=report["relation"],
                display=report["display"],
                outcomes={},
            )
            bundle.write(verified / (case + ".json"), case + "/comparison.json")
            for side in ("before", "after"):
                sha = report[side]["sha256"]
                (rundir,) = (
                    p.parent
                    for p in sorted((root / "state/aisle").glob("*-run/result.json"))
                    if read(p).get("controller_hash") == sha
                )
                result = read(rundir / "result.json")
                assert (
                    result["controller_hash"]
                    == hashlib.sha256(
                        (rundir / "controller.json").read_bytes()
                    ).hexdigest()
                )
                for name in (
                    "controller.json",
                    "result.json",
                    "source.json",
                    "trajectory.json",
                ):
                    bundle.write(rundir / name, f"{case}/{side}/{name}")
                if (rundir / "motor-targets.f64").exists():
                    bundle.write(
                        rundir / "motor-targets.f64", f"{case}/{side}/motor-targets.f64"
                    )
                entry["outcomes"][side] = {
                    k: result[k]
                    for k in [
                        "kind",
                        "task_success",
                        "requested_delta",
                        "measured_delta",
                        "phase",
                        "frames",
                        "controller_hash",
                    ]
                    if k in result
                }
            summary[case] = entry
        for name, folder in (
            ("g1-handoff-changes", "g1"),
            ("nano-pickup-changes", "nano"),
        ):
            shutil.copyfile(
                roots[folder] / "verified" / (name + ".png"), out / (name + ".png")
            )
        bundle.write(
            roots["g1"] / "verified/g1-restored-changes.json", "restored-g1.json"
        )
        bundle.write(args.unavailable / "checks.json", "unavailable.json")
        for folder in ("g1", "nano", "independent"):
            bundle.write(
                roots[folder] / "verified/recovery.json", folder + "-recovery.json"
            )
            manifest = read(roots[folder] / "verified/mode-manifest.json")
            summary[folder] = {
                "backend": manifest["backend"],
                "size": manifest["requested_size"],
                "studio_sha256": manifest["native_binary_sha256"],
            }
    (out / "results.json").write_text(json.dumps(summary, indent=2) + "\n")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    for flag in ("handoff", "selection", "pickup", "unavailable", "out"):
        parser.add_argument("--" + flag, type=Path, required=True)
    archive(parser.parse_args())
