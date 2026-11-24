"""Recheck the published camera inputs, model replies and Nexus motor journals offline."""

import hashlib
import io
import json
from pathlib import Path
from zipfile import ZipFile

import numpy as np
from PIL import Image
from sight_check import QUESTION, pixel_centres

HERE = Path(__file__).resolve().parent


def sha(data):
    return hashlib.sha256(data).hexdigest()


def audit(bundle):
    def read(path):
        return json.loads(bundle.read(path))

    sight = read("sight/results.json")
    fleet = read("fleet.json")
    tasks = set()
    colors = []
    for provider in ("codex", "octos-deepseek"):
        cli = read(f"{provider}/arguments.json")
        native = read(f"{provider}/native.json")
        assert native["restart_passed"] and native["stale_before_motor_write"]
        assert len(cli) == 2
        for seed, target in enumerate(("red", "blue")):
            folder = f"{provider}/{seed}/"
            plan_bytes = bundle.read(folder + "controller.json")
            plan = json.loads(plan_bytes)
            tasks.add(plan["task"])
            receipt = plan["scene_observation"]["author_input"]
            assert plan["provider"] == receipt["provider"] == provider
            assert plan["scene_observation"]["layout"] == seed
            source = read(folder + "source.json")
            result = read(folder + "verdict.json")
            assert (
                result["controller_hash"]
                == source["controller_sha256"]
                == sha(plan_bytes)
            )
            assert source["adapter"] == "aisle.sim.nexus_backend"
            assert plan["model_hash"] == source["model_sha256"] == result["model_hash"]
            files = plan["scene_observation"]["files"]
            for name, signature in files.items():
                assert sha(bundle.read(folder + name)) == signature
            request = read(folder + "controller-request.json")
            reply = read(folder + "reply.json")
            metrics = read(folder + "provider-metrics.json")
            assert request["provider"] == metrics["provider"] == provider
            assert plan["policy_model"] == metrics["model"]
            assert receipt == request["onboard_input"]
            assert receipt["packet_sha256"] == files["onboard-scene.npz"]
            assert receipt["calibration_sha256"] == files["onboard-calibration.json"]
            assert receipt["measurements_sha256"] == files["onboard-scene.json"]
            scene = read(folder + "onboard-scene.json")
            calibration = read(folder + "onboard-calibration.json")
            assert json.dumps(scene) in request["instruction"]
            assert json.dumps(calibration["cameras"]) in request["instruction"]
            left = max(
                scene["objects"], key=lambda color: scene["objects"][color]["y_left_m"]
            )
            assert left == target == reply["color_order"] == plan["color_order"]
            with np.load(
                io.BytesIO(bundle.read(folder + "onboard-scene.npz")),
                allow_pickle=False,
            ) as packet:
                assert set(packet) == {
                    "head",
                    "left_wrist",
                    "right_wrist",
                    "encoders",
                    "stamp_ns",
                }
                assert np.array_equal(
                    packet["head"],
                    np.frombuffer(
                        bundle.read(folder + "onboard-head.rgb"), np.uint8
                    ).reshape(360, 480, 3),
                )
                centres = pixel_centres(packet["head"])
                assert min(centres, key=centres.get) == target
                if provider == "codex":
                    images = receipt["images"]
                    assert [x["camera"] for x in images] == [
                        "head",
                        "left_wrist",
                        "right_wrist",
                    ]
                    assert cli[seed] == {
                        "tool": "codex",
                        "images": [
                            {"file": i["file"], "sha256": i["sha256"]} for i in images
                        ],
                    }
                    for im in images:
                        assert im["sha256"] == files[im["file"]]
                        with Image.open(
                            io.BytesIO(bundle.read(folder + im["file"]))
                        ) as image:
                            assert image.format == "PNG" and image.mode == "RGB"
                            assert np.array_equal(
                                np.asarray(image), packet[im["camera"]]
                            )
                    ablation = read(f"sight/request-{seed}.json")
                    assert ablation["instruction"] == QUESTION and ablation[
                        "onboard_input"
                    ] == {"images": images}
                    assert read("sight/arguments.json")[seed] == cli[seed]
                    assert (
                        sight[seed]["images"] == images
                        and sight[seed]["head_centres_x_px"] == centres
                    )
                    assert (
                        sight[seed]["reply"]["color"]
                        == sight[seed]["expected"]
                        == target
                    )
                else:
                    assert receipt["images"] == [] and cli[seed] == {
                        "tool": "octos",
                        "image_argument": False,
                    }
            assert result["task_success"] and result["requested"] == [target]
            assert result["wheel_supported_ticks"] == result["physics_ticks"]
            assert (
                result["root_pose_writes"]
                == result["max_joint_excursion_rad"]
                == result["rejected_motor_writes"]
                == 0
            )
            assert not any(result["robot_furniture_contacts"].values())
            cube = result["cubes"][target]
            assert (
                cube["passed"] and cube["detached"] and cube["cube_center_over_table"]
            )
            assert (
                cube["table_contact_tail_ticks"] >= 100
                and cube["opposing_finger_airborne_ticks"] > 0
            )
            motors = result["actuator_audit"]
            assert (
                motors["passed"] and motors["applied_rows"] == result["physics_ticks"]
            )
            assert sha(bundle.read(folder + "motor-targets.f64")) == motors["sha256"]
            if seed == 0 and provider == "codex":
                assert (
                    fleet["files"] == files
                    and fleet["controller_sha256"] == result["controller_hash"]
                )
                assert fleet["motor_journal_sha256"] == motors["sha256"] and fleet[
                    "target"
                ] == [target]
                assert fleet["reopened"] and fleet["altered_image_rejected"]
            colors.append((provider, seed, target, result["physics_ticks"]))
    assert len(tasks) == 1
    return colors


if __name__ == "__main__":
    with ZipFile(HERE / "evidence.zip") as bundle:
        for provider, seed, target, ticks in audit(bundle):
            print(f"{provider} layout {seed} placed {target}, {ticks} supported ticks")
