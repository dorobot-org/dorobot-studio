"""Run durable AISLE simulation leases for one local or SSH-connected Studio user."""

import argparse
import base64
from contextlib import ExitStack, closing, contextmanager
import fcntl
import hashlib
from importlib.metadata import version
import json
import os
from pathlib import Path
import platform
import secrets
import signal
import sqlite3
import subprocess
import sys
import time
from uuid import UUID

from worker import describe, digest, validate

HERE = Path(__file__).resolve().parent
ACTIVE = ("reserved", "running", "stopping")
LEASE_SECONDS = 15
MAX_REQUEST = 16 * 1024 * 1024
BUNDLE_PREFIX = b"AISLE-FLEET-1\n"
ROOM_FILES = {"room.json", "survey.json", "reference-image", "room-preview.rgb"}


def companion_names(plan):
    from onboard_scene import PACKET, CALIBRATION, SUMMARY, PREVIEW

    names = set()
    if "scene_observation" in plan:
        files = set(plan["scene_observation"]["files"])
        if files not in (
            {PACKET, CALIBRATION, SUMMARY},
            {PACKET, CALIBRATION, SUMMARY, PREVIEW},
        ):
            raise ValueError("Unknown onboard scene files")
        names.update(files)
    if "reach_checkpoint" in plan:
        names.update(("training.json", "reach-checkpoint.json"))
    if "grasp_checkpoint" in plan:
        names.update(("training.json", "grasp-checkpoint.json"))
    return names


SCHEMA = """
CREATE TABLE settings(slots INTEGER NOT NULL);
CREATE TABLE jobs(id TEXT PRIMARY KEY, request TEXT NOT NULL, controller BLOB NOT NULL, token TEXT NOT NULL,
 state TEXT NOT NULL, deadline REAL NOT NULL, stream INTEGER NOT NULL, created REAL NOT NULL,
 ended REAL, message TEXT);
"""


@contextmanager
def transaction(root):
    with closing(
        sqlite3.connect(
            (root / "leases.sqlite").resolve().as_uri() + "?mode=rw",
            uri=True,
            timeout=15,
        )
    ) as db:
        db.row_factory = sqlite3.Row
        db.execute("BEGIN IMMEDIATE")
        db.execute(
            "UPDATE jobs SET state='lost', ended=?, message='Worker lease expired' WHERE state IN (?,?,?) AND deadline<?",
            (time.time(), *ACTIVE, time.time()),
        )
        with db:
            yield db


def initialize(root, slots):
    if not 1 <= slots <= 64:
        raise ValueError("Choose a slot count between 1 and 64")
    root.mkdir(parents=True, exist_ok=True, mode=0o700)
    with closing(sqlite3.connect(root / "leases.sqlite", isolation_level=None)) as db:
        db.execute("BEGIN IMMEDIATE")
        if not db.execute(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name='settings'"
        ).fetchone():
            for statement in SCHEMA.strip().split(";"):
                if statement.strip():
                    db.execute(statement)
            db.execute("INSERT INTO settings VALUES(?)", (slots,))
        elif db.execute("SELECT slots FROM settings").fetchone()[0] != slots:
            raise ValueError("Worker already initialized with a different slot count")
        db.commit()


def job_id(value):
    if not isinstance(value, str) or str(UUID(value)) != value:
        raise ValueError("Use a canonical UUID as the simulation job ID")
    return value


def payload(request):
    source = base64.b64decode(request["controller"], validate=True)
    if (
        not 0 < len(source) <= 1024 * 1024
        or hashlib.sha256(source).hexdigest() != request["controller_sha256"]
    ):
        raise ValueError("The submitted controller bytes differ from their digest")
    plan = json.loads(source)
    profile = describe(request["robot"])
    if (
        plan.get("robot") != profile["robot"]
        or plan.get("model_hash") != profile["model_hash"]
    ):
        raise ValueError("Simulation controller and worker robot differ")
    files = request.get("files", {})
    names = companion_names(plan)
    room_hash = request.get("room_sha256")
    if room_hash is not None:
        if plan.get("kind") not in ("table-transfer", "visual-approach"):
            raise ValueError("Room revisions require a mobile Nano simulation")
        names.update("room/" + n for n in ROOM_FILES)
    if not isinstance(files, dict) or set(files) != names:
        raise ValueError("Selected controller or room files are incomplete")
    hashes = {}
    for name, encoded in files.items():
        blob = base64.b64decode(encoded, validate=True)
        hashes[name] = hashlib.sha256(blob).hexdigest()
        if (
            name in plan.get("scene_observation", {}).get("files", {})
            and hashes[name] != plan["scene_observation"]["files"][name]
        ):
            raise ValueError("The captured observation file has changed")
    if (room_hash is not None and hashes["room/room.json"] != room_hash) or (
        "reach_checkpoint" in plan
        and hashes["reach-checkpoint.json"] != plan["reach_checkpoint"]
    ):
        raise ValueError("The selected revision or checkpoint bytes have changed")
    if "grasp_checkpoint" in plan and (
        hashes["grasp-checkpoint.json"] != plan["grasp_checkpoint"]
        or hashes["training.json"] != plan["grasp_estimator"]["training_sha256"]
    ):
        raise ValueError("The selected grasp checkpoint or its training record changed")
    period = request.get("observation_period_s", 1)
    if (
        type(period) is not int
        or period not in (1, 2, 5)
        or (period != 1 and request["mode"] != "observations")
    ):
        raise ValueError(
            "Select a background RGB observation period of 1, 2 or 5 simulated seconds"
        )
    control = validate(plan, profile)
    if request.get("rgb_timing", "normal") not in (
        "normal",
        "intermittent",
        "disconnected",
    ) or request.get("target_motion", "stationary") not in (
        "stationary",
        "slide-positive",
        "slide-negative",
    ):
        raise ValueError("Unknown simulation camera timing or target motion")
    if (
        request.get("mode") not in ("stream", "observations")
        or type(request.get("seed")) is not int
        or not 0 <= request["seed"] < 2**32
        or request.get("fixture") not in ("standard", "compliant", "impact")
    ):
        raise ValueError(
            "Choose the simulation mode, nonnegative layout and a supported fixture"
        )
    if request["fixture"] == "impact" and control.get("kind") != "standing-inspection":
        raise ValueError("Impact requires a G1 standing controller")
    if files:
        source = (
            BUNDLE_PREFIX
            + json.dumps(dict(controller=request["controller"], files=files)).encode()
        )
    return source, hashes


def status(root, ident):
    with transaction(root) as db:
        record = db.execute(
            "SELECT id,state,created,ended,stream,message FROM jobs WHERE id=?",
            (ident,),
        ).fetchone()
    if record is None:
        raise ValueError("Unknown simulation lease")
    row = dict(record)
    trial = root / ident / "trial"
    for key, filename in (("progress", "progress.json"), ("result", "result.json")):
        if (trial / filename).is_file():
            row[key] = json.loads((trial / filename).read_bytes())
    return row


def submit(root, request):
    ident = job_id(request["id"])
    raw, hashes = payload(request)
    config = {
        k: request[k] for k in ("controller_sha256", "robot", "mode", "seed", "fixture")
    }
    for key, default in (("rgb_timing", "normal"), ("target_motion", "stationary")):
        if request.get(key, default) != default:
            config[key] = request[key]
    if request.get("observation_period_s", 1) != 1:
        config["observation_period_s"] = request["observation_period_s"]
    if hashes:
        config["files_sha256"] = hashes
    if request.get("room_sha256"):
        config["room_sha256"] = request["room_sha256"]
    fingerprint = json.dumps(config, sort_keys=True)
    token = secrets.token_hex(24)
    now = time.time()
    with transaction(root) as db:
        old = db.execute("SELECT request FROM jobs WHERE id=?", (ident,)).fetchone()
        if old:
            if old[0] != fingerprint:
                raise ValueError("This job ID already identifies a different request")
        else:
            occupied, streams = db.execute(
                "SELECT COUNT(*), COALESCE(SUM(stream),0) FROM jobs WHERE state IN (?,?,?)",
                ACTIVE,
            ).fetchone()
            if occupied >= db.execute("SELECT slots FROM settings").fetchone()[0]:
                raise ValueError("All simulation slots are occupied")
            if request["mode"] == "stream" and streams:
                raise ValueError("This worker already has a foreground stream")
            db.execute(
                "INSERT INTO jobs VALUES(?,?,?,?,'reserved',?,?,?,NULL,NULL)",
                (
                    ident,
                    fingerprint,
                    raw,
                    token,
                    now + LEASE_SECONDS,
                    int(request["mode"] == "stream"),
                    now,
                ),
            )
    if not old:
        try:
            subprocess.Popen(
                [
                    sys.executable,
                    str(Path(__file__).resolve()),
                    "--root",
                    str(root),
                    "_supervise",
                    ident,
                    token,
                ],
                stdin=subprocess.DEVNULL,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                start_new_session=True,
            )
        except OSError as exc:
            with transaction(root) as db:
                db.execute(
                    "UPDATE jobs SET state='failed',message=?,ended=? WHERE id=? AND state='reserved'",
                    (str(exc), time.time(), ident),
                )
    return status(root, ident)


def supervise(root, ident, token):
    process = None
    with transaction(root) as db:
        row = db.execute(
            "SELECT request,controller,state FROM jobs WHERE id=? AND token=?",
            (ident, token),
        ).fetchone()
        if not row or row["state"] not in ("reserved", "stopping"):
            return
        if row["state"] == "stopping":
            db.execute(
                "UPDATE jobs SET state='cancelled',ended=? WHERE id=?",
                (time.time(), ident),
            )
            return
        config = json.loads(row["request"])
        total_slots = db.execute("SELECT slots FROM settings").fetchone()[0]
        db.execute(
            "UPDATE jobs SET state='running',deadline=? WHERE id=?",
            (time.time() + LEASE_SECONDS, ident),
        )
    state, message = "failed", "Simulation worker failed"
    locks = ExitStack()
    try:
        directory = root / ident
        directory.mkdir()
        source = row["controller"]
        if source.startswith(BUNDLE_PREFIX):
            bundle = json.loads(source[len(BUNDLE_PREFIX) :])
            source = base64.b64decode(bundle["controller"], validate=True)
            for name, encoded in bundle["files"].items():
                path = directory / name
                path.parent.mkdir(exist_ok=True)
                path.write_bytes(base64.b64decode(encoded, validate=True))
        (directory / "controller.json").write_bytes(source)
        handles = []
        for slot in range(total_slots):
            handle = (root / f"slot-{slot}.lock").open("a")
            try:
                fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                handle.close()
                continue
            handles.append(locks.enter_context(handle))
            break
        if not handles:
            raise ValueError("All physical slots are still occupied")
        if config["mode"] == "stream":
            handle = locks.enter_context((root / "foreground.lock").open("a"))
            fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
            handles.append(handle)
        trial = directory / "trial"
        trial.mkdir()
        args = [
            sys.executable,
            str(HERE / "worker.py"),
            "run",
            "--reserved",
            "--robot",
            config["robot"],
            "--controller",
            str(directory / "controller.json"),
            "--seed",
            str(config["seed"]),
            "--fixture",
            config["fixture"],
            "--out",
            str(trial),
        ]
        for key in ("rgb_timing", "target_motion"):
            if key in config:
                args.extend(("--" + key.replace("_", "-"), config[key]))
        if config.get("room_sha256"):
            args.extend(
                [
                    "--room-revision",
                    str(directory / "room/room.json"),
                    "--room-hash",
                    config["room_sha256"],
                ]
            )
        if config["mode"] == "stream":
            args.append("--live-preview")
        if config["mode"] == "observations":
            args.extend(
                (
                    "--observations-only",
                    "--observation-period-s",
                    str(config.get("observation_period_s", 1)),
                )
            )
        with (trial / "worker.log").open("xb") as log:
            process = subprocess.Popen(
                args,
                pass_fds=tuple(h.fileno() for h in handles),
                stdin=subprocess.PIPE,
                stdout=log,
                stderr=subprocess.STDOUT,
                start_new_session=True,
            )
            while True:
                with transaction(root) as db:
                    active = db.execute(
                        "SELECT state FROM jobs WHERE id=? AND token=?", (ident, token)
                    ).fetchone()
                    if not active or active[0] != "running":
                        state, message = "cancelled", "Cancelled by operator"
                        break
                    db.execute(
                        "UPDATE jobs SET deadline=? WHERE id=?",
                        (time.time() + LEASE_SECONDS, ident),
                    )
                if process.poll() is not None:
                    if process.returncode == 0 and (trial / "result.json").is_file():
                        state, message = "completed", None
                    elif (trial / "error.json").is_file():
                        message = json.loads((trial / "error.json").read_bytes()).get(
                            "message", message
                        )
                    break
                time.sleep(0.25)
    except Exception as exc:
        message = str(exc)
    finally:
        if process is not None:
            process.stdin.close()
            if process.poll() is None:
                try:
                    process.wait(timeout=3)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()
        locks.close()
        with transaction(root) as db:
            db.execute(
                "UPDATE jobs SET state=CASE WHEN state='stopping' THEN 'cancelled' ELSE ? END,message=?,ended=? WHERE id=? AND token=? AND state IN ('running','stopping')",
                (state, message, time.time(), ident, token),
            )


def dispatch(root, request):
    if not isinstance(request, dict):
        raise ValueError("The simulation request must be an object")
    op = request.get("op")
    if op == "hello":
        from aisle.sim import nexus_backend as nx

        with transaction(root) as db:
            slots = db.execute("SELECT slots FROM settings").fetchone()[0]
            active = db.execute(
                "SELECT id,state,stream FROM jobs WHERE state IN (?,?,?)", ACTIVE
            ).fetchall()
        robots = {}
        for robot in ("Nano", "G1"):
            try:
                robots[robot] = describe(robot)["model_hash"]
            except (ValueError, OSError):
                continue
        return dict(
            protocol=1,
            foreground_rgb_protocol=1,
            background_rgb_periods_s=[1, 2, 5],
            host=platform.node(),
            system=platform.system(),
            slots=slots,
            active=[dict(r) for r in active],
            backend=nx._ensure_nexus().backend_name,
            nexus=version("dimforge-nexus3d"),
            robots=robots,
            worker_sha256=digest(HERE / "worker.py"),
        )
    if op == "submit":
        return submit(root, request)
    ident = job_id(request["id"])
    if op in ("archive", "read-archive"):
        from fleet_artifacts import metadata, piece

        row = status(root, ident)
        if row["state"] not in ("completed", "failed", "cancelled"):
            raise ValueError("Wait for the worker to close the simulation artifacts")
        return (
            metadata(root / ident) if op == "archive" else piece(root / ident, request)
        )
    if op == "cancel":
        with transaction(root) as db:
            db.execute(
                "UPDATE jobs SET state='stopping' WHERE id=? AND state IN ('running','reserved')",
                (ident,),
            )
    elif op in ("frame", "observation"):
        row = status(root, ident)
        if op == "frame" and not row["stream"]:
            raise ValueError("Choose the foreground job for a live frame")
        if op == "observation" and row["stream"]:
            raise ValueError("Choose a background job to sample an observation")
        path = root / ident / "trial/preview.rgb"
        if not path.is_file():
            return dict(ready=False)
        raw = path.read_bytes()
        height = (len(raw) - 4) // (480 * 3)
        if height not in (320, 360) or len(raw) != 4 + 480 * height * 3:
            raise ValueError("Invalid live frame")
        return dict(
            width=480,
            height=height,
            ready=True,
            tick=int.from_bytes(raw[:4], "big"),
            data=base64.b64encode(raw[4:]).decode(),
        )
    elif op != "status":
        raise ValueError("Unknown worker operation")
    return status(root, ident)


def stream_rgb(root, ident, output):
    """Write a bounded JSON header and optional tick-prefixed RGB plane per update."""
    ident = job_id(ident)
    path = root / ident / "trial/preview.rgb"
    last_tick = None
    next_status = 0
    while True:
        now = time.monotonic()
        info = None
        if now >= next_status:
            info = status(root, ident)
            if not info["stream"]:
                raise ValueError("Choose the foreground job for a live stream")
            next_status = now + 0.25
        frame = b""
        try:
            with path.open("rb") as f:
                header = f.read(4)
                if len(header) != 4:
                    raise ValueError("Incomplete foreground RGB tick")
                tick = int.from_bytes(header, "big")
                if tick != last_tick:
                    frame = header + f.read(480 * 360 * 3 + 1)
                    if len(frame) not in (4 + 480 * 320 * 3, 4 + 480 * 360 * 3):
                        raise ValueError("Invalid foreground RGB plane")
                    last_tick = tick
        except FileNotFoundError:
            pass
        if info or frame:
            packet = dict(bytes=len(frame))
            if info:
                packet["job"] = info
            output.write(json.dumps(packet, allow_nan=False).encode() + b"\n")
            if frame:
                output.write(frame)
            output.flush()
        if info and info["state"] not in ACTIVE:
            return
        time.sleep(0.004)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    cmd = parser.add_subparsers(dest="command", required=True)
    init = cmd.add_parser("init")
    init.add_argument("--slots", type=int, required=True)
    cmd.add_parser("rpc")
    cmd.add_parser("watch-rgb").add_argument("id")
    supervise_cmd = cmd.add_parser("_supervise")
    supervise_cmd.add_argument("id")
    supervise_cmd.add_argument("token")
    args = parser.parse_args()
    root = args.root.resolve()
    if args.command == "init":
        initialize(root, args.slots)
    elif args.command == "watch-rgb":
        try:
            stream_rgb(root, args.id, sys.stdout.buffer)
        except BrokenPipeError:
            return
    elif args.command == "rpc":
        try:
            raw = sys.stdin.buffer.read(MAX_REQUEST + 1)
            if len(raw) > MAX_REQUEST:
                raise ValueError("Simulation request exceeds supported size")
            result = dict(ok=True, reply=dispatch(root, json.loads(raw)))
        except (KeyError, OSError, TypeError, ValueError, sqlite3.Error) as exc:
            result = dict(ok=False, error=str(exc))
        print(json.dumps(result, allow_nan=False))
    else:
        supervise(root, job_id(args.id), args.token)


if __name__ == "__main__":
    main()
