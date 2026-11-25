"""Recalculate native RGB and observation rates from the archived measurements."""

import argparse
import hashlib
import json
from collections import Counter
from pathlib import Path
from statistics import median
from zipfile import ZipFile


def rate(stamps):
    assert len(stamps) > 1 and all(a < b for a, b in zip(stamps, stamps[1:]))
    return (len(stamps) - 1) * 1e9 / (stamps[-1] - stamps[0])


def check_case(archive, name, reported):
    def load(file):
        return json.loads(archive.read(f"{name}/{file}"))

    captured = dict(load("preview-timing.json")["tick_time"])
    jobs = load("background-verdicts.json")
    start = max(job["progress"][0][1] for job in jobs)
    end = min(job["progress"][-1][1] for job in jobs)
    trace = archive.read(f"{name}/journey-draws.jsonl").splitlines()
    draws = [json.loads(line) for line in trace]
    watch = next(
        draw["source"] for draw in draws if draw["source"].endswith("-fleet-watch")
    )
    frames = [
        draw
        for draw in draws
        if draw["source"] == watch and start <= captured[draw["tick"]] <= end
    ]
    assert all(a["tick"] < b["tick"] for a, b in zip(frames, frames[1:]))
    stamps = [frame["elapsed_ns"] for frame in frames]
    foreground = rate(stamps)
    assert abs(foreground - reported["viewport_fps"]) < 1e-6
    bins = Counter((stamp - stamps[0]) // 1_000_000_000 for stamp in stamps)
    assert sum(bins[i] < 25 for i in range(max(bins))) == reported["windows_below_25"]
    gaps = sorted((b - a) / 1e6 for a, b in zip(stamps, stamps[1:]))
    for key, value in (
        ("median_draw_gap_ms", median(gaps)),
        ("maximum_draw_gap_ms", gaps[-1]),
        ("p95_draw_gap_ms", gaps[int(0.95 * (len(gaps) - 1))]),
    ):
        assert abs(value - reported[key]) < 1e-6
    assert len(stamps) == reported["viewport_draws"]
    motor = reported["motor_sha256"]
    verdicts = [(load("result.json"), "motor-targets.f64")]
    rates = []
    for index, job in enumerate(jobs):
        rates.append(
            rate([stamp for _, stamp, _ in job["samples"] if start <= stamp <= end])
        )
        verdicts.append((job["result"], f"background-{index}-motor-targets.f64"))
    for result, path in verdicts:
        assert result["task_success"] and result["requested"] == ["red"]
        assert result["wheel_supported_ticks"] == result["physics_ticks"] == 6848
        assert (
            hashlib.sha256(archive.read(f"{name}/{path}")).hexdigest()
            == result["actuator_audit"]["sha256"]
            == motor
        )
    assert (
        len(rates) == len(reported["background_wall_fps"]) == reported["sessions"] - 1
    )
    assert all(
        abs(a - b) < 1e-6 for a, b in zip(rates, reported["background_wall_fps"])
    )
    assert (
        stamps[-1] - stamps[0] >= 30e9 and foreground >= 25 and min(rates) >= 1
    ) == reported["meets_cadence"]
    videos = load("exported-videos.json")
    assert len(videos) == 3 and all(
        int(video["nb_read_frames"]) == 342 for video in videos.values()
    )
    return f"{name}: {foreground:.2f} viewport FPS, {min(rates):.2f}–{max(rates):.2f} background FPS"


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "directory", nargs="?", type=Path, default=Path(__file__).resolve().parent
    )
    root = parser.parse_args().directory
    summary = json.loads((root / "results.json").read_bytes())
    with ZipFile(root / "measurements.zip") as archive:
        for name, result in summary.items():
            if "sessions" in result:
                print(check_case(archive, name, result))
        ssh = json.loads(archive.read("openssh/receipt.json"))
        source = json.loads(archive.read("openssh/preview-timing.json"))["tick_time"]
        received = ssh["measurements"]
        ticks = [r[0] for r in received]
        source_ticks = {r[0] for r in source}
        assert (
            all(a < b for a, b in zip(ticks, ticks[1:])) and set(ticks) <= source_ticks
        )
        assert (
            len(ticks) == ssh["frames"] == summary["openssh"]["fresh_received_frames"]
        )
        assert (
            len(source_ticks)
            == ssh["source_frames"]
            == summary["openssh"]["source_frames"]
        )
        assert len(ticks) > 1 and ticks[-1] == source[-1][0]
        assert abs(rate([r[1] for r in received]) - summary["openssh"]["rate"]) < 1e-6
        assert len({r[2] for r in received}) == summary["openssh"]["rgb_hashes"]
        print(f"OpenSSH: {len(received)} of {len(source_ticks)} distinct source ticks received")
        direct = [
            json.loads(line)
            for line in archive.read("direct-run/draws.jsonl").splitlines()
        ]
        source_ticks = dict(
            json.loads(archive.read("direct-run/preview-timing.json"))["tick_time"]
        )
        assert all(row["tick"] in source_ticks for row in direct)
        assert all(a["tick"] < b["tick"] for a, b in zip(direct, direct[1:]))
        direct_rate = rate([r["elapsed_ns"] for r in direct])
        assert len(direct) == summary["direct_run"]["draws"]
        assert abs(direct_rate - summary["direct_run"]["rate"]) < 1e-6
        print(f"Direct Run: {len(direct)} native draws at {direct_rate:.2f} FPS")
