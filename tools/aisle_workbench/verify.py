"""Launch a fresh native Studio, exercise the pages and verify evidence after restart."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--studio", type=Path, required=True)
    parser.add_argument("--remote", default="makepad-remote")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument(
        "--backend", choices=["headless", "offscreen"], default="headless"
    )
    parser.add_argument("--size", choices=["1024x768", "1440x900"], default="1440x900")
    parser.add_argument("--dark", action="store_true")
    models = parser.add_mutually_exclusive_group()
    models.add_argument("--codex", dest="provider", action="store_const", const="codex")
    models.add_argument(
        "--octos-deepseek",
        dest="provider",
        action="store_const",
        const="octos-deepseek",
    )
    scenario = parser.add_mutually_exclusive_group()
    scenario.add_argument(
        "--standing", action="store_true", help="Run a floating-base G1 tracking trial"
    )
    scenario.add_argument(
        "--transfer",
        choices=["red", "three"],
        help="Run and score a Nano floor-to-table journey",
    )
    scenario.add_argument(
        "--observed-author",
        action="store_true",
        help="Select a target from two actual onboard scene observations",
    )
    scenario.add_argument(
        "--camera-faults",
        action="store_true",
        help="Drive the same Nano controller with normal, delayed and frozen RGB delivery",
    )
    scenario.add_argument(
        "--handoff",
        action="store_true",
        help="Revise a measured Codex trial using DeepSeek via Octos",
    )
    scenario.add_argument(
        "--evaluation",
        type=Path,
        help="Verify a batch archive in Evidence and reopen it after restart",
    )
    parser.add_argument("--layout", type=int, choices=[0, 1, 2], default=0)
    parser.add_argument(
        "--impact", action="store_true", help="Select the G1 development impact fixture"
    )
    parser.add_argument(
        "--survey",
        type=Path,
        help="Import and edit the Nano reference survey during a transfer",
    )
    args = parser.parse_args()
    if args.survey and not args.transfer:
        parser.error("--survey requires --transfer")
    if args.impact and not args.standing:
        parser.error("--impact requires --standing")
    if args.layout and not args.transfer:
        parser.error("--layout requires a Nano transfer scenario")
    if (
        args.standing or args.transfer or args.camera_faults or args.observed_author
    ) and not args.provider:
        parser.error("Select --codex or --octos-deepseek to author the requested trial")
    output = args.out.resolve()
    output.mkdir(parents=True, exist_ok=False)
    report = output / "ui.json"
    studio_root = Path(__file__).resolve().parents[2]
    env = os.environ | {
        "MAKEPAD_HAL_BACKEND": args.backend,
        "DOROBOT_UX_REPORT": str(report),
        "DOROBOT_STUDIO_SOURCE_ROOT": str(studio_root),
        "DOROBOT_STUDIO_STATE_DIR": str(output / "state"),
    }
    # The binary can come from another build directory. Its digest identifies the
    # executable, while these fields identify the workspace observed at launch.
    with args.studio.open("rb") as binary:
        binary_hash = hashlib.file_digest(binary, "sha256").hexdigest()
    revision = subprocess.check_output(
        ["git", "-C", str(studio_root), "rev-parse", "HEAD"], text=True
    ).strip()
    changed = subprocess.check_output(
        ["git", "-C", str(studio_root), "status", "--porcelain"], text=True
    ).splitlines()
    source_hashes = {
        str(path.relative_to(studio_root)): hashlib.sha256(
            path.read_bytes()
        ).hexdigest()
        for path in sorted(
            [
                *Path(__file__).resolve().parent.glob("*.py"),
                *(studio_root / "tools/aisle-validation").glob("*.py"),
                *(studio_root / "tools/nano-three-cubes").glob("*.py"),
                *(
                    studio_root / f
                    for f in (
                        "Cargo.lock",
                        "crates/dorobot-studio-core/src/runtime.rs",
                        "plugins/aisle/src/lib.rs",
                        "crates/dorobot-studio-core/src/api/aisle.rs",
                        "dorobot-flex/src/bin/dorobot-flex.rs",
                    )
                ),
            ]
        )
    }
    sessions = []
    for stage in ["journey", "recovery"]:
        with args.studio.open("rb") as binary:
            if hashlib.file_digest(binary, "sha256").hexdigest() != binary_hash:
                raise RuntimeError(
                    "Studio executable changed between journey and recovery"
                )
        report.unlink(missing_ok=True)
        command = [str(args.studio.resolve()), "--remote", "--screen", "aisle-task"]
        if args.dark:
            command.append("--dark")
        with (output / f"{stage}.log").open("w") as log:
            process = subprocess.Popen(
                command, env=env, stdout=log, stderr=subprocess.STDOUT
            )
            try:
                deadline = time.monotonic() + 30
                while not report.exists():
                    if process.poll() is not None or time.monotonic() > deadline:
                        raise RuntimeError(f"Studio failed to start. See {log.name}")
                    time.sleep(0.1)
                if (
                    Path(
                        json.loads(report.read_text())["state"].get("aisle_source_root")
                        or ""
                    )
                    != studio_root
                ):
                    raise RuntimeError(
                        "Studio launched against a different source checkout"
                    )
                check = [
                    sys.executable,
                    str(Path(__file__).with_name("journey.py")),
                    "--remote",
                    args.remote,
                    "--pid",
                    str(process.pid),
                    "--report",
                    str(report),
                    "--out",
                    str(output / "verified"),
                    "--size",
                    args.size,
                ]
                if args.observed_author:
                    check.append("--observed-author")
                if args.camera_faults:
                    check.append("--camera-faults")
                if args.handoff:
                    check.append("--handoff")
                if args.evaluation:
                    check.extend(["--evaluation", str(args.evaluation.resolve())])
                if args.standing:
                    check.append("--standing")
                    if args.impact:
                        check.append("--impact")
                if args.survey:
                    check.extend(["--survey", str(args.survey.resolve())])
                if args.transfer:
                    check.extend(
                        ["--transfer", args.transfer, "--layout", str(args.layout)]
                    )
                if stage == "recovery":
                    check.append("--recover")
                else:
                    if args.provider:
                        check.append("--" + args.provider)
                    if args.backend == "offscreen":
                        check.append("--screenshots")
                subprocess.run(check, check=True, timeout=900 if args.transfer else 330)
                status = subprocess.check_output(
                    [args.remote, "app", str(process.pid), "status"], text=True
                )
                windows = [
                    w
                    for line in status.splitlines()
                    for w in json.loads(line).get("w", [])
                    if w.get("i") == 0
                ]
                state = json.loads(report.read_text())["state"]
                assert state["dark"] == args.dark
                sessions.append(
                    dict(
                        stage=stage,
                        pid=process.pid,
                        observed_dark=state["dark"],
                        observed_source_root=state["aisle_source_root"],
                        windows=[w.get("sz") for w in windows],
                    )
                )
            finally:
                if process.poll() is None:
                    process.terminate()
                process.wait(timeout=15)

    directory = output / "verified"
    manifest = dict(
        backend=args.backend,
        requested_size=args.size,
        provider=(
            "codex+octos-deepseek" if args.handoff else args.provider or "reference"
        ),
        native_binary_sha256=binary_hash,
        studio_source_commit=revision,
        source_files_sha256=source_hashes,
        workspace_changes_at_launch=changed,
        sessions=sessions,
        trials=sorted(p.name for p in (output / "state/aisle").glob("*-run")),
        files={
            p.name: hashlib.sha256(p.read_bytes()).hexdigest()
            for p in sorted(directory.iterdir())
        },
    )
    (directory / "mode-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")


if __name__ == "__main__":
    main()
