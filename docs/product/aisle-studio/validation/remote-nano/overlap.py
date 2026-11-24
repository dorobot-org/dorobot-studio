"""Run a Mac observer alongside the active remote foreground in the native fleet check."""

import argparse
import json
from pathlib import Path
import sys
import time
import os
from zipfile import ZipFile
from uuid import uuid4
import hashlib

ROOT = Path(__file__).resolve().parents[5]
sys.path.insert(0, str(ROOT / "tools/aisle_workbench"))
from fleet_artifacts import download
from fleet_client import send, submission
from fleet_worker import initialize


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("native_folder", type=Path)
    p.add_argument("output", type=Path)
    args = p.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)

    def remote():
        s = json.loads((args.native_folder / "ui.json").read_bytes())["state"]["aisle"]
        return s.get("worker_observation") or {}

    view = remote()
    server = view.get("job") or {}
    assert (
        server["state"] == "running" and server["stream"] and view["frame"]["tick"] > 0
    )
    remote_id = server["id"]
    (author,) = (args.native_folder / "state/aisle").glob(
        "*-Nano-prepare/controller.json"
    )
    request = submission(author, mode="observations", ident=str(uuid4()))
    spool = (args.output / "mac-worker").resolve()
    initialize(spool, 1)
    node = dict(checkout=str(ROOT), python=sys.executable, spool=str(spool))
    if os.environ.get("DOROBOT_AISLE_ROOT"):
        node["aisle_root"] = os.environ["DOROBOT_AISLE_ROOT"]
    hello = send(node, dict(op="hello"))
    assert hello["system"] == "Darwin" and hello["backend"] == "metal"
    job = send(node, request)
    samples = []
    ident = job["id"]
    deadline = time.monotonic() + 240
    try:
        while job["state"] in ("reserved", "running", "stopping"):
            if time.monotonic() > deadline:
                raise TimeoutError("Local Nano exceeded the overlap test window")
            job = send(node, dict(op="status", id=ident))
            view = remote()
            if (
                (view.get("job") or {}).get("id") == remote_id
                and view["job"]["state"] == "running"
                and view.get("frame")
                and job["state"] == "running"
            ):
                samples.append(
                    [
                        time.monotonic_ns(),
                        view["frame"]["tick"],
                        (job.get("progress") or {}).get("step", 0),
                    ]
                )
            time.sleep(0.35)
    finally:
        if job["state"] in ("reserved", "running", "stopping"):
            send(node, dict(op="cancel", id=ident))
    assert job["state"] == "completed"
    archive = args.output / "mac-trial.zip"
    receipt = download(send, node, ident, archive)
    with ZipFile(archive) as z:
        result = json.loads(z.read("result.json"))
        source = json.loads(z.read("source.json"))
        assert (
            source["backend"] == "metal"
            and result["task_success"]
            and result["recording"] is False
        )
        assert result["controller_hash"] == request["controller_sha256"]
        assert result["physics_ticks"] == result["wheel_supported_ticks"]
        assert (
            hashlib.sha256(z.read("motor-targets.f64")).hexdigest()
            == result["actuator_audit"]["sha256"]
        )
    active = [r for r in samples if r[2] > 0]
    assert (
        len(active) >= 2
        and active[-1][1] > active[0][1]
        and active[-1][2] > active[0][2]
    )
    start = active[0][0]
    report = dict(
        local_backend="metal",
        remote_backend="webgpu",
        local_job=ident,
        remote_job=remote_id,
        controller_sha256=request["controller_sha256"],
        archive_sha256=receipt["sha256"],
        overlap_s=(active[-1][0] - start) / 1e9,
        samples_elapsed_ms_remote_frame_local_step=[
            [(t - start) // 1_000_000, r, l] for t, r, l in active
        ],
        local_result=dict(
            task_success=result["task_success"],
            ticks=result["physics_ticks"],
            motor_sha256=result["actuator_audit"]["sha256"],
        ),
    )
    (args.output / "overlap.json").write_text(json.dumps(report, indent=2) + "\n")


if __name__ == "__main__":
    main()
