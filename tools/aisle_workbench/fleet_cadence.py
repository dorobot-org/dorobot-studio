"""Check the native Nano viewport with concurrent, independently authored observers."""

from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time
from zipfile import ZipFile

from fleet_client import send, submission
from fleet_artifacts import download
from fleet_worker import initialize
from worker import save

ALIAS = "preview-benchmark"
TASK = "Move the red floor cube onto the coffee table."


def registry(root, checkout, slots):
    spool = root / "benchmark-worker"
    initialize(spool, slots)
    node = dict(python=sys.executable, checkout=str(checkout), spool=str(spool))
    if os.environ.get("DOROBOT_AISLE_ROOT"):
        node["aisle_root"] = os.environ["DOROBOT_AISLE_ROOT"]
    path = root / "benchmark-nodes.json"
    save(path, {ALIAS: node})
    return path


def cancel_benchmark_jobs(root):
    # This registry was created exclusively for this invocation under a new output directory.
    node = json.loads((root / "benchmark-nodes.json").read_bytes())[ALIAS]
    for job in send(node, dict(op="hello"))["active"]:
        send(node, dict(op="cancel", id=job["id"]))


def background(index, node, folder, provider, clock="simulation", period=1):
    author = folder / f"background-author-{index}"
    subprocess.run(
        [
            sys.executable,
            str(Path(__file__).with_name("worker.py")),
            "prepare",
            "--robot",
            "Nano",
            "--provider",
            provider,
            "--task",
            TASK,
            "--out",
            str(author),
        ],
        check=True,
        timeout=180,
    )
    request = submission(author / "controller.json", mode="observations")
    request["observation_period_s"] = period
    if clock == "wall":
        request["observation_clock"] = clock
    ident = request["id"]
    job = send(node, request)
    deadline = time.monotonic() + 450
    samples = []
    progress = []
    last_tick = None
    while job["state"] in ("reserved", "running", "stopping"):
        if time.monotonic() > deadline:
            send(node, dict(op="cancel", id=ident))
            raise TimeoutError(
                f"Background simulation {index} exceeded the test window"
            )
        job = send(node, dict(op="status", id=ident))
        tick = (job.get("progress") or {}).get("step", 0)
        if tick and job["state"] == "running":
            progress.append([tick, time.monotonic_ns()])
        result = send(node, dict(op="observation", id=ident))
        if result["ready"] and result["tick"] != last_tick:
            import base64

            pixels = base64.b64decode(result["data"], validate=True)
            assert len(pixels) == result["width"] * result["height"] * 3
            samples.append(
                [
                    result["tick"],
                    time.monotonic_ns(),
                    hashlib.sha256(pixels).hexdigest(),
                ]
            )
            last_tick = result["tick"]
        time.sleep(0.25)
    assert job["state"] == "completed" and job["result"]["task_success"], job
    path = folder / f"background-{index}.zip"
    archive = download(send, node, ident, path)
    with ZipFile(path) as z:
        motor_digest = hashlib.sha256(z.read("motor-targets.f64")).hexdigest()
        result = json.loads(z.read("result.json"))
    assert (
        result["requested"] == ["red"]
        and result["actuator_audit"]["sha256"] == motor_digest
    )
    assert (
        result["recording"] is False
        and result["wheel_supported_ticks"] == result["physics_ticks"]
    )
    return dict(
        id=ident,
        result=result,
        samples=samples,
        progress=progress,
        archive_sha256=archive["sha256"],
        author=json.loads((author / "provider-metrics.json").read_bytes()),
    )


def while_streaming(args, foreground, done, grab):
    folder = args.out.parent
    node = json.loads((folder / "benchmark-nodes.json").read_bytes())[ALIAS]
    observers = args.foreground_cadence - 1
    with ThreadPoolExecutor(max_workers=observers) as pool:
        jobs = [
            pool.submit(
                background,
                i,
                node,
                folder,
                args.provider,
                args.background_clock,
                args.background_period_s or 1,
            )
            for i in range(observers)
        ]
        grab("foreground")
        row = done(
            lambda w: ((w.get("worker_observation") or {}).get("job") or {}).get("id")
            == foreground
            and w["worker_observation"]["job"]["state"] == "completed",
            "Foreground finished in native Run",
            500,
        )
        backgrounds = [f.result() for f in jobs]
    return row["worker_observation"]["job"]["result"], backgrounds


def verify_viewport(out, trial, backgrounds, clock="simulation", period=1):
    from statistics import median

    times = json.loads((trial / "preview-timing.json").read_bytes())["tick_time"]
    captured = dict(times)
    rows = [
        json.loads(l) for l in (out / "journey-draws.jsonl").read_text().splitlines()
    ]
    draws = [r for r in rows if Path(r["source"]).name.endswith("-fleet-watch")]
    # Returning to Run during evidence import can attach a new observer and
    # display its cached terminal frame. Measure the original live subscription.
    assert draws
    live_source = draws[0]["source"]
    draws = [row for row in draws if row["source"] == live_source]
    assert all(r["tick"] in captured for r in draws)
    assert all(a["tick"] < b["tick"] for a, b in zip(draws, draws[1:]))
    # All simulation workers and the sampling client use the same host monotonic clock.
    begin = max(job["progress"][0][1] for job in backgrounds)
    end = min(job["progress"][-1][1] for job in backgrounds)
    observed = [r for r in draws if begin <= captured[r["tick"]] <= end]
    gaps = [
        (b["elapsed_ns"] - a["elapsed_ns"]) / 1e9
        for a, b in zip(observed, observed[1:])
    ]
    assert len(gaps) > 1
    phase_seconds = sum(gaps)
    fps = len(gaps) / phase_seconds
    background_rates = []
    for job in backgrounds:
        stamps = [stamp for _, stamp, _ in job["samples"] if begin <= stamp <= end]
        background_rates.append(
            1e9 * (len(stamps) - 1) / (stamps[-1] - stamps[0])
            if len(stamps) > 1
            else 0.0
        )
    report = dict(
        sessions=1 + len(backgrounds),
        background_clock=clock,
        background_period_s=period,
        draw_source=Path(live_source).name,
        background_wall_fps=background_rates,
        meets_cadence=phase_seconds >= 30 and fps >= 25 and min(background_rates) >= 1,
        common_physics_window_s=(end - begin) / 1e9,
        draw_window_s=phase_seconds,
        viewport_draws=len(observed),
        viewport_fps=fps,
        median_draw_gap_ms=1000 * median(gaps),
        p95_draw_gap_ms=1000 * sorted(gaps)[int(0.95 * (len(gaps) - 1))],
        maximum_draw_gap_ms=1000 * max(gaps),
        foreground_target_fps=30,
        foreground_total_source_exposures=len(times),
        foreground_total_recorded_frames=json.loads(
            (trial / "result.json").read_bytes()
        )["frames"],
        first_common_tick=observed[0]["tick"],
        last_common_tick=observed[-1]["tick"],
        background_observations=[len(j["samples"]) for j in backgrounds],
    )
    save(out / "preview-cadence.json", report)
    save(out / "background-verdicts.json", backgrounds)
    return report
