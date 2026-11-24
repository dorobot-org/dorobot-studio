"""Recompute physical verdicts, simultaneous viewport cadence and RGB delivery from a published ZIP."""

import argparse
import hashlib
import io
import math
import subprocess
import tempfile
import json
from pathlib import Path
from statistics import median
from zipfile import ZipFile


def check_dataset(read, base, features, fps, frames):
    import pyarrow.parquet as pq

    data = pq.read_table(
        io.BytesIO(read(base + "data/chunk-000/episode_000000.parquet"))
    )
    assert data.num_rows == frames
    assert data["frame_index"].to_pylist() == list(range(frames))
    assert set(data["episode_index"].to_pylist()) == {0}
    times = data["timestamp"].to_pylist()
    assert all(abs(t - i / fps) < 1e-5 for i, t in enumerate(times))
    for field in ("observation.state", "action"):
        (width,) = features[field]["shape"]
        assert width == 17 and len(features[field]["names"]) == width
        assert all(
            len(row) == width and all(math.isfinite(v) for v in row)
            for row in data[field].to_pylist()
        )
    for camera in ("head", "left_wrist", "right_wrist"):
        with tempfile.NamedTemporaryFile(suffix=".mp4") as video:
            video.write(
                read(
                    base
                    + f"videos/chunk-000/observation.images.{camera}/episode_000000.mp4"
                )
            )
            video.flush()
            proc = subprocess.run(
                [
                    "ffprobe",
                    "-v",
                    "error",
                    "-select_streams",
                    "v:0",
                    "-show_frames",
                    "-show_entries",
                    "frame=width,height,best_effort_timestamp_time",
                    "-of",
                    "json",
                    video.name,
                ],
                check=True,
                capture_output=True,
            )
            assert not proc.stderr, proc.stderr
            decoded = json.loads(proc.stdout)["frames"]
            assert len(decoded) == frames
            assert all(
                (f["width"], f["height"]) == (480, 360)
                and abs(float(f["best_effort_timestamp_time"]) - t) < 1e-5
                for f, t in zip(decoded, times)
            )


def cohort(archive, name, datasets=False):
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
            rgb_clock = delivery.get("observation_clock", "simulation")
            period = delivery.get("observation_period_s", 1)
            assert source.get("observation_clock", "simulation") == rgb_clock
            assert source.get("observation_period_s", 1) == period
            if rgb_clock == "simulation":
                assert period in (1, 2, 5)
                step = int(100 * period)
                assert list(images) == list(range(step, row["ticks"] + 1, step))
            else:
                assert rgb_clock == "wall" and period in (0.5, 1, 2, 5)
                ticks = [e["tick"] for e in exposures]
                stamps = [e["published_monotonic_ns"] for e in exposures]
                assert len(ticks) >= 2 and 0 <= ticks[0] <= ticks[-1] <= row["ticks"]
                assert all(a < b for a, b in zip(ticks, ticks[1:]))
                assert all(a < b for a, b in zip(stamps, stamps[1:]))
                row.update(
                    observation_clock=rgb_clock,
                    observation_period_s=period,
                    worker_publication_hz=(len(stamps) - 1)
                    * 1e9
                    / (stamps[-1] - stamps[0]),
                )
            assert all(e["width"] == 480 and e["height"] == 360 for e in exposures)
            assert all(images[tick] == digest for tick, _, digest in samples)
            last = read(base + "preview.rgb")
            last_tick = int.from_bytes(last[:4], "big")
            assert len(last) == 4 + 480 * 360 * 3 and samples[-1][0] == last_tick
            assert (
                hashlib.sha256(last[4:]).hexdigest()
                == images[last_tick]
                == samples[-1][2]
            )
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
            assert (
                load(base + "recovery.json")[-1]["check"]
                == "Imported worker trial and export recovered"
            )
            info = load(base + "dataset/meta/info.json")
            assert (
                info["total_frames"] == verdict["frames"]
                and info["total_episodes"] == 1
            )
            row.update(
                dataset_frames=info["total_frames"],
                recorded_cameras=[k for k in info["features"] if "images." in k],
            )
            cameras = {"head", "left_wrist", "right_wrist"}
            assert set(row["recorded_cameras"]) == {
                "observation.images." + cam for cam in cameras
            }
            metadata = load(base + "dataset/meta/aisle.json")
            assert (
                metadata["controller_hash"] == hashed
                and metadata["model_hash"] == verdict["model_hash"]
            )
            assert (
                set(metadata["onboard_cameras"])
                == set(metadata["calibration"])
                == cameras
            )
            for calibration in metadata["calibration"].values():
                assert calibration["parent_link"] and calibration["resolution"] == [
                    480,
                    360,
                ]
                for key, size in (("intrinsics", 3), ("link_from_camera_gl", 4)):
                    matrix = calibration[key]
                    assert len(matrix) == size and all(
                        len(r) == size and all(math.isfinite(v) for v in r)
                        for r in matrix
                    )
            if datasets:
                check_dataset(
                    read,
                    base + "dataset/",
                    info["features"],
                    info["fps"],
                    info["total_frames"],
                )
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
    assert len({process[1] for process in compute}) == 8 and {
        int(process[0]) for process in compute
    } == set(range(8))
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
    cli.add_argument(
        "--datasets",
        action="store_true",
        help="Decode all six videos and verify frame time alignment with the Parquet using FFprobe and PyArrow",
    )
    args = cli.parse_args()
    with ZipFile(args.archive) as archive:
        names = sorted({p.split("/")[0] for p in archive.namelist()})
        results = {name: cohort(archive, name, args.datasets) for name in names}
    raw = json.dumps(results, indent=2) + "\n"
    if args.output:
        args.output.write_text(raw)
    else:
        print(raw, end="")


if __name__ == "__main__":
    main()
