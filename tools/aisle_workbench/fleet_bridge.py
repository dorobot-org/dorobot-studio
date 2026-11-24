"""Coordinate Studio's foreground stream and recover submitted jobs across restarts."""

import base64
from contextlib import closing, contextmanager
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import select
import shutil
import sqlite3
import stat
import subprocess
import sys
import tempfile
import time
from uuid import uuid4
from zipfile import ZipFile

from visual_profile import BUNDLE_SHA256, validate_profile
from fleet_artifacts import download
from fleet_client import (
    check_background_period,
    decode_rgb,
    poll_observation,
    send,
    submission,
    worker_command,
)
from fleet_worker import ACTIVE, initialize, job_id
from worker import ROOT, save

PENDING = (*ACTIVE, "submitting")


@contextmanager
def journal(root):
    with closing(sqlite3.connect(root / "fleet.sqlite", timeout=20)) as db:
        db.row_factory = sqlite3.Row
        db.execute("BEGIN IMMEDIATE")
        with db:
            db.execute("""CREATE TABLE IF NOT EXISTS jobs
                (id TEXT PRIMARY KEY, node TEXT NOT NULL, robot TEXT NOT NULL,
                 mode TEXT NOT NULL, request TEXT NOT NULL, state TEXT NOT NULL,
                 trial TEXT NOT NULL UNIQUE)""")
            yield db


def nodes(root):
    path = os.environ.get("DOROBOT_AISLE_WORKERS")
    registered = json.loads(Path(path).read_bytes()) if path else {}
    if "local" in registered:
        raise ValueError("The name local belongs to this Studio machine")
    local = dict(
        python=sys.executable, checkout=str(ROOT), spool=str(root / "local-worker")
    )
    if os.getenv("DOROBOT_AISLE_ROOT"):
        local["aisle_root"] = os.environ["DOROBOT_AISLE_ROOT"]
    return {"local": local, **registered}


def record(root, ident):
    with journal(root) as db:
        row = db.execute("SELECT * FROM jobs WHERE id=?", (job_id(ident),)).fetchone()
    if not row:
        raise ValueError("Select a submitted Studio job")
    return row


def update(root, ident, state):
    with journal(root) as db:
        db.execute(
            "UPDATE jobs SET state=? WHERE id=? AND state IN (?,?,?,?)",
            (state, ident, *PENDING),
        )


def reconcile(root, row, node):
    try:
        info = send(node, dict(op="status", id=row["id"]))
    except ValueError as exc:
        if str(exc) != "Unknown simulation lease" or row["state"] != "submitting":
            raise
        try:
            info = send(node, json.loads(row["request"]))
        except ValueError:
            update(root, row["id"], "rejected")
            raise
    update(root, row["id"], info["state"])
    return info


def submit(root, registry, alias, request):
    if request["mode"] == "stream":
        with journal(root) as db:
            claims = db.execute(
                "SELECT * FROM jobs WHERE mode='stream' AND state IN (?,?,?,?)", PENDING
            ).fetchall()
        for row in claims:
            reconcile(root, row, registry[row["node"]])
    with journal(root) as db:
        if (
            request["mode"] == "stream"
            and db.execute(
                "SELECT 1 FROM jobs WHERE mode='stream' AND state IN (?,?,?,?)", PENDING
            ).fetchone()
        ):
            raise ValueError(
                "A foreground stream is already running in this Studio workspace"
            )
        trial = f"{time.time_ns()}-{request['robot']}-run"
        db.execute(
            "INSERT INTO jobs VALUES(?,?,?,?,?,?,?)",
            (
                request["id"],
                alias,
                request["robot"],
                request["mode"],
                json.dumps(request),
                "submitting",
                trial,
            ),
        )
    # Persist the exact request before sending so a lost SSH response is recoverable.
    try:
        info = send(registry[alias], request)
    except ValueError:
        update(root, request["id"], "rejected")
        raise
    update(root, request["id"], info["state"])
    return info


def verify_wrist_fixture(folder, request, state):
    fixture = request.get("rgb_timing", "normal")
    if state != "completed":
        return
    plan = json.loads(base64.b64decode(request["controller"], validate=True))
    if plan.get("kind") != "table-transfer":
        return
    wants_clearance = plan.get("wrist_clearance", False)
    if fixture == "normal" and not wants_clearance:
        return
    for name, field in [
        ("source.json", "wrist_rgb_timing"),
        ("result.json", "wrist_timing"),
    ]:
        path = folder / name
        record = json.loads(path.read_bytes()) if path.is_file() else {}
        if wants_clearance and record.get("wrist_clearance") is not True:
            raise ValueError(
                f"Worker {name} does not confirm experimental wrist clearance"
            )
        value = record.get(field)
        if field == "wrist_timing":
            value = value.get("fixture") if isinstance(value, dict) else None
        if value != fixture:
            raise ValueError(
                f"Worker {name} does not confirm the requested {fixture} wrist delivery"
            )


def verify_visual_profile(folder, request, state):
    if state != "completed":
        return
    expected = request.get("visual_profile", "original")
    for name in ("source.json", "result.json"):
        path = folder / name
        record = json.loads(path.read_bytes()) if path.is_file() else {}
        if record.get("visual_profile", "original") != expected:
            raise ValueError("Recorded Nano camera profile differs from the submission")
        if (
            name == "source.json"
            and expected != "original"
            and record.get("visual_bundle_sha256") != BUNDLE_SHA256
        ):
            raise ValueError(
                "Recorded Nano camera geometry differs from the selected bundle"
            )


def import_trial(root, node, row, status):
    request = json.loads(row["request"])
    dest = root / row["trial"]
    if dest.is_dir():
        receipt = json.loads((dest / "worker-origin.json").read_bytes())
        if receipt["job"] != row["id"]:
            raise ValueError("Trial directory belongs to another worker job")
        verify_visual_profile(dest, request, receipt["state"])
        verify_wrist_fixture(dest, request, receipt["state"])
        return dest
    with tempfile.TemporaryDirectory(dir=root) as tmp:
        folder = Path(tmp) / "trial"
        folder.mkdir()
        meta = download(send, node, row["id"], Path(tmp) / "download.zip")
        with ZipFile(meta["output"]) as bundle:
            entries = bundle.infolist()
            if (
                len(entries) > 200000
                or sum(f.file_size for f in entries) > 16 * 1024**3
            ):
                raise ValueError("Trial exceeds Studio import limits")
            for member in entries:
                name = PurePosixPath(member.filename)
                if (
                    name.is_absolute()
                    or ".." in name.parts
                    or "\\" in member.filename
                    or stat.S_ISLNK(member.external_attr >> 16)
                ):
                    raise ValueError("Invalid trial archive entry")
                target = folder / name
                if member.is_dir():
                    target.mkdir(parents=True, exist_ok=True)
                    continue
                target.parent.mkdir(parents=True, exist_ok=True)
                with bundle.open(member) as source, target.open("xb") as out:
                    shutil.copyfileobj(source, out)
        # Early cancellation can precede the worker writing its plan to the trial.
        controller = folder / "controller.json"
        if not controller.is_file() and status["state"] in ("cancelled", "failed"):
            controller.write_bytes(
                base64.b64decode(request["controller"], validate=True)
            )
        plan = json.loads(controller.read_bytes())

        if (
            plan.get("robot") != row["robot"]
            or hashlib.sha256((folder / "controller.json").read_bytes()).hexdigest()
            != json.loads(row["request"])["controller_sha256"]
        ):
            raise ValueError("Evidence belongs to a different controller or robot")
        if status["state"] == "cancelled":
            save(folder / "stopped.json", dict(stopped=True))
        elif status["state"] == "failed" and not (folder / "error.json").exists():
            save(
                folder / "error.json",
                dict(
                    message=status.get("message")
                    or "Worker failed before producing a verdict"
                ),
            )
        verify_visual_profile(folder, request, status["state"])
        verify_wrist_fixture(folder, request, status["state"])
        save(
            folder / "worker-origin.json",
            dict(
                job=row["id"],
                node=row["node"],
                state=status["state"],
                sha256=meta["sha256"],
            ),
        )
        # A second Studio may have fetched the same immutable trial concurrently.
        with journal(root):
            if not dest.exists():
                folder.rename(dest)
    return dest


def save_frame(reply, out):
    if not reply["ready"]:
        return None
    data = decode_rgb(reply)
    tmp = out / "preview.tmp"
    tmp.write_bytes(reply["tick"].to_bytes(4, "big") + data)
    tmp.replace(out / "preview.rgb")
    return dict(
        tick=reply["tick"],
        height=reply["height"],
        sha256=hashlib.sha256(data).hexdigest(),
    )


def foreground_packets(pipe, silence_s=10):
    """Decode bounded packets and time out even if a peer stops halfway through one."""
    buf = bytearray()
    header_limit = 128 * 1024

    def read_more():
        if not select.select([pipe], [], [], silence_s)[0]:
            raise TimeoutError(
                "Foreground RGB stream stalled. Refresh the worker to reconnect."
            )
        chunk = os.read(pipe.fileno(), 65536)
        buf.extend(chunk)
        return bool(chunk)

    while True:
        boundary = buf.find(b"\n")
        while boundary < 0:
            if len(buf) >= header_limit:
                raise ValueError("Invalid foreground stream header")
            if not read_more():
                if buf:
                    raise ValueError("Incomplete foreground stream header")
                return
            boundary = buf.find(b"\n")
        if boundary >= header_limit:
            raise ValueError("Invalid foreground stream header")
        packet = json.loads(buf[:boundary])
        del buf[: boundary + 1]
        size = packet["bytes"]
        if type(size) is not int or size not in (
            0,
            4 + 480 * 320 * 3,
            4 + 480 * 360 * 3,
        ):
            raise ValueError("Invalid foreground frame size")
        while len(buf) < size:
            if not read_more():
                raise ValueError("Incomplete foreground RGB frame")
        raw = bytes(buf[:size])
        del buf[:size]
        yield packet, raw


def watch_foreground(node, ident, out, root):
    job, frame = None, None
    proc = subprocess.Popen(
        worker_command(node, "watch-rgb", ident), stdout=subprocess.PIPE
    )
    try:
        for packet, raw in foreground_packets(proc.stdout):
            if "job" in packet:
                new_job = packet["job"]
                if new_job.get("id") != ident:
                    raise ValueError("Foreground stream returned a different job")
                if not job or new_job["state"] != job["state"]:
                    update(root, ident, new_job["state"])
                job = new_job
            if not job:
                raise ValueError("Missing foreground job identity")
            if raw:
                tmp = out / "preview.tmp"
                tmp.write_bytes(raw)
                tmp.replace(out / "preview.rgb")
                frame = dict(
                    tick=int.from_bytes(raw[:4], "big"),
                    height=(len(raw) - 4) // 1440,
                    sha256=hashlib.sha256(memoryview(raw)[4:]).hexdigest(),
                )
            save(out / "watch.json", dict(job=job, frame=frame))
        if proc.wait(timeout=5) or not job or job["state"] in ACTIVE:
            raise ValueError(
                "Foreground RGB stream disconnected. Refresh the worker to reconnect."
            )
    finally:
        proc.stdout.close()
        if proc.poll() is None:
            proc.terminate()
            try:
                proc.wait(timeout=3)
            except subprocess.TimeoutExpired:
                proc.kill()
                proc.wait()


def operate(args):
    root = args.out.parent
    registry = nodes(root)
    alias = args.node or "local"
    if alias not in registry:
        raise ValueError("Worker name is absent from DOROBOT_AISLE_WORKERS")
    node = registry[alias]
    if alias == "local":
        initialize(Path(node["spool"]), 2)
    ident = args.job_id
    op = args.fleet_action
    if op == "watch":
        row = record(root, ident)
        if row["node"] != alias:
            raise ValueError("This job belongs to another node")
        if (
            row["mode"] == "stream"
            and send(node, dict(op="hello")).get("foreground_rgb_protocol") == 1
        ):
            return watch_foreground(node, ident, args.out, root)
        while True:
            info = reconcile(root, row, node)
            frame = None
            if row["mode"] == "stream":
                frame = save_frame(send(node, dict(op="frame", id=ident)), args.out)
            save(args.out / "watch.json", dict(job=info, frame=frame))
            if info["state"] not in ACTIVE:
                return
            time.sleep(0.25 if row["mode"] == "stream" else 1)
    if op in ("stream", "observations"):
        if not args.controller:
            raise ValueError("Prepare a controller before submitting a job")
        req = submission(
            args.controller,
            op,
            args.seed,
            args.fixture,
            str(uuid4()),
            args.room_revision,
        )
        expected = getattr(args, "controller_sha256", None)
        if expected is not None and req["controller_sha256"] != expected:
            raise ValueError(
                "Selected controller contents changed before fleet submission"
            )
        if args.room_hash and req["room_sha256"] != args.room_hash:
            raise ValueError("Selected room revision has changed")
        if op == "observations":
            clock = getattr(args, "observation_clock", "simulation")
            check_background_period(node, args.observation_period_s, clock)
            req["observation_period_s"] = args.observation_period_s
            if clock == "wall":
                req["observation_clock"] = clock
        plan = json.loads(base64.b64decode(req["controller"], validate=True))
        visual_profile = getattr(args, "visual_profile", "original")
        validate_profile(visual_profile, plan)
        wants_clearance = plan.get("wrist_clearance", False)
        if plan.get("kind") == "table-transfer" and (
            args.rgb_timing != "normal"
            or wants_clearance
            or visual_profile != "original"
        ):
            capabilities = send(node, {"op": "hello"})
            if visual_profile != "original":
                if (
                    capabilities.get("nano_visual_bundles", {}).get(visual_profile)
                    != BUNDLE_SHA256
                ):
                    raise ValueError(
                        "Update the selected worker to install matching Nano compact visuals"
                    )
                req["visual_profile"] = visual_profile
            if (
                wants_clearance
                and capabilities.get("experimental_wrist_clearance") is not True
            ):
                raise ValueError(
                    "Update the selected worker to use experimental wrist clearance"
                )
            if args.rgb_timing != "normal" and args.rgb_timing not in capabilities.get(
                "transfer_wrist_timing", []
            ):
                raise ValueError(
                    "Update the selected worker to test wrist timing during a pick"
                )
        req.update(rgb_timing=args.rgb_timing, target_motion=args.target_motion)
        ident = submit(root, registry, alias, req)["id"]
    with journal(root) as db:
        jobs = db.execute(
            "SELECT id, state FROM jobs WHERE node=? AND state!='rejected' ORDER BY rowid",
            (alias,),
        ).fetchall()
    ids = [item["id"] for item in jobs]
    if ident not in ids:
        ident = ids[-1] if ids else None
    if op in ("previous", "next") and ident:
        i = ids.index(ident) + (1 if op == "next" else -1)
        ident = ids[max(0, min(len(ids) - 1, i))]
    info, trial, sample = None, None, None
    if ident:
        row = record(root, ident)
        if op == "sample":
            if row["state"] == "submitting":
                reconcile(root, row, node)
            info, frame = poll_observation(node, ident)
            update(root, ident, info["state"])
            sample = save_frame(frame, args.out)
            if sample is None:
                raise ValueError("Waiting for the first background RGB exposure")
        else:
            info = reconcile(root, row, node)
        if op == "cancel":
            info = send(node, dict(op="cancel", id=ident))
            update(root, ident, info["state"])
        if op == "fetch":
            trial = import_trial(root, node, row, info).name
        info.update(robot=row["robot"], mode=row["mode"])
        if row["mode"] == "observations":
            request = json.loads(row["request"])
            info["observation_period_s"] = request.get("observation_period_s", 1)
            info["observation_clock"] = request.get("observation_clock", "simulation")
    capabilities = send(node, dict(op="hello"))
    return dict(
        node=alias,
        capabilities=capabilities,
        jobs=ids,
        selected=info,
        imported=trial,
        sample=sample,
    )
