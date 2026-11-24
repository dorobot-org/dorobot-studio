"""Register eight isolated single-slot workers on one Linux NVIDIA Vulkan host."""

import argparse
import csv
import json
from pathlib import Path, PurePosixPath
import re
import shlex
import subprocess


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    for opt in ("ssh", "checkout", "aisle-root", "interpreter", "remote-parent"):
        cli.add_argument("--" + opt, required=True)
    cli.add_argument("--ssh-config", type=Path, default=Path.home() / ".ssh/config")
    cli.add_argument("--out", type=Path, required=True)
    args = cli.parse_args()
    for path in (args.checkout, args.aisle_root, args.interpreter, args.remote_parent):
        if not PurePosixPath(path).is_absolute():
            cli.error("Supply absolute paths on the remote Linux machine")
    if args.out.exists():
        cli.error("Select a new output registry path")
    ssh = [
        "ssh",
        "-F",
        str(args.ssh_config.resolve()),
        "-o",
        "BatchMode=yes",
        "--",
        args.ssh,
    ]

    def remote(argv, data=None):
        return subprocess.run(
            [*ssh, shlex.join(argv)],
            input=data,
            text=True,
            capture_output=True,
            check=True,
            timeout=90,
        ).stdout.strip()

    rows = list(
        csv.reader(
            remote(
                [
                    "nvidia-smi",
                    "--query-gpu=index,pci.bus_id,name",
                    "--format=csv,noheader",
                ]
            ).splitlines(),
            skipinitialspace=True,
        )
    )
    if (
        len(rows) != 8
        or {int(r[0]) for r in rows} != set(range(8))
        or any("H100" not in r[2] for r in rows)
    ):
        cli.error(
            "This reference workload requires eight individually enumerated H100 GPUs"
        )
    root = remote(
        ["mktemp", "-d", args.remote_parent.rstrip("/") + "/eight-card-XXXXXXXX"]
    )
    runtime = root + "/xdg"
    remote(["install", "-d", "-m", "700", runtime])
    nodes = {}
    for index, address, _name in sorted(rows, key=lambda row: int(row[0])):
        bits = re.fullmatch(
            r"([0-9a-fA-F]+):([0-9a-fA-F]{2}):([0-9a-fA-F]{2})\.([0-7])", address
        )
        if not bits:
            raise ValueError(f"Unrecognized PCI address {address!r}")
        domain, bus, slot, function = bits.groups()
        dri = f"pci-{int(domain,16):04x}_{bus.lower()}_{slot.lower()}_{function}"
        python, spool = root + "/python-" + index, root + "/spool-" + index
        env = dict(
            VK_ICD_FILENAMES="/usr/share/vulkan/icd.d/nvidia_icd.json",
            VK_INSTANCE_LAYERS="VK_LAYER_MESA_device_select",
            DRI_PRIME=dri,
            MESA_VK_DEVICE_SELECT_FORCE_DEFAULT_DEVICE="1",
            WGPU_BACKEND="vulkan",
            XDG_RUNTIME_DIR=runtime,
            PYTHONPATH=args.aisle_root + "/src",
            DOROBOT_AISLE_ROOT=args.aisle_root,
            OPENBLAS_NUM_THREADS="1",
            OMP_NUM_THREADS="1",
        )
        wrapper = "#!/bin/sh\n" + "".join(
            "export " + k + "=" + shlex.quote(v) + "\n" for k, v in env.items()
        )
        wrapper += "exec " + shlex.quote(args.interpreter) + ' "$@"\n'
        remote(["tee", python], wrapper)
        remote(["chmod", "700", python])
        remote(
            [
                python,
                args.checkout + "/tools/aisle_workbench/fleet_worker.py",
                "--root",
                spool,
                "init",
                "--slots",
                "1",
            ]
        )
        nodes["gpu" + index] = dict(
            ssh=args.ssh,
            ssh_config=str(args.ssh_config.resolve()),
            python=python,
            spool=spool,
            checkout=args.checkout,
            aisle_root=args.aisle_root,
        )
    with args.out.open("x") as output:
        json.dump(nodes, output, indent=2)
        output.write("\n")


if __name__ == "__main__":
    main()
