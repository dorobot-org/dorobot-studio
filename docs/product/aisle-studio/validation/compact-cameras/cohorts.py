"""Audit the common interval, unique draws and verified observer receipts for each fleet run."""

import gzip
import json
from pathlib import Path


def bounded_rate(times):
    assert len(times) >= 2 and all(a < b for a, b in zip(times, times[1:]))
    return (len(times) - 1) * 1e9 / (times[-1] - times[0])


def measure(path):
    with gzip.open(path, "rt") as source:
        record = json.load(source)
    draws = record["draws"]
    source = dict(record["source_ticks"])
    assert all(row[2] in source for row in draws)
    assert all(a[2] < b[2] for a, b in zip(draws, draws[1:]))
    active = [(draws[0][0], draws[-1][0])]
    observations = []
    assert set(record["background"]) == {str(i) for i in range(1, 8)}
    for background in record["background"].values():
        steps = background["steps"]
        active.append((steps[0][1], steps[-1][1]))
        exposures = {
            frame["tick"]: frame["sha256"] for frame in background["exposures"]
        }
        samples = background["samples"]
        assert all(exposures[tick] == digest for tick, _, digest in samples)
        assert all(a[0] < b[0] for a, b in zip(samples, samples[1:]))
        observations.append([stamp for _, stamp, _ in samples])

    begin = max(a for a, _ in active)
    end = min(b for _, b in active)
    window = [draw for draw in draws if begin <= draw[0] <= end]
    fps = bounded_rate([draw[1] for draw in window])
    rates = [
        bounded_rate([t for t in times if begin <= t <= end]) for times in observations
    ]
    source_rate = bounded_rate(
        [
            stamp
            for tick, stamp in record["source_ticks"]
            if window[0][2] <= tick <= window[-1][2]
        ]
    )
    summary = record["summary"]
    assert abs(source_rate - summary["source_fps"]) < 1e-9
    assert abs(fps - summary["viewport_fps"]) < 1e-9
    assert abs(min(rates) - summary["min_wall_hz"]) < 1e-9
    assert abs(max(rates) - summary["max_wall_hz"]) < 1e-9
    assert all(
        r["success"] and r["placed_red"] and r["supported_ticks"] == r["ticks"]
        for r in summary["rows"]
    )
    assert len(summary["rows"]) == 8 and len(summary["gpu_peak_mib"]) == 8
    assert record["profile"] == "compact-v1"
    full_seconds = (end - begin) // 1_000_000_000
    passes = 0
    streak = longest_streak = 0
    draw_counts = []
    for i in range(full_seconds):
        left = begin + i * 1_000_000_000
        right = left + 1_000_000_000
        count = sum(left <= draw[0] < right for draw in window)
        draw_counts.append(count)
        both = count >= 25 and all(
            any(left <= t < right for t in times) for times in observations
        )
        passes += int(both)
        streak = streak + 1 if both else 0
        longest_streak = max(streak, longest_streak)
    return dict(
        overlap_s=(end - begin) / 1e9,
        viewport_fps=fps,
        source_fps=source_rate,
        lowest_second=min(draw_counts),
        slowest_background_hz=min(rates),
        fastest_background_hz=max(rates),
        full_seconds=full_seconds,
        longest_unbroken_seconds=longest_streak,
        seconds_meeting_both=passes,
        all_tasks_passed=True,
        qualified=summary["qualified"],
    )


if __name__ == "__main__":
    for archive in sorted(Path(__file__).parent.glob("*-cohort.json.gz")):
        print(archive.name, json.dumps(measure(archive), indent=2))
