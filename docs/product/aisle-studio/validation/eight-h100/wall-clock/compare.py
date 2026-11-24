"""Compare the two H100 observation schedules with fixed controllers and motor targets."""

import argparse
import json
import sys
from pathlib import Path
from zipfile import ZipFile

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
from audit import cohort


def delivery_intervals(archive, name):
    def read(file):
        return archive.read(f"{name}/{file}")

    draws = [json.loads(line) for line in read("draws.jsonl").splitlines()]
    foreground = [row[0] for row in draws if row[3] == draws[0][3]]
    workers = [json.loads(read(f"gpu{i}/delivery.json")) for i in range(1, 8)]
    start = max([foreground[0]] + [w["steps"][0][1] for w in workers])
    end = min([foreground[-1]] + [w["steps"][-1][1] for w in workers])
    second = 1_000_000_000
    total = (end - start) // second
    assert total >= 30

    def intervals(times, target):
        counts = [0] * total
        within = [t for t in times if start <= t < end]
        for stamp in within:
            index = (stamp - start) // second
            if index < total:
                counts[index] += 1
        return {
            "seconds_below_target": sum(count < target for count in counts),
            "max_gap_ms": max(b - a for a, b in zip(within, within[1:])) / 1e6,
        }

    return {
        "complete_seconds": total,
        "foreground": intervals(foreground, 25),
        "background": [intervals([s[1] for s in w["samples"]], 1) for w in workers],
    }


def compare(earlier, current, datasets):
    results = {}
    with ZipFile(earlier) as baseline, ZipFile(current) as measured:
        for name in ("codex", "deepseek"):
            old = cohort(baseline, name)
            new = cohort(measured, name, datasets)
            for gpu, (before, after) in enumerate(
                zip(old["rows"], new["rows"], strict=True)
            ):
                assert before["gpu"] == after["gpu"] == gpu
                stem = f"{name}/gpu{gpu}/"
                for item in (
                    "controller.json",
                    "author/provider-metrics.json",
                    "motor-targets.f64",
                ):
                    assert baseline.read(stem + item) == measured.read(stem + item)
                if gpu:
                    original = json.loads(baseline.read(stem + "delivery.json"))
                    assert (
                        original.get("observation_clock", "simulation") == "simulation"
                    )
                    assert original.get("observation_period_s", 1) == 1
                    assert (
                        after["observation_clock"] == "wall"
                        and after["observation_period_s"] == 0.5
                    )
                for key in ("success", "placed_red", "supported_ticks", "ticks"):
                    assert before[key] == after[key]
            rates = [row["worker_publication_hz"] for row in new["rows"][1:]]
            results[name] = {
                "controllers_reused": len(new["rows"]),
                "identical_motor_journals": len(new["rows"]),
                "tasks_passed": new["task_passes"],
                "window_seconds": new["common_window_s"],
                "viewport_fps": new["viewport_fps"],
                "source_fps": new["source_fps"],
                "previous_viewport_fps": old["viewport_fps"],
                "background_worker_hz": [min(rates), max(rates)],
                "background_receiver_hz": [new["min_wall_hz"], new["max_wall_hz"]],
                "previous_background_receiver_hz": [
                    old["min_wall_hz"],
                    old["max_wall_hz"],
                ],
                "receivers_at_1hz": sum(row["wall_hz"] >= 1 for row in new["rows"][1:]),
                "all_seven_receivers_at_1hz": new["min_wall_hz"] >= 1,
                "qualified": new["qualified"],
                "receipt_intervals": delivery_intervals(measured, name),
                "datasets_frames_per_camera": new["rows"][0]["dataset_frames"],
            }
    return results


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--baseline", type=Path, default=HERE.parent / "measurements.zip"
    )
    parser.add_argument("--archive", type=Path, default=HERE / "measurements.zip")
    parser.add_argument("--datasets", action="store_true")
    args = parser.parse_args()
    print(json.dumps(compare(args.baseline, args.archive, args.datasets), indent=2))
