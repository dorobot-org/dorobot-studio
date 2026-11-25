"""Replay the native Nano trials and the Linux observer against the same saved rig."""

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
    if not __debug__:
        raise RuntimeError("Run this verification with Python assertions enabled")
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
            # OpenCV's solver differs slightly across CPU builds. Compare the
            # recovered mount in physical units. Archived runtime optics below
            # must still agree exactly with the selected revision.
            actual = np.asarray(pose)
            reference = np.asarray(saved["link_from_camera_gl"])
            translation_m = np.linalg.norm(actual[:3, 3] - reference[:3, 3])
            relative = actual[:3, :3].T @ reference[:3, :3]
            rotation_deg = np.degrees(
                np.arccos(np.clip((np.trace(relative) - 1) / 2, -1, 1))
            )
            assert translation_m <= 1e-6 and rotation_deg <= 0.001
            assert check["check_points"] == 5 and check["fit_points"] == 9
            assert check["max_check_px"] < 1.0
            image_refs.update(p["image"] for p in camera["landmarks"])
        assert len(image_refs) == 42 and image_refs == set(images)
        for key, encoded in images.items():
            assert digest(base64.b64decode(encoded, validate=True)) == key

        # Published counts and center-to-edge margins, rounded to millimetres.
        expected = {
            "mac-deepseek": (6968, 348, {"red": 120}),
            "mac-codex": (6968, 348, {"red": 120}),
            "mac-deepseek-three": (
                28302,
                1415,
                {"red": 120, "green": 123, "blue": 138},
            ),
            "h100-deepseek": (7048, 0, {"red": 125}),
        }
        for name, (expected_ticks, frames, margins_mm) in expected.items():
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
            assert ticks == expected_ticks and result["frames"] == frames
            assert result["task_success"] and set(result["requested"]) == set(
                margins_mm
            )
            for color, margin in margins_mm.items():
                cube = result["cubes"][color]
                assert (
                    cube["passed"]
                    and cube["detached"]
                    and cube["table_contact_tail_ticks"] == 100
                )
                assert round(1000 * cube["nearest_table_edge_m"]) == margin
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
                assert len(stamps) == 222 and round(rate, 2) == 2
                assert read(trial / "controller.json")["max_forward_speed_m_s"] == 0.08
                print(
                    f"{name}: {ticks} supported ticks, {len(stamps)} RGB publications at {rate:.3f} Hz"
                )


if __name__ == "__main__":
    main()
