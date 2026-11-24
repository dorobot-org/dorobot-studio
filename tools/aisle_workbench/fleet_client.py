"""Connect to an explicitly configured AISLE worker locally or over SSH."""

import argparse
import base64
import hashlib
import json
from pathlib import Path
import shlex
import subprocess
from uuid import uuid4


def send(node, request):
    args = [
        node["python"],
        str(Path(node["checkout"]) / "tools/aisle_workbench/fleet_worker.py"),
        "--root",
        node["spool"],
        "rpc",
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
    reply = subprocess.run(
        args, input=json.dumps(request), capture_output=True, text=True, timeout=60
    )
    if reply.returncode:
        raise RuntimeError(
            f"Worker transport returned {reply.returncode}: {reply.stderr.strip()[-500:]}"
        )
    result = json.loads(reply.stdout)
    if not result["ok"]:
        raise ValueError(result["error"])
    return result["reply"]


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
    submit.add_argument("--seed", type=int, default=0)
    submit.add_argument(
        "--fixture", choices=["standard", "compliant", "impact"], default="standard"
    )
    for op in ["status", "cancel", "frame", "fetch"]:
        p = cmds.add_parser(op)
        p.add_argument("id")
        if op in ("frame", "fetch"):
            p.add_argument("output", type=Path)
    args = cli.parse_args()
    request = dict(op=args.op)
    if args.op == "submit":
        request = submission(
            args.controller, args.mode, args.seed, args.fixture, args.id, args.room
        )
    elif args.op != "hello":
        request["id"] = args.id
    node = json.loads(args.nodes.read_bytes())[args.node]
    if args.op == "fetch":
        from fleet_artifacts import download

        print(json.dumps(download(send, node, args.id, args.output), indent=2))
        return
    reply = send(node, request)
    if args.op == "frame" and reply["ready"]:
        args.output.write_bytes(base64.b64decode(reply.pop("data"), validate=True))
    print(json.dumps(reply, indent=2))


if __name__ == "__main__":
    main()
