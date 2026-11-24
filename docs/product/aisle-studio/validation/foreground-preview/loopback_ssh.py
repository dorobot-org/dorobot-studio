"""Run one real local Nano worker and receive its preview through isolated loopback OpenSSH."""

import argparse
import getpass
import hashlib
import json
import os
import shlex
import socket
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path
from uuid import uuid4

repo = Path(__file__).resolve().parents[5]
wb = repo / "tools/aisle_workbench"
sys.path.insert(0, str(wb))
from fleet_bridge import submit, watch_foreground  # noqa: E402 (repository checkout on sys.path)
from fleet_client import send, submission  # noqa: E402 (repository checkout on sys.path)
from fleet_worker import initialize  # noqa: E402 (repository checkout on sys.path)

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("out", type=Path, help="A new output directory")
parser.add_argument(
    "controller", type=Path, help="Prepared red-cube Nano controller.json"
)
args = parser.parse_args()
out = args.out.resolve()
out.mkdir(exist_ok=False)
spool = out / "worker"
initialize(spool, 1)
(alias, ident) = ("preview-proof", str(uuid4()))
local = dict(
    checkout=str(repo),
    python=sys.executable,
    spool=str(spool),
    aisle_root=os.environ["DOROBOT_AISLE_ROOT"],
)
req = submission(args.controller, mode="stream", ident=ident)


def dump(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


with tempfile.TemporaryDirectory() as tmp:
    scratch = Path(tmp)
    for name in ("host", "client"):
        subprocess.run(
            ["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-f", str(scratch / name)],
            check=True,
        )
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    (scratch / "known").write_text(
        f"[127.0.0.1]:{port} " + (scratch / "host.pub").read_text()
    )
    user = getpass.getuser()
    command = shlex.join(
        [
            sys.executable,
            str(wb / "fleet_worker.py"),
            "--root",
            str(spool),
            "watch-rgb",
            ident,
        ]
    )
    server_config = scratch / "server.conf"
    server_config.write_text(f"""Port {port}
ListenAddress 127.0.0.1
HostKey {scratch / "host"}
PidFile {scratch / "pid"}
AuthorizedKeysFile {scratch / "client.pub"}
StrictModes yes
UsePAM no
PasswordAuthentication no
KbdInteractiveAuthentication no
PermitTTY no
AllowTcpForwarding no
AllowAgentForwarding no
AllowUsers {user}
ForceCommand {command}
""")
    client_config = scratch / "client.conf"
    client_config.write_text(f"""Host {alias}
 HostName 127.0.0.1
 Port {port}
 User {user}
 IdentitiesOnly yes
 IdentityFile {scratch / "client"}
 StrictHostKeyChecking yes
 UserKnownHostsFile {scratch / "known"}
""")
    sshnode = local | dict(ssh=alias, ssh_config=str(client_config))
    trial = spool / ident / "trial"
    with (out / "sshd.log").open("w") as log:
        server = subprocess.Popen(
            ["/usr/sbin/sshd", "-D", "-e", "-f", str(server_config)], stderr=log
        )
        try:
            for _ in range(50):
                if (scratch / "pid").is_file():
                    break
                if server.poll() is not None:
                    raise RuntimeError("Test SSH daemon failed")
                time.sleep(0.1)
            submit(out, {alias: local}, alias, req)
            viewport = out / "received"
            viewport.mkdir()
            task_errors = []

            def watch():
                try:
                    watch_foreground(sshnode, ident, viewport, out)
                except Exception as exc:
                    task_errors.append(str(exc))

            t = threading.Thread(target=watch)
            t.start()
            observed = []
            last = None
            deadline = time.monotonic() + 300
            while t.is_alive() and time.monotonic() < deadline:
                try:
                    raw = (viewport / "preview.rgb").read_bytes()
                except FileNotFoundError:
                    time.sleep(0.004)
                    continue
                tick = int.from_bytes(raw[:4], "big")
                if tick != last:
                    assert len(raw) == 4 + 480 * 360 * 3
                    observed.append(
                        [tick, time.monotonic_ns(), hashlib.sha256(raw[4:]).hexdigest()]
                    )
                    last = tick
                time.sleep(0.004)
            t.join(5)
            assert not t.is_alive() and not task_errors, task_errors
            final = json.loads((viewport / "watch.json").read_bytes())
            source = dict(
                json.loads((trial / "preview-timing.json").read_bytes())["tick_time"]
            )
            assert (
                final["job"]["state"] == "completed"
                and final["job"]["result"]["task_success"]
            )
            assert (viewport / "preview.rgb").read_bytes() == (
                trial / "preview.rgb"
            ).read_bytes() and all(tick in source for tick, _, _ in observed)
            fps = 1e9 * (len(observed) - 1) / (observed[-1][1] - observed[0][1])
            result = dict(
                transport="One OpenSSH connection on loopback",
                drawn_by_native=False,
                job=ident,
                camera="head",
                frames=len(observed),
                source_frames=len(source),
                received_fps=fps,
                final=final,
                measurements=observed,
            )
            dump(out / "proof.json", result)
            print("SSH frames:", len(observed), "FPS:", fps)
        except BaseException:
            # The spool and its only possible lease belong to this invocation.
            if (spool / ident).is_dir():
                send(local, {"op": "cancel", "id": ident})
            raise
        finally:
            server.terminate()
            server.wait(timeout=5)
