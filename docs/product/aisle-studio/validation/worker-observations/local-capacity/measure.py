"""Compare complete Nano sessions through the local Studio worker RPC."""

import argparse, base64, hashlib, json, os, subprocess, sys, time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from zipfile import ZipFile

p = argparse.ArgumentParser()
p.add_argument("checkout", type=Path)
p.add_argument("out", type=Path)
p.add_argument("--sessions", type=int, choices=(1, 4), required=True)
args = p.parse_args()
repo = args.checkout.resolve()
out = args.out.resolve()
out.mkdir(exist_ok=False, parents=True)
wb = repo / "tools/aisle_workbench"
sys.path.insert(0, str(wb))
from fleet_client import send, submission
from fleet_artifacts import download


def save(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def load(path):
    return json.loads(path.read_bytes())


def worker(*cmd):
    subprocess.run(
        [sys.executable, str(wb / "worker.py"), *cmd],
        check=True,
        capture_output=True,
        text=True,
    )


spool = out / "spool"
subprocess.run(
    [
        sys.executable,
        str(wb / "fleet_worker.py"),
        "--root",
        str(spool),
        "init",
        "--slots",
        str(args.sessions),
    ],
    check=True,
    capture_output=True,
)
node = dict(
    checkout=str(repo),
    python=sys.executable,
    aisle_root=os.environ["DOROBOT_AISLE_ROOT"],
    spool=str(spool),
)
hello = send(node, dict(op="hello"))
request = "Move the red floor cube onto the coffee table."
start = time.monotonic()
epoch = time.time()


def launch(i):
    d = out / f"author-{i}"
    worker(
        "prepare",
        "--robot",
        "Nano",
        "--provider",
        "octos-deepseek",
        "--task",
        request,
        "--out",
        str(d),
    )
    plan = load(d / "controller.json")
    assert plan["color_order"] == "red"
    req = submission(d / "controller.json", mode="stream" if i == 0 else "observations")
    req["observation_period_s"] = 1
    submit_start = time.monotonic() - start
    ack = send(node, req)
    return dict(
        index=i,
        id=ack["id"],
        mode=req["mode"],
        provider=load(d / "provider-metrics.json"),
        submitted_s=submit_start,
        ack_s=time.monotonic() - start,
    )


def capture(item, bg=False):
    r = send(node, dict(op="observation" if bg else "frame", id=item["id"]))
    if not r["ready"]:
        return
    buf = base64.b64decode(r.pop("data"), validate=True)
    assert len(buf) == r["width"] * r["height"] * 3
    return dict(
        tick=r["tick"],
        seen_s=time.monotonic() - start,
        sha256=hashlib.sha256(buf).hexdigest(),
    )


def track(item):
    samples = []
    status_log = []
    last = None
    next_status = 0
    terminal = None
    while time.monotonic() - start < 600:
        now = time.monotonic()
        if now >= next_status:
            row = send(node, dict(op="status", id=item["id"]))
            status_log.append(
                dict(
                    t_s=now - start,
                    state=row["state"],
                    tick=(row.get("progress") or {}).get("step"),
                )
            )
            next_status = now + 1
            if row["state"] not in ("reserved", "running"):
                terminal = row
                break
        sample = capture(item, item["mode"] == "observations")
        if sample and sample["tick"] != last:
            samples.append(sample)
            last = sample["tick"]
        time.sleep(0.12 if item["mode"] == "observations" else 0.015)
    if not terminal or terminal["state"] != "completed":
        raise ValueError((item, terminal))
    if not terminal["result"]["task_success"]:
        raise ValueError(terminal["result"])
    archive = out / (item["id"] + ".zip")
    meter = download(send, node, item["id"], archive)
    with ZipFile(archive) as z:
        result = json.loads(z.read("result.json"))
    audit = result["actuator_audit"]
    assert (
        audit["passed"] and result["wheel_supported_ticks"] == result["physics_ticks"]
    )
    return dict(
        **item,
        created_wall_s=terminal["created"] - epoch,
        ended_wall_s=terminal["ended"] - epoch,
        verified_archive_sha256=meter["sha256"],
        trial=result,
        samples=samples,
        status=status_log,
    )


with ThreadPoolExecutor(max_workers=args.sessions * 2) as pool:
    # Subscribe to each worker as soon as its own author and submit finish.
    import concurrent.futures as futures

    tracks = []
    for result in futures.as_completed(
        [pool.submit(launch, i) for i in range(args.sessions)]
    ):
        tracks.append(pool.submit(track, result.result()))
    rows = sorted((f.result() for f in tracks), key=lambda r: r["index"])
simulation_end = max(r["ended_wall_s"] for r in rows)
overlap_start = max(r["created_wall_s"] for r in rows)
overlap_end = min(r["ended_wall_s"] for r in rows)
samples = [p for p in rows[0]["samples"] if overlap_start <= p["seen_s"] <= overlap_end]
fps = (
    (len(samples) - 1) / (samples[-1]["seen_s"] - samples[0]["seen_s"])
    if len(samples) > 1
    else 0
)
fore = rows[0]
worker(
    "export",
    "--robot",
    "Nano",
    "--trial",
    str(spool / fore["id"] / "trial"),
    "--out",
    str(out / "export"),
)
videos = sorted((out / "export/dataset").rglob("*.mp4"))
frames = []
for v in videos:
    info = json.loads(
        subprocess.check_output(
            [
                "ffprobe",
                "-v",
                "error",
                "-count_frames",
                "-show_entries",
                "stream=nb_read_frames",
                "-of",
                "json",
                str(v),
            ]
        )
    )
    frames.append(int(info["streams"][0]["nb_read_frames"]))
assert len(videos) == 3 and frames == [fore["trial"]["frames"]] * 3
report = dict(
    studio_commit=subprocess.check_output(
        ["git", "-C", str(repo), "rev-parse", "HEAD"], text=True
    ).strip(),
    host=subprocess.check_output(
        ["sysctl", "-n", "machdep.cpu.brand_string"], text=True
    ).strip(),
    memory_bytes=int(subprocess.check_output(["sysctl", "-n", "hw.memsize"])),
    worker=hello,
    sessions=args.sessions,
    overlap_s=overlap_end - overlap_start,
    foreground_distinct_frames_during_overlap=len(samples),
    foreground_delivered_fps=fps,
    wall_s_to_all_verdicts=simulation_end,
    wall_s_with_fetch_and_export=time.monotonic() - start,
    exported_frames_per_camera=frames,
    jobs=rows,
)
save(out / "measurement.json", report)
print(
    json.dumps(
        {
            k: report[k]
            for k in (
                "sessions",
                "overlap_s",
                "foreground_delivered_fps",
                "wall_s_to_all_verdicts",
                "wall_s_with_fetch_and_export",
                "exported_frames_per_camera",
            )
        }
    ),
    flush=True,
)
