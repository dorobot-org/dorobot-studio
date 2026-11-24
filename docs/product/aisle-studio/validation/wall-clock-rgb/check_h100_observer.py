"""Replay the source and receiver measurements for one H100 observation-only lease."""

import hashlib
from io import BytesIO
import json
from pathlib import Path
from zipfile import ZipFile

GUIDE = Path(__file__).resolve().parent


def sha256(raw):
    return hashlib.sha256(raw).hexdigest()


def cadence(stamps):
    assert all(a < b for a, b in zip(stamps, stamps[1:]))
    return (len(stamps) - 1) * 1e9 / (stamps[-1] - stamps[0])


def main():
    if not __debug__:
        raise RuntimeError("Run the evidence checks with assertions enabled")
    archive = GUIDE / "h100-observer.zip"
    assert (
        sha256(archive.read_bytes())
        == "ab07add3f6c97b51b24cd3bb33bd297c49e04252b1f4bd3d3ac528c8bdfd0fa8"
    )
    with ZipFile(archive) as bundle:

        def read(name):
            return json.loads(bundle.read(name))

        client = read("gpu3.json")
        terminal = read("reconnected-status.json")
        preflight = read("preflight.json")
        worker = preflight["worker"]
        device = read("device-sample.json")
        host, card = device["host_and_card"]
        index, bus, uuid, model = card.split(", ")
        assert host == worker["host"] and model == "NVIDIA H100 NVL"
        assert [index, uuid] == preflight["sample"][:2] and client[
            "alias"
        ] == f"gpu{index}"
        assert terminal["created"] <= device["observed_unix_s"] <= terminal["ended"]
        sampled_device = bundle.read("device.txt").decode()
        assert f"{index}, {bus}, {model}" in sampled_device and "C+G" in sampled_device
        assert client["id"] == terminal["id"] and terminal["state"] == "completed"
        ticks, arrivals, hashes = zip(*client["samples"])
        assert client["errors"] and "ssh" in client["error"].lower()
        assert all(
            stamp > arrivals[-1] and "ssh" in msg.lower()
            for stamp, msg in client["errors"]
        )
        raw = bundle.read("gpu3.zip")
        assert sha256(raw) == bundle.read("archive-sha256.txt").decode().strip()
    with (
        ZipFile(BytesIO(raw)) as trial,
        ZipFile(GUIDE.parent / "eight-h100/measurements.zip") as frozen,
    ):
        plan = trial.read("controller.json")
        assert plan == frozen.read("deepseek/gpu3/author/controller.json")
        controller = json.loads(plan)
        assert (
            controller["provider"] == "octos-deepseek"
            and controller["max_forward_speed_m_s"] == 0.08
        )
        source = json.loads(trial.read("source.json"))
        assert source["backend"] == worker["backend"] == "webgpu"
        assert source["nexus"] == worker["nexus"] == "0.2.1"
        assert source["adapter"] == "aisle.sim.nexus_backend" and source["seed"] == 0
        assert (
            source["observation_period_s"] == 0.5
            and source["observation_clock"] == "wall"
        )
        assert (
            source["controller_sha256"] == client["controller_sha256"] == sha256(plan)
        )
        result = json.loads(trial.read("result.json"))
        assert (
            result == terminal["result"]
            and result["task_success"]
            and result["requested"] == ["red"]
        )
        red = result["cubes"]["red"]
        assert (
            red["passed"]
            and red["detached"]
            and red["cube_center_over_table"]
            and red["table_contact_tail_ticks"] == 100
        )
        assert (
            result["wheel_supported_ticks"] == result["physics_ticks"] == 6948
            and not result["unsupported_wheel_ranges"]
        )
        assert (
            sha256(trial.read("motor-targets.f64"))
            == result["actuator_audit"]["sha256"]
        )
        rows = [
            json.loads(line)
            for line in trial.read("background-observations.jsonl").splitlines()
        ]
        published = {r["tick"]: r["sha256"] for r in rows}
        assert all([r["width"], r["height"]] == [480, 360] for r in rows)
        assert (
            len(published) == len(rows) == 202 and len(set(published.values())) == 200
        )
        assert len(set(hashes)) == len(ticks) == 43 and all(
            a < b for a, b in zip(ticks, ticks[1:])
        )
        assert all(published[t] == digest for t, digest in zip(ticks, hashes))
        assert round((arrivals[0] - client["submitted_ns"]) / 1e9, 2) == 7.57
        source_hz = cadence([r["published_monotonic_ns"] for r in rows])
        client_hz = cadence(arrivals)
        assert round(source_hz, 2) == 2.0 and round(client_hz, 2) == 1.67
        print(
            f"{len(rows)} published at {source_hz:.2f} Hz. {len(ticks)} received at {client_hz:.2f} Hz before SSH loss."
        )


if __name__ == "__main__":
    main()
