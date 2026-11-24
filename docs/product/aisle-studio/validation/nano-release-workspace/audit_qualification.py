"""Replay a published Nano qualification and print its full case-level verdict."""

import argparse
from hashlib import sha256
import json
from pathlib import Path
import subprocess
import sys
from zipfile import ZipFile

ROOT = Path(__file__).resolve().parents[5]
sys.path.insert(0, str(ROOT / "tools/aisle-validation"))
from nano_suite_evidence import verify


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("cohort", type=Path)
    parser.add_argument(
        "--check",
        action="store_true",
        help="Compare decoded results with verification.json",
    )
    args = parser.parse_args()
    cohort = args.cohort.resolve()
    source = json.loads((cohort / "provenance.json").read_text())

    def git_file(revision, name):
        return subprocess.check_output(
            ["git", "-C", str(ROOT), "show", f"{revision}:{name}"]
        )

    public = (cohort / "suite.json").read_bytes()
    if (
        git_file(source["registration_revision"], source["registered_manifest"])
        != public
    ):
        raise ValueError("Input manifest differs from its original Git commitment")
    plan = json.loads(public)
    for name, expected in plan["inputs"]["studio_files"].items():
        if sha256(git_file(source["runtime_revision"], name)).hexdigest() != expected:
            raise ValueError(f"Runtime Git revision differs at {name}")
    if (
        plan["inputs"]["studio_files"][
            "tools/aisle-validation/nano_rgb_transfer_policy.py"
        ]
        != source["policy_sha256"]
    ):
        raise ValueError("Unexpected frozen policy digest")
    with ZipFile(cohort / "evidence.zip") as archive:
        controller = sha256(archive.read("controller.json")).hexdigest()
    if (
        controller != source["controller_sha256"]
        or controller != plan["controller_sha256"]
    ):
        raise ValueError("Scored controller differs from the preregistered program")
    manifest = sha256((cohort / "suite.json").read_bytes()).hexdigest()
    catalog = json.loads(
        (ROOT / "tools/aisle-validation/nano_published_outputs.json").read_text()
    )
    receipt = (cohort / "evidence.zip.sha256").read_text().strip()
    if catalog.get(manifest) != receipt:
        raise ValueError("Output receipt differs from the published qualification")
    result = verify(
        cohort / "evidence.zip", manifest, include_trials=True, expected_archive=receipt
    )
    if args.check and json.loads((cohort / "verification.json").read_text()) != result:
        raise ValueError("Replayed results differ from the published measurements")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
