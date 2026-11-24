"""Verify source and client cadence separately for the single H100 observer."""

import hashlib
from io import BytesIO
import json
from pathlib import Path
from zipfile import ZipFile


def cadence(stamps):
    assert all(a < b for a, b in zip(stamps, stamps[1:]))
    return (len(stamps) - 1) * 1e9 / (stamps[-1] - stamps[0])


def main():
    with ZipFile(Path(__file__).with_name("h100-observer.zip")) as bundle:
        client = json.loads(bundle.read("gpu3.json"))
        terminal = json.loads(bundle.read("reconnected-status.json"))
        worker = json.loads(bundle.read("preflight.json"))["worker"]
        raw = bundle.read("gpu3.zip")
        assert (
            hashlib.sha256(raw).hexdigest()
            == bundle.read("archive-sha256.txt").decode().strip()
        )
        assert client["id"] == terminal["id"] and terminal["state"] == "completed"
        assert client["errors"] and "ssh" in client["error"]
        assert "NVIDIA H100 NVL" in bundle.read("device.txt").decode()
    with ZipFile(BytesIO(raw)) as trial:
        rows = [
            json.loads(line)
            for line in trial.read("background-observations.jsonl").splitlines()
        ]
        result = json.loads(trial.read("result.json"))
        source = json.loads(trial.read("source.json"))
        assert (
            source["backend"] == worker["backend"]
            and source["nexus"] == worker["nexus"]
        )
        assert (
            source["observation_period_s"] == 0.5
            and source["observation_clock"] == "wall"
        )
        assert (
            source["controller_sha256"]
            == client["controller_sha256"]
            == hashlib.sha256(trial.read("controller.json")).hexdigest()
        )
        assert (
            result == terminal["result"]
            and result["task_success"]
            and result["requested"] == ["red"]
        )
        assert (
            result["wheel_supported_ticks"] == result["physics_ticks"] == 6948
            and not result["unsupported_wheel_ranges"]
        )
        assert (
            hashlib.sha256(trial.read("motor-targets.f64")).hexdigest()
            == result["actuator_audit"]["sha256"]
        )
        published = {r["tick"]: r["sha256"] for r in rows}
        assert all([r["width"], r["height"]] == [480, 360] for r in rows)
        assert len(published) == len(rows) == 202
        assert len({t for t, _, _ in client["samples"]}) == len(client["samples"]) == 43
        assert all(published[t] == sha for t, _, sha in client["samples"])
        source_hz = cadence([r["published_monotonic_ns"] for r in rows])
        client_hz = cadence([s[1] for s in client["samples"]])
        assert round(source_hz, 2) == 2.0 and round(client_hz, 2) == 1.67
        print(
            f"{len(rows)} published at {source_hz:.2f} Hz. {len(client['samples'])} received at {client_hz:.2f} Hz before SSH loss."
        )


if __name__ == "__main__":
    main()
