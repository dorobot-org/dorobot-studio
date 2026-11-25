"""Connect to an explicitly configured AISLE worker locally or over SSH."""

import argparse
import base64
import hashlib
import json
from pathlib import Path
import shlex
import subprocess
from uuid import uuid4
import zlib

from worker import observation_seconds


class WorkerRejection(ValueError):
    """The worker explicitly rejected an RPC request."""


def worker_command(node, *operation):
    args = [
        node["python"],
        str(Path(node["checkout"]) / "tools/aisle_workbench/fleet_worker.py"),
        "--root",
        node["spool"],
        *operation,
    ]
    if node.get("aisle_root"):
        args = [
            "env",
            f"PYTHONPATH={node['aisle_root']}/src",
            f"DOROBOT_AISLE_ROOT={node['aisle_root']}",
            *args,
        ]
    if node.get("ssh"):
        args = [
            "ssh",
            *(["-F", node["ssh_config"]] if node.get("ssh_config") else []),
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=8",
            "--",
            node["ssh"],
            shlex.join(args),
        ]
    return args


def send(node, request):
    reply = subprocess.run(
        worker_command(node, "rpc"),
        input=json.dumps(request),
        capture_output=True,
        text=True,
        timeout=60,
    )
    if reply.returncode:
        raise RuntimeError(
            f"Worker transport returned {reply.returncode}: {reply.stderr.strip()[-500:]}"
        )
    try:
        result = json.loads(reply.stdout)
    except json.JSONDecodeError as exc:
        raise RuntimeError("Worker returned an unreadable RPC response") from exc
    if isinstance(result, dict):
        data = result.get("reply")
        error = result.get("error")
        if result.get("ok") is True and isinstance(data, dict) and "error" not in result:
            return data
        if (
            result.get("ok") is False
            and isinstance(error, str)
            and error.strip()
            and "reply" not in result
        ):
            raise WorkerRejection(error)
    raise RuntimeError("Worker returned an invalid RPC response")


def decode_rgb(reply):
    height = reply["height"]
    if type(height) is not int or reply["width"] != 480 or height not in (320, 360):
        raise ValueError("Invalid worker camera dimensions")
    size = 480 * height * 3
    if len(reply["data"]) > 4 * ((size + 2) // 3):
        raise ValueError("Invalid worker camera payload length")
    pixels = base64.b64decode(reply["data"], validate=True)
    return decode_plane(pixels, reply.get("encoding", "raw"), size)


def decode_plane(pixels, encoding, size):
    if encoding == "zlib":
        decoder = zlib.decompressobj()
        try:
            pixels = decoder.decompress(pixels, size + 1)
        except zlib.error as exc:
            raise ValueError("Invalid compressed worker RGB") from exc
        if not decoder.eof or decoder.unused_data:
            raise ValueError("Incomplete or oversized worker RGB")
    elif encoding != "raw":
        raise ValueError("Unknown worker RGB encoding")
    if len(pixels) != size:
        raise ValueError("Invalid worker camera frame")
    return pixels


def poll_observation(node, ident, last_tick=None, *, rpc=send):
    request = dict(op="observation", id=ident, accept_encoding="zlib", include_job=True)
    if last_tick is not None:
        request["after_tick"] = last_tick
    frame = rpc(node, request)
    job = frame.get("job")
    if job is None:  # Older workers return only the image.
        job = rpc(node, dict(op="status", id=ident))
    if job.get("id") != ident:
        raise ValueError("Observation returned a different job")
    return job, frame


def check_background_period(node, period, clock="simulation"):
    capabilities = send(node, dict(op="hello"))
    if clock not in capabilities.get("background_rgb_clocks", ["simulation"]):
        raise ValueError(
            "This worker does not advertise wall-clock RGB observations. Select simulation time or update the worker."
        )
    periods = capabilities.get("background_rgb_periods_s", [])
    if clock == "wall":
        periods = capabilities.get("background_wall_rgb_periods_s", periods)
    if period not in periods:
        raise ValueError(
            "This worker does not advertise the selected background RGB period. Update its Studio checkout."
        )


def submission(
    path, mode="observations", seed=0, fixture="standard", ident=None, room=None
):
    from fleet_worker import ROOM_FILES, companion_names

    raw = path.read_bytes()
    plan = json.loads(raw)
    files = {
        name: base64.b64encode((path.parent / name).read_bytes()).decode()
        for name in companion_names(plan)
    }
    room_hash = None
    if room is not None:
        room_hash = hashlib.sha256(room.read_bytes()).hexdigest()
        files.update(
            {
                "room/" + name: base64.b64encode(
                    room.with_name(name).read_bytes()
                ).decode()
                for name in ROOM_FILES
            }
        )
    return dict(
        op="submit",
        id=ident or str(uuid4()),
        controller=base64.b64encode(raw).decode(),
        controller_sha256=hashlib.sha256(raw).hexdigest(),
        robot=plan["robot"],
        files=files,
        room_sha256=room_hash,
        mode=mode,
        seed=seed,
        fixture=fixture,
    )


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("--nodes", type=Path, required=True)
    cli.add_argument("--node", required=True)
    cmds = cli.add_subparsers(dest="op", required=True)
    cmds.add_parser("hello")
    submit = cmds.add_parser("submit")
    submit.add_argument("controller", type=Path)
    submit.add_argument("--room", type=Path, help="Selected room.json revision")
    submit.add_argument("--id", help="Reuse the same UUID when retrying a submission")
    submit.add_argument(
        "--mode", choices=["stream", "observations"], default="observations"
    )
    submit.add_argument(
        "--observation-period-s",
        type=observation_seconds,
        choices=(0.5, 1, 2, 5),
        default=1,
    )
    submit.add_argument(
        "--observation-clock", choices=("simulation", "wall"), default="simulation"
    )
    submit.add_argument("--seed", type=int, default=0)
    submit.add_argument(
        "--fixture", choices=["standard", "compliant", "impact"], default="standard"
    )
    for op in ["status", "cancel", "frame", "observation", "fetch"]:
        p = cmds.add_parser(op)
        p.add_argument("id")
        if op in ("frame", "observation", "fetch"):
            p.add_argument("output", type=Path)
    args = cli.parse_args()
    request = dict(op=args.op)
    if args.op == "submit":
        request = submission(
            args.controller, args.mode, args.seed, args.fixture, args.id, args.room
        )
        request["observation_period_s"] = args.observation_period_s
        if args.observation_clock == "wall":
            if args.mode != "observations":
                cli.error("--observation-clock wall requires background observations")
            request["observation_clock"] = "wall"
    elif args.op != "hello":
        request["id"] = args.id
    node = json.loads(args.nodes.read_bytes())[args.node]
    if args.op == "submit" and args.mode == "observations":
        check_background_period(node, args.observation_period_s, args.observation_clock)
    if args.op == "fetch":
        from fleet_artifacts import download

        print(json.dumps(download(send, node, args.id, args.output), indent=2))
        return
    if args.op == "observation":
        request["accept_encoding"] = "zlib"
    reply = send(node, request)
    if args.op in ("frame", "observation") and reply["ready"]:
        args.output.write_bytes(decode_rgb(reply))
        del reply["data"]
    print(json.dumps(reply, indent=2))


if __name__ == "__main__":
    main()
