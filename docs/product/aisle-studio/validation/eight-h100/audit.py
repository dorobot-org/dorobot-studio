"""Recompute physical verdicts, simultaneous viewport cadence and RGB delivery from a published ZIP."""

import argparse
import hashlib
import json
from pathlib import Path
from statistics import median
from zipfile import ZipFile


def cohort(archive, name):
    def read(file):
        return archive.read(name + "/" + file)

    def load(file):
        return json.loads(read(file))

    def lines(file):
        return [json.loads(line) for line in read(file).splitlines()]

    prep = load("authors.json")
    rows, active, receipts = [], [], []
    captures = dict(load("gpu0/preview-timing.json")["tick_time"])
    draws = lines("draws.jsonl")
    first_feed = draws[0][3]
    draws = [r for r in draws if r[3] == first_feed]
    assert len(draws) > 1 and all(a[2] < b[2] for a, b in zip(draws, draws[1:]))
    assert all(tick in captures for _, _, tick, _ in draws)
    active.append((draws[0][0], draws[-1][0]))
    for index, author in enumerate(prep["authors"]):
        base = f"gpu{index}/"
        source = load(base + "source.json")
        verdict = load(base + "result.json")
        controller = read(base + "controller.json")
        hashed = hashlib.sha256(controller).hexdigest()
        assert controller == read(base + "author/controller.json")
        assert (
            hashed
            == author["sha256"]
            == source["controller_sha256"]
            == verdict["controller_hash"]
        )
        assert (
            hashlib.sha256(read(base + "motor-targets.f64")).hexdigest()
            == verdict["actuator_audit"]["sha256"]
        )
        assert source["nexus"] == "0.2.1" and source["backend"] == "webgpu"
        assert verdict["requested"] == ["red"] and verdict["seed"] == 0
        assert json.loads(controller)["max_forward_speed_m_s"] == 0.08
        row = dict(
            gpu=index,
            success=verdict["task_success"],
            placed_red=verdict["cubes"]["red"]["passed"],
            supported_ticks=verdict["wheel_supported_ticks"],
            ticks=verdict["physics_ticks"],
            controller=hashed,
            motor=verdict["actuator_audit"]["sha256"],
            retries=0,
        )
        if index:
            delivery = load(base + "delivery.json")
            assert delivery["state"] == "completed" and verdict["recording"] is False
            samples = delivery["samples"]
            exposures = lines(base + "background-observations.jsonl")
            images = {e["tick"]: e["sha256"] for e in exposures}
            assert list(images) == list(range(100, row["ticks"] + 1, 100))
            assert all(e["width"] == 480 and e["height"] == 360 for e in exposures)
            assert all(images[tick] == digest for tick, _, digest in samples)
            steps = delivery["steps"]
            assert all(a[0] < b[0] and a[1] < b[1] for a, b in zip(steps, steps[1:]))
            assert all(
                a[0] < b[0] and a[1] < b[1] for a, b in zip(samples, samples[1:])
            )
            active.append((steps[0][1], steps[-1][1]))
            receipts.append(samples)
            row.update(
                published=len(exposures),
                delivered=len(samples),
                retries=len(delivery["errors"]),
            )
        else:
            proof = load(base + "journey.json")[-1]
            assert proof["origin"]["state"] == "completed"
            # The native JSON decoder can round the final bit of a position.
            for field in (
                "controller_hash",
                "model_hash",
                "task_success",
                "physics_ticks",
                "wheel_supported_ticks",
                "unsupported_wheel_ranges",
                "frames",
                "actuator_audit",
            ):
                assert proof["result"][field] == verdict[field]
            assert proof["result"]["cubes"]["red"]["passed"] == row["placed_red"]
            assert "recovered" in load(base + "recovery.json")[-1]["check"].lower()
            info = load(base + "dataset/meta/info.json")
            assert (
                info["total_frames"] == verdict["frames"]
                and info["total_episodes"] == 1
            )
            row.update(
                dataset_frames=info["total_frames"],
                recorded_cameras=[k for k in info["features"] if "images." in k],
            )
            assert len(row["recorded_cameras"]) == 3
        rows.append(row)

    begin = max(v[0] for v in active)
    end = min(v[1] for v in active)
    seconds = (end - begin) / 1e9
    assert seconds > 0
    live = [d for d in draws if begin <= d[0] <= end]
    gaps = [(b[1] - a[1]) / 1e6 for a, b in zip(live, live[1:])]
    assert gaps and min(gaps) > 0
    draw_s = (live[-1][1] - live[0][1]) / 1e9
    fps = (len(live) - 1) / draw_s
    rates = []
    for row, samples in zip(rows[1:], receipts):
        times = [stamp for _, stamp, _ in samples if begin <= stamp <= end]
        hz = 1e9 * (len(times) - 1) / (times[-1] - times[0]) if len(times) > 1 else 0
        row.update(
            common_deliveries=len(times),
            wall_hz=hz,
            entire_window_hz=len(times) / seconds,
        )
        rates.append(hz)
    gpu_logs = [v for v in lines("gpu.jsonl") if begin <= v[0] <= end and v[1] == 0]
    gpu_rows = [
        [list(map(str.strip, line.split(","))) for line in r[2]] for r in gpu_logs
    ]
    assert gpu_rows and all(len(r) == 8 for r in gpu_rows)
    observed = [max(int(r[i][2]) for r in gpu_rows) for i in range(8)]
    utilization = [median(int(r[i][3]) for r in gpu_rows) for i in range(8)]
    assert min(observed) > 0 and len({sample[1] for sample in gpu_rows[0]}) == 8
    compute = [
        line.split()
        for line in read("gpu-processes.txt").decode().splitlines()
        if "C+G" in line and "python" in line
    ]
    assert len({l[1] for l in compute}) == 8 and {int(l[0]) for l in compute} == set(
        range(8)
    )
    clocks = load("timing.json")
    latency = [a["provider"]["wall_seconds"] for a in prep["authors"]]
    assert len(rows) == 8 and clocks["native_exit"] == 0
    source_times = [v for t, v in captures.items() if live[0][2] <= t <= live[-1][2]]
    source_fps = (len(source_times) - 1) * 1e9 / (source_times[-1] - source_times[0])
    return dict(
        provider=prep["authors"][0]["provider"]["model"],
        common_window_s=seconds,
        draw_window_s=draw_s,
        viewport_fps=fps,
        source_fps=source_fps,
        viewport_draws=len(live),
        whole_window_viewport_fps=len(live) / seconds,
        draw_gap_ms_median=median(gaps),
        draw_gap_ms_p95=sorted(gaps)[int(0.95 * (len(gaps) - 1))],
        min_wall_hz=min(rates),
        max_wall_hz=max(rates),
        qualified=(draw_s >= 30 and fps >= 25 and min(rates) >= 1),
        task_passes=sum(r["success"] for r in rows),
        rows=rows,
        gpu_peak_mib=observed,
        gpu_median_util_pct=utilization,
        gpu_samples=len(gpu_rows),
        model_requests=len(latency),
        model_wall_median_s=median(latency),
        author_batch_s=(prep["end_ns"] - prep["start_ns"]) / 1e9,
        launch_through_native_recovery_s=(clocks["end_ns"] - clocks["start_ns"]) / 1e9,
    )


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("archive", type=Path)
    cli.add_argument("--output", type=Path)
    args = cli.parse_args()
    with ZipFile(args.archive) as archive:
        names = sorted({p.split("/")[0] for p in archive.namelist()})
        results = {name: cohort(archive, name) for name in names}
    raw = json.dumps(results, indent=2) + "\n"
    if args.output:
        args.output.write_text(raw)
    else:
        print(raw, end="")


if __name__ == "__main__":
    main()
