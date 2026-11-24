"""Coordinate Studio's foreground stream and recover submitted jobs across restarts."""

import base64
from contextlib import closing, contextmanager
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import sqlite3
import stat
import sys
import tempfile
import time
from uuid import uuid4
from zipfile import ZipFile

from fleet_artifacts import download
from fleet_client import send, submission
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
        db.execute("UPDATE jobs SET state=? WHERE id=?", (state, ident))


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


def import_trial(root, node, row):
    dest = root / row["trial"]
    if dest.is_dir():
        receipt = json.loads((dest / "worker-origin.json").read_bytes())
        if receipt["job"] != row["id"]:
            raise ValueError("Trial directory belongs to another worker job")
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
        plan = json.loads((folder / "controller.json").read_bytes())

        if (
            plan.get("robot") != row["robot"]
            or hashlib.sha256((folder / "controller.json").read_bytes()).hexdigest()
            != json.loads(row["request"])["controller_sha256"]
        ):
            raise ValueError("Evidence belongs to a different controller or robot")
        save(
            folder / "worker-origin.json",
            dict(job=row["id"], node=row["node"], sha256=meta["sha256"]),
        )
        # A second Studio may have fetched the same immutable trial concurrently.
        with journal(root):
            if not dest.exists():
                folder.rename(dest)
    return dest


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
        while True:
            info = reconcile(root, row, node)
            frame = None
            if row["mode"] == "stream":
                reply = send(node, dict(op="frame", id=ident))
                if reply["ready"]:
                    data = base64.b64decode(reply["data"], validate=True)
                    if (
                        reply["width"] != 480
                        or reply["height"] not in (320, 360)
                        or len(data) != 480 * reply["height"] * 3
                    ):
                        raise ValueError("Invalid worker camera frame")
                    tmp = args.out / "preview.tmp"
                    tmp.write_bytes(reply["tick"].to_bytes(4, "big") + data)
                    tmp.replace(args.out / "preview.rgb")
                    frame = dict(tick=reply["tick"], height=reply["height"])
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
        if args.room_hash and req["room_sha256"] != args.room_hash:
            raise ValueError("Selected room revision has changed")
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
    info, trial = None, None
    if ident:
        row = record(root, ident)
        info = reconcile(root, row, node)
        if op == "cancel":
            info = send(node, dict(op="cancel", id=ident))
            update(root, ident, info["state"])
        if op == "fetch":
            trial = import_trial(root, node, row).name
        info.update(robot=row["robot"], mode=row["mode"])
    capabilities = send(node, dict(op="hello"))
    return dict(
        node=alias, capabilities=capabilities, jobs=ids, selected=info, imported=trial
    )
