"""Assemble the native camera cadence evidence from original verification directories."""

import argparse
import hashlib
import json
import shutil
import statistics
import subprocess
from collections import Counter
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile


def get(path):
    return json.loads(path.read_bytes())


def save(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--case",
        dest="runs",
        action="append",
        type=Path,
        required=True,
        help="Directory from a native fleet cadence run (repeatable)",
    )
    parser.add_argument(
        "--ssh", type=Path, required=True, help="Output directory of loopback_ssh.py"
    )
    parser.add_argument(
        "--direct",
        type=Path,
        required=True,
        help="Output of the direct --transfer red run",
    )
    parser.add_argument(
        "--out",
        type=Path,
        required=True,
        help="New directory for the assembled evidence",
    )
    args = parser.parse_args()
    destination = args.out
    destination.mkdir(parents=True, exist_ok=False)
    summary = {}
    motor = None
    with ZipFile(
        destination / "measurements.zip", "w", ZIP_DEFLATED, compresslevel=9
    ) as bundle:
        for job in args.runs:
            key = job.resolve().name
            if key in summary:
                raise ValueError("Use distinct run directory names")
            verified = job / "verified"
            manifest = get(verified / "mode-manifest.json")
            run = next((job / "state/aisle").glob("*-Nano-run"))
            result = get(run / "result.json")
            state = get(job / "ui.json")["state"]["aisle"]
            assert run.resolve() == Path(state["selected"])
            video_counts = {}
            for video in Path(state["dataset"]).rglob("*.mp4"):
                probe = json.loads(
                    subprocess.check_output(
                        [
                            "ffprobe",
                            "-v",
                            "error",
                            "-count_frames",
                            "-show_entries",
                            "stream=nb_read_frames,width,height",
                            "-of",
                            "json",
                            str(video),
                        ]
                    )
                )
                video_counts[str(video.relative_to(Path(state["dataset"])))] = probe[
                    "streams"
                ][0]
            assert len(video_counts) == 3 and all(
                int(value["nb_read_frames"]) == result["frames"] == 342
                for value in video_counts.values()
            )
            bundle.writestr(
                key + "/exported-videos.json", json.dumps(video_counts, indent=2)
            )
            for filename in (
                "journey.json",
                "recovery.json",
                "mode-manifest.json",
                "preview-cadence.json",
                "background-verdicts.json",
                "journey-draws.jsonl",
            ):
                bundle.write(verified / filename, key + "/" + filename)
            for filename in (
                "preview-timing.json",
                "source.json",
                "result.json",
                "controller.json",
                "worker-origin.json",
                "motor-targets.f64",
            ):
                bundle.write(run / filename, key + "/" + filename)
            for folder in job.glob("background-author-*"):
                for filename in ("provider-metrics.json", "controller.json"):
                    bundle.write(
                        folder / filename, key + "/" + folder.name + "/" + filename
                    )
            for folder in (job / "state/aisle").glob("*-Nano-prepare"):
                for filename in ("provider-metrics.json", "controller.json"):
                    bundle.write(
                        folder / filename, key + "/foreground-author/" + filename
                    )
            backgrounds = get(verified / "background-verdicts.json")
            for index, background in enumerate(backgrounds):
                archive = job / f"background-{index}.zip"
                assert (
                    hashlib.sha256(archive.read_bytes()).hexdigest()
                    == background["archive_sha256"]
                )
                with ZipFile(archive) as data:
                    journal = data.read("motor-targets.f64")
                    assert (
                        hashlib.sha256(journal).hexdigest()
                        == background["result"]["actuator_audit"]["sha256"]
                    )
                    bundle.writestr(
                        key + f"/background-{index}-motor-targets.f64", journal
                    )
            cadence = get(verified / "preview-cadence.json")
            rows = [
                json.loads(line)
                for line in (verified / "journey-draws.jsonl").read_text().splitlines()
            ]
            rows = [
                x
                for x in rows
                if cadence["first_common_tick"]
                <= x["tick"]
                <= cadence["last_common_tick"]
                and Path(x["source"]).name
                == cadence.get("draw_source", Path(rows[0]["source"]).name)
            ]
            bins = Counter(
                (entry["elapsed_ns"] - rows[0]["elapsed_ns"]) // 1_000_000_000
                for entry in rows
            )
            complete = [bins[index] for index in range(max(bins))]
            assert cadence["viewport_draws"] == len(rows) and round(
                cadence["viewport_fps"], 3
            ) == round(
                (len(rows) - 1)
                * 1e9
                / (rows[-1]["elapsed_ns"] - rows[0]["elapsed_ns"]),
                3,
            )
            assert all(
                entry["result"]["task_success"]
                and entry["result"]["wheel_supported_ticks"] == 6848
                and entry["result"]["actuator_audit"]["sha256"]
                == result["actuator_audit"]["sha256"]
                for entry in backgrounds
            )
            assert result["wheel_supported_ticks"] == 6848 and result["task_success"]
            assert (
                hashlib.sha256((run / "motor-targets.f64").read_bytes()).hexdigest()
                == result["actuator_audit"]["sha256"]
            )
            if motor:
                assert motor == result["actuator_audit"]["sha256"]
            motor = result["actuator_audit"]["sha256"]
            begin = max(job_result["progress"][0][1] for job_result in backgrounds)
            end = min(job_result["progress"][-1][1] for job_result in backgrounds)
            rates = []
            for job_result in backgrounds:
                stamps = [
                    row[1] for row in job_result["samples"] if begin <= row[1] <= end
                ]
                rates.append(
                    1e9 * (len(stamps) - 1) / (stamps[-1] - stamps[0])
                    if len(stamps) > 1
                    else 0.0
                )
            passed = (
                cadence["viewport_fps"] >= 25
                and min(rates) >= 1
                and cadence["draw_window_s"] >= 30
            )
            assert cadence.get("meets_cadence", passed) == passed
            summary[key] = cadence | dict(
                provider=manifest["provider"],
                backend=manifest["backend"],
                viewport_size=manifest["requested_size"],
                sessions=1 + len(backgrounds),
                meets_cadence=passed,
                background_wall_fps=rates,
                complete_one_second_windows=len(complete),
                windows_below_25=sum(index < 25 for index in complete),
                minimum_draws_in_one_second=min(complete),
                maximum_draws_in_one_second=max(complete),
                median_draws_in_one_second=statistics.median(complete),
                successes=1 + len(backgrounds),
                motor_sha256=motor,
                exported_videos=video_counts,
            )
        ssh = get(args.ssh / "proof.json")
        bundle.write(args.ssh / "proof.json", "openssh/receipt.json")
        ssh_trial = args.ssh / "worker" / ssh["job"] / "trial"
        bundle.write(ssh_trial / "preview-timing.json", "openssh/preview-timing.json")
        summary["openssh"] = dict(
            transport=ssh["transport"],
            fresh_received_frames=ssh["frames"],
            source_frames=ssh["source_frames"],
            rgb_hashes=len(set(row[2] for row in ssh["measurements"])),
            rate=ssh["received_fps"],
            job=ssh["job"],
        )
        direct = args.direct
        direct_trial = next((direct / "state/aisle").glob("*-Nano-run"))
        direct_draws = [
            json.loads(line)
            for line in (direct / "verified/journey-draws.jsonl")
            .read_text()
            .splitlines()
        ]
        bundle.write(direct / "verified/journey-draws.jsonl", "direct-run/draws.jsonl")
        bundle.write(
            direct_trial / "preview-timing.json", "direct-run/preview-timing.json"
        )
        summary["direct_run"] = dict(
            draws=len(direct_draws),
            rate=1e9
            * (len(direct_draws) - 1)
            / (direct_draws[-1]["elapsed_ns"] - direct_draws[0]["elapsed_ns"]),
        )
    save(destination / "results.json", summary)
    for case in args.runs:
        screenshot = case / "verified/foreground.png"
        if screenshot.is_file():
            shutil.copyfile(screenshot, destination / "foreground.png")
            break
    print(
        json.dumps(
            {
                k: value.get("viewport_fps", value.get("rate"))
                for k, value in summary.items()
            }
        )
    )


if __name__ == "__main__":
    main()
