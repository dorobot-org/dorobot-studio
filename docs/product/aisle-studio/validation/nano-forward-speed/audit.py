"""Check the published controllers, motor envelopes, speed caps and replayed support."""

import hashlib
from io import BytesIO
import json
from pathlib import Path
import sys
import xml.etree.ElementTree as ET
from zipfile import ZipFile

import numpy as np

PROOF = Path(__file__).resolve().parent
ROOT = PROOF.parents[4]
sys.path.insert(0, str(ROOT / "tools/aisle-validation"))
from nano_actuator_audit import MotorEnvelope


def digest(data):
    return hashlib.sha256(data).hexdigest()


def main():
    report = json.loads((PROOF / "measurements.json").read_bytes())
    model = ROOT / "data/adora_nano/adora_nano.urdf"
    names = [
        j.get("name")
        for j in ET.parse(model).getroot().findall("joint")
        if j.get("type") not in ("fixed", "floating")
    ]
    envelope = MotorEnvelope(model, names, 0.01)
    archive = PROOF / "trials.zip"
    assert digest(archive.read_bytes()) == report["archive_sha256"]
    with ZipFile(archive) as bundle:
        index = json.loads(bundle.read("index.json"))
        assert set(index) == set(report["trials"])
        for case, refs in index.items():

            def raw(file):
                expected = refs[file]
                value = bundle.read("objects/" + expected)
                assert digest(value) == expected
                return value

            for file in refs:
                raw(file)
            r = json.loads(raw("result.json"))
            c = json.loads(raw("controller.json"))
            source = json.loads(raw("source.json"))
            assert (
                r["controller_hash"]
                == source["controller_sha256"]
                == digest(raw("controller.json"))
            )
            assert (
                r["model_hash"] == source["model_sha256"] == digest(model.read_bytes())
            )
            motor = raw("motor-targets.f64")
            assert (
                digest(motor)
                == r["actuator_audit"]["sha256"]
                == report["trials"][case]["motor_sha256"]
            )
            rows = np.frombuffer(motor, dtype="<f8").reshape(-1, len(names))
            assert (
                len(rows) - 1 == r["physics_ticks"]
                and envelope.valid(rows[0], rows[0])
                and envelope.valid(rows[1:], rows[:-1]).all()
            )
            cap = c.get("max_forward_speed_m_s", 0.13)
            wheels = [
                a
                for a in map(json.loads, raw("actions.jsonl").splitlines())
                if a["op"] == "wheel"
            ]
            assert wheels and all(
                -0.08 <= a["speed"] <= cap and -0.5 <= a["turn"] <= 0.5 for a in wheels
            )
            scored = report["trials"][case]
            assert (
                scored["task_success"] == r["task_success"]
                and scored["settled"] == r["cubes"]["red"]["passed"]
            )
            gaps = [
                t
                for g in r["unsupported_wheel_ranges"]
                for t in range(g["first_step"], g["last_step"] + 1)
            ]
            assert len(gaps) == r["physics_ticks"] - r["wheel_supported_ticks"]
            if "replay.json" in refs:
                receipt = json.loads(raw("replay.json"))
                assert (
                    receipt["motor_sha256"] == digest(motor)
                    and receipt["backend"] == source["backend"]
                )
                assert receipt["impulses_sha256"] == digest(raw("wheel-impulses.npz"))
                with np.load(
                    BytesIO(raw("wheel-impulses.npz")), allow_pickle=False
                ) as data:
                    peaks = data["peak_normal_impulse_ns"]
                    assert (
                        peaks.shape == (len(rows) - 1, 3)
                        and np.isfinite(peaks).all()
                        and (peaks >= 0).all()
                    )
                    assert (
                        (1 + np.flatnonzero(peaks.max(axis=1) <= 1e-6)).tolist()
                        == gaps
                        == receipt["unsupported_ticks"]
                    )
            print(
                f"{case}: placed={scored['settled']}, unsupported_ticks={len(gaps)}, task_passed={scored['task_success']}"
            )


if __name__ == "__main__":
    main()
