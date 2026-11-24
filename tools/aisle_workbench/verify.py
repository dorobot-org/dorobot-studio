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
    parser.add_argument(
        "--compare-runs",
        nargs=2,
        type=Path,
        metavar=("PREVIOUS", "CURRENT"),
        help="Verify replay of the first changed action using two recorded Nano trials",
    )
    parser.add_argument(
        "--foreground-cadence",
        nargs="?",
        type=int,
        const=4,
        choices=range(2, 65),
        metavar="SESSIONS",
        help="Measure native foreground RGB with this many concurrent Nano sessions (default four)",
    )
    parser.add_argument(
        "--device-preflight",
        action="store_true",
        help="Exercise the selected trial’s device preflight and retained report",
    )
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
        "--revise-pickup",
        type=Path,
        metavar="FAILED_TRIAL",
        help="Revise a recorded Nano pickup and run the frozen controller",
    )
    scenario.add_argument(
        "--fleet",
        action="store_true",
        help="Submit concurrent jobs from native Run and fetch one into Evidence",
    )
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
        "--moving-target",
        action="store_true",
        help="Compare still and sliding Nano targets in native Studio",
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
    parser.add_argument(
        "--train-reach",
        action="store_true",
        help="Train and run a correction from the exported G1 episode",
    )
    parser.add_argument(
        "--photo-input",
        type=Path,
        help="Draft and review the reference room from its calibrated photograph",
    )
    parser.add_argument(
        "--upright", action="store_true", help="Request an upright G1 trunk posture"
    )
    parser.add_argument(
        "--train-grasp",
        action="store_true",
        help="Train a Nano wrist detector and run it on a different layout",
    )
    parser.add_argument(
        "--heading",
        nargs="?",
        const=20,
        type=int,
        choices=(20, 30),
        help="Test a bounded left waist turn in degrees",
    )
    parser.add_argument(
        "--pickup-motion",
        choices=["slide-positive", "slide-negative"],
        help="Track and intercept a moving floor cube during Nano transfer",
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
    parser.add_argument(
        "--prepared-pickup",
        type=Path,
        help="Reopen the original native authoring receipt without a second model call",
    )
    parser.add_argument(
        "--fleet-node",
        help="Run the two-job native fleet check against this registered worker",
    )
    parser.add_argument(
        "--resume-fleet",
        type=Path,
        metavar="EARLIER_OUTPUT",
        help="Reconnect a prior two-job fleet check and fetch its completed foreground",
    )
    args = parser.parse_args()
    if args.resume_fleet and (not args.fleet_node or not args.fleet):
        parser.error(
            "--resume-fleet needs --fleet and the same registered --fleet-node"
        )
    if args.fleet_node and (not args.fleet or args.foreground_cadence):
        parser.error("--fleet-node requires the two-job --fleet journey")
    if args.photo_input and (args.survey or not args.transfer):
        parser.error("--photo-input requires a transfer without --survey")
    if args.survey and not args.transfer:
        parser.error("--survey requires --transfer")
    if args.train_grasp and not args.transfer:
        parser.error("--train-grasp requires --transfer")
    if args.pickup_motion and not args.transfer:
        parser.error("--pickup-motion requires --transfer")
    if args.device_preflight and not (args.transfer or args.standing):
        parser.error("--device-preflight requires --standing or --transfer")
    if args.train_reach and not args.standing:
        parser.error("--train-reach requires --standing")
    if args.heading and (not args.standing or args.upright):
        parser.error("--heading requires --standing with neutral pitch")
    if args.upright and not args.standing:
        parser.error("--upright requires --standing")
    if args.impact and not args.standing:
        parser.error("--impact requires --standing")
    if args.layout and not args.transfer:
        parser.error("--layout requires a Nano transfer scenario")
    if (
        args.standing
        or args.transfer
        or args.camera_faults
        or args.moving_target
        or args.observed_author
        or args.revise_pickup
    ) and not args.provider:
        parser.error("Select --codex or --octos-deepseek to author the requested trial")
    if args.foreground_cadence and (not args.fleet or not args.provider):
        parser.error("--foreground-cadence requires --fleet and an author")
    if args.fleet and not args.provider:
        args.provider = "octos-deepseek"
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
    if args.resume_fleet:
        import shutil

        shutil.copytree(args.resume_fleet / "state", output / "state")
    if args.prepared_pickup and not args.revise_pickup:
        parser.error("--prepared-pickup requires --revise-pickup")
    if args.revise_pickup and args.compare_runs:
        parser.error("Select one recorded-trial journey")
    if args.revise_pickup:
        result = json.loads((args.revise_pickup / "result.json").read_bytes())
        if result.get("task_success") is not False or not result.get(
            "last_pickup_observation"
        ):
            parser.error(
                "--revise-pickup requires a stopped Nano pickup with a camera packet"
            )
    if args.compare_runs or args.revise_pickup:
        import shutil

        for index, source in enumerate(
            args.compare_runs or [args.revise_pickup], start=1
        ):
            if (
                not (source / "recording.json").is_file()
                or not (source / "result.json").is_file()
            ):
                parser.error(f"Missing recording or verdict in {source}")
            shutil.copytree(source, output / "state/aisle" / f"000{index}-Nano-run")
    if args.prepared_pickup:
        name = args.prepared_pickup.name
        if (
            not name.endswith("-Nano-prepare")
            or not (args.prepared_pickup / "controller.json").is_file()
        ):
            parser.error("Select a completed native Nano preparation")
        shutil.copytree(args.prepared_pickup, output / "state/aisle" / name)
    # The binary can come from another build directory. Its digest identifies the
    # executable, while these fields identify the workspace observed at launch.
    if args.foreground_cadence:
        from fleet_cadence import registry

        env["DOROBOT_AISLE_WORKERS"] = str(
            registry(output, studio_root, args.foreground_cadence)
        )
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
                        "crates/dorobot-studio-core/src/widgets/video_player.rs",
                        "plugins/aisle/src/lib.rs",
                        "crates/dorobot-studio-core/src/api/aisle.rs",
                        "dorobot-flex/src/bin/dorobot-flex.rs",
                    )
                ),
            ]
        )
    }

    def check_sources():
        for name, expected in source_hashes.items():
            if (
                hashlib.sha256((studio_root / name).read_bytes()).hexdigest()
                != expected
            ):
                raise RuntimeError(
                    f"Native verification source changed during the run: {name}"
                )

    sessions = []
    for stage in ["journey", "recovery"]:
        check_sources()
        with args.studio.open("rb") as binary:
            if hashlib.file_digest(binary, "sha256").hexdigest() != binary_hash:
                raise RuntimeError(
                    "Studio executable changed between journey and recovery"
                )
        report.unlink(missing_ok=True)
        command = [str(args.studio.resolve()), "--remote", "--screen", "aisle-task"]
        if args.dark:
            command.append("--dark")
        if args.fleet or args.transfer:
            env["DOROBOT_AISLE_DRAW_TRACE"] = str(
                output / "verified" / f"{stage}-draws.jsonl"
            )
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
                if args.prepared_pickup:
                    check.append("--prepared-pickup")
                if args.revise_pickup:
                    check.append("--revise-pickup")
                if args.compare_runs:
                    check.append("--compare-runs")
                if args.fleet:
                    check.append("--fleet")
                    if args.resume_fleet:
                        check.append("--resume-fleet")
                    if args.fleet_node:
                        check.extend(["--fleet-node", args.fleet_node])
                    if args.foreground_cadence:
                        check.extend(
                            ["--foreground-cadence", str(args.foreground_cadence)]
                        )
                if args.observed_author:
                    check.append("--observed-author")
                if args.moving_target:
                    check.append("--moving-target")
                if args.camera_faults:
                    check.append("--camera-faults")
                if args.handoff:
                    check.append("--handoff")
                if args.evaluation:
                    check.extend(["--evaluation", str(args.evaluation.resolve())])
                if args.train_grasp:
                    check.append("--train-grasp")
                if args.train_reach:
                    check.append("--train-reach")
                if args.standing:
                    check.append("--standing")
                    if args.heading:
                        check.extend(["--heading", str(args.heading)])
                    if args.upright:
                        check.append("--upright")
                    if args.impact:
                        check.append("--impact")
                if args.photo_input:
                    check.extend(["--photo-input", str(args.photo_input.resolve())])
                if args.survey:
                    check.extend(["--survey", str(args.survey.resolve())])
                if args.device_preflight:
                    check.append("--device-preflight")
                if args.pickup_motion:
                    check.extend(["--pickup-motion", args.pickup_motion])
                if args.transfer:
                    check.extend(
                        ["--transfer", args.transfer, "--layout", str(args.layout)]
                    )
                if stage == "recovery":
                    check.append("--recover")
                    if args.compare_runs and args.backend == "offscreen":
                        check.append("--screenshots")
                else:
                    if args.provider:
                        check.append("--" + args.provider)
                    if args.backend == "offscreen":
                        check.append("--screenshots")
                timeout = (
                    1800
                    if args.train_grasp
                    else 900
                    if args.transfer or args.fleet or args.revise_pickup
                    else 330
                )
                try:
                    subprocess.run(check, check=True, timeout=timeout)
                except (subprocess.SubprocessError, KeyboardInterrupt):
                    if args.foreground_cadence:
                        from fleet_cadence import cancel_benchmark_jobs

                        cancel_benchmark_jobs(output)
                    raise
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

    check_sources()
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
    if args.foreground_cadence:
        cadence = json.loads((directory / "preview-cadence.json").read_bytes())
        if not cadence["meets_cadence"]:
            raise SystemExit(
                "Cadence target missed. Measured rates and all robot verdicts are retained in "
                + str(directory)
            )


if __name__ == "__main__":
    main()
