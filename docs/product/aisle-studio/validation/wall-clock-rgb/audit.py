"""Recalculate native viewport and background RGB rates from the retained traces."""

import argparse
import hashlib
import io
import json
import runpy
from collections import Counter
from pathlib import Path
from itertools import combinations
from zipfile import ZipFile

import numpy as np

from archive import BASE_CASES, EXTENDED_CASES, G1, NATIVE


def rate(stamps):
    assert len(stamps) > 1 and all(a < b for a, b in zip(stamps, stamps[1:]))
    return (len(stamps) - 1) * 1e9 / (stamps[-1] - stamps[0])


def case(z, tag, capacity):
    def raw(f):
        return z.read(tag + "/" + f)

    def load(f):
        return json.loads(raw(f))

    def lines(f):
        return list(map(json.loads, raw(f).splitlines()))

    proof = load("journey.json")[-1]
    recovered = [r["check"] for r in load("recovery.json")]
    assert recovered[-1] == "Imported worker trial and export recovered"
    result = load("foreground/result.json")
    motor = result["actuator_audit"]["sha256"]
    assert result["task_success"] and result["cubes"]["red"]["passed"]
    assert (
        result["wheel_supported_ticks"]
        == result["physics_ticks"]
        == result["actuator_audit"]["applied_rows"]
    )
    assert result["actuator_audit"]["passed"] and result["rejected_motor_writes"] == 0
    assert hashlib.sha256(raw("foreground/motor-targets.f64")).hexdigest() == motor
    assert (
        hashlib.sha256(raw("foreground/controller.json")).hexdigest()
        == result["controller_hash"]
    )
    assert proof["result"]["controller_hash"] == result["controller_hash"]
    assert load("foreground/meta/info.json")["total_frames"] == result["frames"]
    actor = load("foreground/author.json")
    model = actor["model"]
    assert (
        actor["provider"] in ("codex", "octos-deepseek") and actor["wall_seconds"] > 0
    )
    metadata = load("foreground/meta/aisle.json")
    assert metadata["controller_hash"] == result["controller_hash"]
    assert metadata["model_hash"] == result["model_hash"]
    assert set(metadata["calibration"]) == {"head", "left_wrist", "right_wrist"}
    requested = 0.5 if ("half" in tag or tag.startswith(("final", "snapshot-"))) else 1
    if not capacity:
        assert "Recovered wall-clock setting for the submitted observer" in recovered
        source = load("selected-observer/source.json")
        assert (
            source["observation_clock"] == "wall"
            and source["observation_period_s"] == requested
        )
        frames = lines("selected-observer/background-observations.jsonl")
        assert [r["tick"] for r in frames] == proof["observation_ticks"]
        assert (
            dict((r["tick"], r["sha256"]) for r in frames)[proof["snapshot"]["tick"]]
            == proof["snapshot"]["sha256"]
        )
        return dict(
            period_s=requested,
            snapshot_tick=proof["snapshot"]["tick"],
            published=len(frames),
            wall_hz=rate([r["published_monotonic_ns"] for r in frames]),
            restored_frames=result["frames"],
        )
    report = load("preview-cadence.json")
    assert (
        report["background_clock"] == "wall"
        and report.get("background_period_s", 1) == requested
    )
    jobs = load("background-verdicts.json")
    expected = 8 if tag.startswith("eight-") else int(tag.split("-")[0])
    assert len(jobs) + 1 == report["sessions"] == expected
    assert len({proof["foreground_id"], *(j["id"] for j in jobs)}) == expected
    start = max(j["progress"][0][1] for j in jobs)
    end = min(j["progress"][-1][1] for j in jobs)
    times = dict(load("foreground/preview-timing.json")["tick_time"])
    draws = lines("draws.jsonl")
    stream = next(r["source"] for r in draws if r["source"].endswith("-fleet-watch"))
    live = [
        r for r in draws if r["source"] == stream and start <= times[r["tick"]] <= end
    ]
    assert all(a["tick"] < b["tick"] for a, b in zip(live, live[1:]))
    stamps = [r["elapsed_ns"] for r in live]
    viewport = rate(stamps)
    bins = Counter((t - stamps[0]) // 1_000_000_000 for t in stamps)
    full_seconds = max(bins)
    assert abs(viewport - report["viewport_fps"]) < 1e-6
    assert len(live) == report["viewport_draws"]
    rates = []
    sources = []
    counts = []
    for i, job in enumerate(jobs):
        base = f"observer-{i}/"
        r = load(base + "result.json")
        s = load(base + "source.json")
        assert r == job["result"] and r["task_success"] and r["requested"] == ["red"]
        assert (
            r["cubes"]["red"]["passed"]
            and r["wheel_supported_ticks"]
            == r["physics_ticks"]
            == result["physics_ticks"]
        )
        assert (
            hashlib.sha256(raw(base + "motor-targets.f64")).hexdigest()
            == r["actuator_audit"]["sha256"]
            == motor
        )
        assert (
            hashlib.sha256(raw(base + "controller.json")).hexdigest()
            == s["controller_sha256"]
            == r["controller_hash"]
        )
        assert (
            s["observation_clock"] == "wall" and s["observation_period_s"] == requested
        )
        assert load(base + "author.json") == job["author"]
        assert job["author"]["model"] == model and job["author"]["wall_seconds"] > 0
        frames = lines(base + "background-observations.jsonl")
        assert all(a["tick"] < b["tick"] for a, b in zip(frames, frames[1:]))
        digests = {f["tick"]: f["sha256"] for f in frames}
        assert all(digests[t] == h for t, _, h in job["samples"])
        pixels = raw(base + "preview.rgb")
        assert len(pixels) == 4 + 480 * 360 * 3
        assert int.from_bytes(pixels[:4], "big") == frames[-1]["tick"]
        assert hashlib.sha256(pixels[4:]).hexdigest() == frames[-1]["sha256"]
        published = [
            f["published_monotonic_ns"]
            for f in frames
            if start <= f["published_monotonic_ns"] <= end
        ]
        delivered = [stamp for _, stamp, _ in job["samples"] if start <= stamp <= end]
        rates.append(rate(delivered))
        sources.append(rate(published))
        counts.append([len(published), len(delivered)])
    assert all(abs(a - b) < 1e-6 for a, b in zip(rates, report["background_wall_fps"]))
    duration = (live[-1]["elapsed_ns"] - live[0]["elapsed_ns"]) / 1e9
    passed = duration >= 30 and viewport >= 25 and min(rates) >= 1
    assert passed == report["meets_cadence"]
    return dict(
        period_s=requested,
        provider=model,
        shared_seconds=(end - start) / 1e9,
        viewport_fps=viewport,
        draws=len(live),
        complete_draw_seconds=full_seconds,
        seconds_below_25_fps=sum(bins[i] < 25 for i in range(full_seconds)),
        worker_publication_hz=sources,
        client_delivery_hz=rates,
        published_received_in_window=counts,
        meets_cadence=passed,
        transfers=1 + len(jobs),
        motor_sha256=motor,
        supported_ticks=result["physics_ticks"],
        exported_frames=result["frames"],
    )


def g1(z):
    results = {}
    targets = {}
    measured = {}
    for name in G1:
        b = "g1/" + name + "/"
        result = json.loads(z.read(b + "result.json"))
        source = json.loads(z.read(b + "source.json"))
        assert (
            result["task_success"]
            and result["both_feet_loaded_steps"] == result["steps"] == 2000
            and result["wrist_reach"]["passed"]
        )
        assert (
            result["max_joint_excursion_rad"] == 0 and result["root_pose_writes"] == 0
        )
        data = z.read(b + "motor-work.npz")
        assert hashlib.sha256(data).hexdigest() == result["motor_work_sha256"]
        assert (
            hashlib.sha256(z.read(b + "wrist-trace.npz")).hexdigest()
            == result["wrist_reach"]["trace_sha256"]
        )
        with np.load(io.BytesIO(data)) as trace:
            targets[name] = trace["target"]
            measured[name] = trace["q_after"]
        raw = z.read(b + "preview.rgb")
        frames = [
            json.loads(l)
            for l in z.read(b + "background-observations.jsonl").splitlines()
        ]
        assert hashlib.sha256(raw[4:]).hexdigest() == frames[-1]["sha256"]
        assert int.from_bytes(raw[:4], "big") == frames[-1]["tick"]
        if source["observation_clock"] == "simulation":
            assert [r["tick"] for r in frames] == [0, *range(199, 2000, 200)]
        results[name] = dict(
            clock=source["observation_clock"],
            period_s=source["observation_period_s"],
            images=len(frames),
            wall_hz=rate([f["published_monotonic_ns"] for f in frames]),
            foot_support_steps=result["both_feet_loaded_steps"],
            wrist_error_m=result["wrist_reach"]["max_final_100_error_m"],
        )
    pairs = {}
    for a, b in combinations([n for n in G1 if n.startswith("paired")], 2):
        pairs[a + "/" + b] = dict(
            target_difference_rad=float(abs(targets[a] - targets[b]).max()),
            encoder_difference_rad=float(abs(measured[a] - measured[b]).max()),
        )
    return dict(trials=results, paired_maxima=pairs)


def runner(z):
    record = json.loads(z.read("runner/receipt.json"))
    assert record["state"] == "completed" and record["observation_clock"] == "wall"
    assert record["observation_period_s"] == 0.5
    data = z.read("runner/worker.zip")
    assert hashlib.sha256(data).hexdigest() == record["archive_sha256"]
    with ZipFile(io.BytesIO(data)) as trial:
        source = json.loads(trial.read("source.json"))
        result = json.loads(trial.read("result.json"))
        assert source["backend"] == "metal" and source["observation_clock"] == "wall"
        assert source["observation_period_s"] == 0.5 and result == record["result"]
        assert (
            result["task_success"]
            and result["wheel_supported_ticks"] == result["physics_ticks"]
        )
        assert (
            hashlib.sha256(trial.read("motor-targets.f64")).hexdigest()
            == result["actuator_audit"]["sha256"]
        )
        frames = [
            json.loads(line)
            for line in trial.read("background-observations.jsonl").splitlines()
        ]
        digests = {r["tick"]: r["sha256"] for r in frames}
        assert all(digests[tick] == digest for tick, _, digest in record["samples"])
        latest = trial.read("preview.rgb")
        assert (
            hashlib.sha256(latest[4:]).hexdigest()
            == digests[int.from_bytes(latest[:4], "big")]
        )
    return dict(
        backend=source["backend"],
        received=len(record["samples"]),
        supported_ticks=result["physics_ticks"],
        placed_red=result["cubes"]["red"]["passed"],
    )


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("archive", type=Path)
    cli.add_argument("--output", type=Path)
    cli.add_argument(
        "--datasets",
        action="store_true",
        help="Decode three camera videos and verify the synchronized Parquet rows",
    )
    args = cli.parse_args()
    with ZipFile(args.archive) as z:
        declared = json.loads(z.read("capacity-cases.json"))
        assert len(set(declared)) == len(declared)
        assert set(BASE_CASES) <= set(declared) <= set(BASE_CASES + EXTENDED_CASES)
        recorded = {
            p.split("/")[0] for p in z.namelist() if p.endswith("/preview-cadence.json")
        }
        assert recorded == set(declared)
        capacity = {c: case(z, c, True) for c in declared}
        assert {
            "10-half-deepseek",
            "10-half-codex",
            "12-half-deepseek",
        } <= capacity.keys()
        twelve_passed = capacity["12-half-deepseek"]["meets_cadence"]
        assert ("16-half-deepseek" in capacity) == twelve_passed
        largest = max(
            row["transfers"]
            for tag, row in capacity.items()
            if tag.endswith("-deepseek") and row["meets_cadence"]
        )
        assert ("eight" if largest == 8 else str(largest)) + "-half-codex" in capacity
        if args.datasets:
            check_dataset = runpy.run_path(
                str(Path(__file__).resolve().parents[1] / "eight-h100/audit.py")
            )["check_dataset"]
            base = "eight-half-deepseek/dataset/"
            info = json.loads(z.read(base + "meta/info.json"))
            check_dataset(
                z.read, base, info["features"], info["fps"], info["total_frames"]
            )
        results = dict(
            capacity=capacity,
            native={c: case(z, c, False) for c in NATIVE},
            g1=g1(z),
            runner=runner(z),
        )
    data = json.dumps(results, indent=2) + "\n"
    if args.output:
        args.output.write_text(data)
    else:
        print(data, end="")


if __name__ == "__main__":
    main()
