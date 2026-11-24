"""Run one native stream and seven independent Nexus workers for a prepared cohort."""

import argparse
import base64
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import threading
import time
from zipfile import ZipFile

ROOT = Path(__file__).resolve().parents[5]
TOOLS = ROOT / "tools/aisle_workbench"
sys.path.insert(0, str(TOOLS))
from fleet_artifacts import download
from fleet_client import send, submission

clock = time.monotonic_ns


def write(path, obj):
    path.write_text(json.dumps(obj, indent=2) + "\n")


def gpu_sampler(stop, node, out):
    with out.open("w") as log:
        while not stop.is_set():
            cmd = [
                "ssh",
                "-F",
                node["ssh_config"],
                node["ssh"],
                "nvidia-smi --query-gpu=index,pci.bus_id,memory.used,utilization.gpu "
                "--format=csv,noheader,nounits",
            ]
            result = subprocess.run(cmd, text=True, capture_output=True, timeout=25)
            print(
                json.dumps([clock(), result.returncode, result.stdout.splitlines()]),
                file=log,
                flush=True,
            )
            stop.wait(4)


def stamp_draws(stop, ready, trace, out):
    with out.open("w") as log:
        while not trace.exists():
            if stop.wait(0.01):
                return
        with trace.open() as source:
            while not stop.is_set():
                pos = source.tell()
                line = source.readline()
                if not line.endswith("\n"):
                    source.seek(pos)
                    stop.wait(0.01)
                    continue
                row = json.loads(line)
                if row["source"].endswith("-fleet-watch"):
                    print(
                        json.dumps(
                            [
                                clock(),
                                row["elapsed_ns"],
                                row["tick"],
                                Path(row["source"]).name,
                            ]
                        ),
                        file=log,
                        flush=True,
                    )
                    ready.set()


def read_worker(node, request, errors):
    for attempt in range(4):
        try:
            return send(node, request)
        except (RuntimeError, subprocess.TimeoutExpired) as error:
            errors.append([clock(), str(error)])
            if attempt == 3:
                raise
            time.sleep(0.15 * (attempt + 1))


def observer(alias, node, author, out):
    req = submission(author / "controller.json", mode="observations", seed=0)
    req["observation_period_s"] = 1
    ident = req["id"]
    samples, steps, errors = [], [], []
    record = dict(
        id=ident,
        alias=alias,
        samples=samples,
        steps=steps,
        errors=errors,
        controller_sha256=req["controller_sha256"],
        submitted_ns=clock(),
    )
    last_step = last_frame = None
    try:
        job = send(node, req)
        deadline = time.monotonic() + 900
        while job["state"] in ("reserved", "running", "stopping"):
            if time.monotonic() > deadline:
                raise TimeoutError(f"{alias} exceeded 900 seconds")
            job = read_worker(node, dict(op="status", id=ident), errors)
            step = (job.get("progress") or {}).get("step", 0)
            if step and step != last_step and job["state"] == "running":
                steps.append([step, clock()])
                last_step = step
            frame = read_worker(node, dict(op="observation", id=ident), errors)
            if frame["ready"] and frame["tick"] != last_frame:
                when = clock()
                pixels = base64.b64decode(frame["data"], validate=True)
                assert (frame["width"], frame["height"]) == (480, 360) and len(
                    pixels
                ) == 480 * 360 * 3
                samples.append(
                    [frame["tick"], when, hashlib.sha256(pixels).hexdigest()]
                )
                last_frame = frame["tick"]
            time.sleep(0.20)
        record.update(state=job["state"], terminal_ns=clock())
        archive = out / f"{alias}.zip"
        record["archive_sha256"] = download(send, node, ident, archive)["sha256"]
        with ZipFile(archive) as z:
            verdict = json.loads(z.read("result.json"))
            src = json.loads(z.read("source.json"))
            captures = [
                json.loads(line)
                for line in z.read("background-observations.jsonl").splitlines()
            ]
            hashes = {r["tick"]: r["sha256"] for r in captures}
            assert all(hashes[tick] == digest for tick, _, digest in samples)
            assert src["nexus"] == "0.2.1" and src["backend"] == "webgpu"
            assert (
                hashlib.sha256(z.read("controller.json")).hexdigest()
                == req["controller_sha256"]
                == src["controller_sha256"]
            )
            assert (
                hashlib.sha256(z.read("motor-targets.f64")).hexdigest()
                == verdict["actuator_audit"]["sha256"]
            )
            assert verdict["requested"] == ["red"] and verdict["recording"] is False
            record.update(result=verdict, capture_ticks=list(hashes))
    except BaseException as error:
        record["error"] = repr(error)
        try:
            if send(node, dict(op="status", id=ident))["state"] in (
                "running",
                "reserved",
            ):
                send(node, dict(op="cancel", id=ident))
        except Exception as cleanup:
            errors.append([clock(), repr(cleanup)])
        raise
    finally:
        write(out / f"{alias}.json", record)


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    for option in ("nodes", "studio", "remote", "authors", "out"):
        cli.add_argument("--" + option, type=Path, required=True)
    a = cli.parse_args()
    nodes = json.loads(a.nodes.read_bytes())
    aliases = list(nodes)
    assert len(nodes) == 8 and len({v["spool"] for v in nodes.values()}) == 8
    assert len({v["ssh"] for v in nodes.values()}) == 1
    authors = sorted(a.authors.glob("*-Nano-prepare"))
    preparations = json.loads((a.authors / "results.json").read_bytes())["authors"]
    assert len(preparations) == len(authors) == 8
    assert all(
        row["exit"] == 0
        and row["program"].get("max_forward_speed_m_s") == 0.08
        and row["program"]["color_order"] == "red"
        for row in preparations
    )
    a.out.mkdir(parents=True, exist_ok=False)
    native = a.out / "native"
    ready, stop = threading.Event(), threading.Event()
    threads = [
        threading.Thread(
            target=stamp_draws,
            args=(
                stop,
                ready,
                native / "verified/journey-draws.jsonl",
                a.out / "draws.jsonl",
            ),
        ),
        threading.Thread(
            target=gpu_sampler, args=(stop, nodes[aliases[0]], a.out / "gpu.jsonl")
        ),
    ]
    for thread in threads:
        thread.start()
    command = [
        sys.executable,
        str(TOOLS / "verify.py"),
        "--studio",
        str(a.studio),
        "--remote",
        str(a.remote),
        "--backend",
        "offscreen",
        "--size",
        "1440x900",
        "--fleet",
        "--fleet-node",
        aliases[0],
        "--fleet-controller",
        str(authors[0]),
        "--out",
        str(native),
    ]
    times = dict(start_ns=clock())
    try:
        with (a.out / "native.log").open("w") as log, subprocess.Popen(
            command,
            stdout=log,
            stderr=subprocess.STDOUT,
            env=os.environ | {"DOROBOT_AISLE_WORKERS": str(a.nodes.resolve())},
        ) as proc:
            try:
                deadline = time.monotonic() + 185
                while not ready.wait(0.2):
                    if proc.poll() is not None or time.monotonic() > deadline:
                        raise RuntimeError(
                            "Native viewport did not deliver its first frame"
                        )
                with ThreadPoolExecutor(max_workers=7) as pool:
                    futures = [
                        pool.submit(observer, alias, nodes[alias], author, a.out)
                        for alias, author in zip(aliases[1:], authors[1:])
                    ]
                    times["native_exit"] = proc.wait(timeout=1050)
                    for f in futures:
                        f.result()
                if times["native_exit"]:
                    raise RuntimeError(
                        f"Native verification exited {times['native_exit']}"
                    )
            finally:
                if proc.poll() is None:
                    proc.terminate()
    finally:
        times["end_ns"] = clock()
        write(a.out / "timing.json", times)
        stop.set()
        for thread in threads:
            thread.join()


if __name__ == "__main__":
    main()
