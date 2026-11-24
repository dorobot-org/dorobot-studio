"""Check the two native Nano trials and the Linux observer against the same saved rig."""

import base64
import hashlib
import json
from pathlib import Path
import sys
from zipfile import Path as ZipPath, ZipFile

import numpy as np

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[4]
sys.path[:0] = [
    str(ROOT / "tools/aisle-validation"),
    str(ROOT / "tools/aisle_workbench"),
]
from camera_survey import fit_mount
from nano_actuator_audit import MotorEnvelope, replay


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def read(path):
    return json.loads(path.read_bytes())


def main():
    model = ROOT / "data/adora_nano/adora_nano.urdf"
    with ZipFile(HERE / "camera-mount-trials.zip") as archive:
        data = ZipPath(archive)
        room = read(data / "room.json")
        survey = data / "survey.json"
        assert room["archived_survey_sha256"] == digest(survey.read_bytes())
        rig = read(survey)["camera_calibration"]
        assert rig["model_sha256"] == room["model_sha256"] == digest(model.read_bytes())
        images = rig["images"]
        image_refs = set()
        for name, camera in rig["cameras"].items():
            pose, check = fit_mount(name, camera)
            saved = room["settings"]["cameras"][name]
            np.testing.assert_allclose(saved["link_from_camera_gl"], pose, atol=1e-8)
            assert check["check_points"] == 5 and check["fit_points"] == 9
            assert check["max_check_px"] < 1.0
            image_refs.update(p["image"] for p in camera["landmarks"])
        assert len(image_refs) == 42 and image_refs == set(images)
        for key, encoded in images.items():
            assert digest(base64.b64decode(encoded, validate=True)) == key

        for name in ("mac-deepseek", "mac-codex", "h100-deepseek"):
            trial = data / name
            result = read(trial / "result.json")
            source = read(trial / "source.json")
            optics = read(trial / "realized-optics.json")
            assert optics["cameras"] == room["settings"]["cameras"]
            assert (
                digest((data / "room.json").read_bytes())
                == source["room_revision_sha256"]
                == result["room_revision_sha256"]
            )
            assert (
                digest((trial / "controller.json").read_bytes())
                == result["controller_hash"]
                == source["controller_sha256"]
            )
            assert (
                room["model_sha256"] == source["model_sha256"] == result["model_hash"]
            )
            ticks = result["physics_ticks"]
            motor = replay(
                trial / "motor-targets.f64",
                MotorEnvelope(model, optics["joint_names"]),
                ticks,
            )
            assert motor == result["actuator_audit"] and motor["passed"]
            assert result["task_success"] and result["cubes"]["red"]["passed"]
            assert (
                result["wheel_supported_ticks"] == ticks
                and result["max_joint_excursion_rad"] == 0
            )
            if name.startswith("mac-"):
                journey = {row["check"]: row for row in read(trial / "journey.json")}
                assert (
                    "9.94 px"
                    in journey["Rejected the incorrect camera measurement"]["reason"]
                )
                assert read(trial / "recovery.json")[0]["outcome"]
                recording = journey[
                    "Native Replay and synchronized encoder/action columns"
                ]
                assert set(recording["cameras"]) == set(optics["cameras"])
                assert result["frames"] == recording["frames_per_camera"] > 0
                print(
                    f"{name}: {ticks} supported ticks, {result['frames']} frames per camera"
                )
            else:
                packets = [
                    json.loads(row)
                    for row in (trial / "background-observations.jsonl")
                    .read_text()
                    .splitlines()
                ]
                stamps = [p["published_monotonic_ns"] for p in packets]
                assert all(b > a for a, b in zip(stamps, stamps[1:]))
                assert (
                    source["observation_clock"] == "wall"
                    and source["observation_period_s"] == 0.5
                )
                rate = (len(stamps) - 1) * 1e9 / (stamps[-1] - stamps[0])
                print(
                    f"{name}: {ticks} supported ticks, {len(stamps)} RGB publications at {rate:.3f} Hz"
                )


if __name__ == "__main__":
    main()
