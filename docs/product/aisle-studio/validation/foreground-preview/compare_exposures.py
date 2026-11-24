"""Compare two recorded foreground Nano trials, including every saved camera exposure."""

import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def pack(archive, side, workspace):
    native = workspace / "verified"
    state = json.loads((workspace / "ui.json").read_bytes())["state"]["aisle"]
    trial, dataset = Path(state["selected"]), Path(state["dataset"])
    for path in (trial, dataset):
        if not path.resolve().is_relative_to(workspace.resolve()):
            raise ValueError(f"Selected path outside workspace: {path}")
    for name in ("mode-manifest.json", "journey-draws.jsonl", "recovery.json"):
        archive.write(native / name, f"{side}/{name}")
    for name in (
        "preview-timing.json",
        "controller.json",
        "result.json",
        "source.json",
        "motor-targets.f64",
        "worker-origin.json",
    ):
        archive.write(trial / name, f"{side}/{name}")
    archive.write(dataset / "meta/aisle.json", f"{side}/export.json")
    hashes = {
        str(path.relative_to(trial)): digest(path.read_bytes())
        for path in sorted((trial / "camera").glob("*/*.png"))
    }
    for name in ("actions.jsonl", "recording.json", "wrist-delivery.jsonl"):
        hashes[name] = digest((trial / name).read_bytes())
    for path in sorted(dataset.rglob("*.mp4")):
        hashes[str(path.relative_to(dataset))] = digest(path.read_bytes())
    archive.writestr(f"{side}/file-hashes.json", json.dumps(hashes))
    archive.writestr(
        f"{side}/reopened.json",
        json.dumps(
            dict(
                trial=trial.name,
                job=state["fleet"]["selected"]["id"],
                dataset=dataset.name,
                result=state["result"],
            )
        ),
    )


def rate(times):
    assert len(times) >= 2 and all(a < b for a, b in zip(times, times[1:]))
    return (len(times) - 1) * 1e9 / (times[-1] - times[0])


def measure(archive, side):
    def read(name):
        return json.loads(archive.read(f"{side}/{name}"))

    result = read("result.json")
    plan = archive.read(f"{side}/controller.json")
    timing = read("preview-timing.json")
    restored = read("reopened.json")
    assert (
        restored["result"] == result
        and restored["job"] == read("worker-origin.json")["job"]
    )
    assert (
        read("mode-manifest.json")["sessions"][0]["pid"]
        != read("mode-manifest.json")["sessions"][1]["pid"]
    )
    assert (
        read("export.json")["trial"] == restored["trial"]
        and restored["dataset"] == "dataset"
    )
    assert (
        read("source.json")["controller_sha256"]
        == digest(plan)
        == result["controller_hash"]
    )
    assert (
        result["wheel_supported_ticks"] == result["physics_ticks"]
        and not result["unsupported_wheel_ranges"]
    )
    assert (
        result["task_success"]
        and result["requested"] == ["red"]
        and result["actuator_audit"]["passed"]
    )
    assert (
        digest(archive.read(f"{side}/motor-targets.f64"))
        == result["actuator_audit"]["sha256"]
    )
    data = read("file-hashes.json")
    cameras = ("head", "left_wrist", "right_wrist")
    expected = {
        f"camera/{name}/{i:04d}.png"
        for name in cameras
        for i in range(result["frames"])
    }
    assert {p for p in data if p.startswith("camera/")} == expected
    assert {p for p in data if p.endswith(".mp4")} == {
        f"videos/chunk-000/observation.images.{name}/episode_000000.mp4"
        for name in cameras
    }
    ticks, times = zip(*timing["tick_time"])
    assert all(a < b for a, b in zip(ticks, ticks[1:]))
    capture_ticks = set(ticks)
    trace = [
        json.loads(line)
        for line in archive.read(f"{side}/journey-draws.jsonl").splitlines()
    ]
    watch = next(
        row["source"] for row in trace if row["source"].endswith("-fleet-watch")
    )
    draws = [row for row in trace if row["source"] == watch]
    assert all(a["tick"] < b["tick"] for a, b in zip(draws, draws[1:]))
    assert all(row["tick"] in capture_ticks for row in draws)
    stamps = [row["elapsed_ns"] for row in draws]
    assert stamps[-1] - stamps[0] >= 30e9
    bins = Counter((s - stamps[0]) // 1_000_000_000 for s in stamps)
    rates = dict(
        captures=len(ticks),
        source_fps=rate(times),
        draws=len(stamps),
        viewport_fps=rate(stamps),
        full_seconds=max(bins),
        seconds_below_25=sum(bins[i] < 25 for i in range(max(bins))),
    )
    if side == "after":
        reads, renders = timing["camera_requests"], timing["camera_renders"]
        assert set(reads) == set(renders) == set(cameras)
        assert all(0 < renders[k] <= reads[k] for k in cameras)
        rates["camera_reads"] = reads
        rates["camera_renders"] = renders
        rates["shared_reads"] = sum(reads.values()) - sum(renders.values())
        assert rates["shared_reads"] > 0
    return rates, data, result


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("archive", type=Path)
    cli.add_argument(
        "--pack",
        nargs=2,
        metavar=("BASELINE", "CANDIDATE"),
        type=Path,
        help="Create a fresh audit archive from two completed native journeys",
    )
    args = cli.parse_args()
    if not __debug__:
        cli.error("Run this measurement with assertions enabled")
    if args.pack:
        with ZipFile(args.archive, "x", ZIP_DEFLATED) as z:
            for side, path in zip(("before", "after"), args.pack):
                pack(z, side, path)
    with ZipFile(args.archive) as z:
        before, original, left = measure(z, "before")
        after, candidate, right = measure(z, "after")
        if original != candidate:
            diff = sorted(
                k
                for k in original.keys() | candidate.keys()
                if original.get(k) != candidate.get(k)
            )
            raise ValueError(
                f"{len(diff)} mismatching artifacts; first five: {diff[:5]}"
            )
        assert left == right
        print(
            json.dumps(
                dict(
                    before=before,
                    after=after,
                    physics_ticks=left["physics_ticks"],
                    frames_per_camera=left["frames"],
                    retained_pngs=3 * left["frames"],
                    controller_sha256=left["controller_hash"],
                    motor_sha256=left["actuator_audit"]["sha256"],
                ),
                indent=2,
            )
        )


if __name__ == "__main__":
    main()
