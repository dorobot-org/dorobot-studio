"""Exercise scene-grounded task generation with physical approach and stale-scene rejection."""

import hashlib
import json
from pathlib import Path
import sys


def journey(args, checks, page, click, remote, done, grab, wait):
    from onboard_scene import CALIBRATION, PACKET, SUMMARY, interpret
    import numpy as np

    validation = Path(__file__).resolve().parents[1] / "aisle-validation"
    sys.path.insert(0, str(validation))
    from worker import describe

    from PIL import Image

    model = describe("Nano")["model"]
    transfer = args.observed_author == "transfer"
    kind = "table-transfer" if transfer else "visual-approach"
    labels = {
        "codex": "Sent to Codex · Head and both wrist images plus measured positions",
        "octos-deepseek": "Sent to DeepSeek · Positions measured from onboard RGB",
    }
    if args.recover:
        page("evidence")
        row = done(
            lambda w: w["result"]
            and w["result"].get("task_success")
            and w["result"].get("kind") == kind,
            "Observed target trial reopened",
        )
        assert (Path(row["selected"]) / PACKET).is_file()
        last = json.loads((args.out / "journey.json").read_bytes())[-1]["trials"][-1]
        assert row["result"]["controller_hash"] == last["controller_hash"]
        assert row["controller"] == last["controller"]
        input_label = labels[last["controller"]["provider"]]
        page("task")
        wait(
            lambda j: j["controls"]["aisle_task.author_inputs"]["text"] == input_label,
            "Original author input label restored",
        )
        (args.out / "recovery.json").write_text(json.dumps(checks, indent=2))
        return
    input_label = labels[args.provider]
    request = (
        "Move only the leftmost visible floor cube as seen by the robot onto the coffee table."
        if transfer
        else "Approach the leftmost visible floor cube as seen by the robot and stop 0.45 metres away."
    )
    page("task")
    click("aisle_task.nano")
    click("aisle_task.onboard")
    done(
        lambda w: w["onboard_context"], "Onboard perception selected for task authoring"
    )
    selected = []
    for seed in [0, 1]:
        if seed:
            page("scene")
            click("aisle_scene.layout1")
            done(
                lambda w: w["layout_seed"] == seed, "Second living-room layout selected"
            )
            page("run")
            click("aisle_run.start_run")
            rejected = done(
                lambda w: not w["busy"]
                and "capture a fresh onboard view" in w["status"],
                "Stale scene controller rejected",
                45,
            )
            rejected_folder = Path(rejected["selected"])
            assert not any(
                (rejected_folder / name).exists()
                for name in ("trace.jsonl", "motor-targets.f64")
            )
            page("task")
        click("aisle_task.instruction")
        remote(
            "key",
            "w=0",
            "c=KeyA",
            "logo=1" if sys.platform == "darwin" else "ctrl=1",
            "wait=1",
        )
        remote("text", "w=0", "t=" + request, "wait=1")
        click(
            "aisle_task.octos"
            if args.provider == "octos-deepseek"
            else "aisle_task.codex"
        )
        row = done(
            lambda w: not w["busy"]
            and w["controller"]
            and w["controller"].get("scene_observation", {}).get("layout") == seed,
            "Authored from measured RGB on layout " + str(seed),
            180,
        )
        plan = row["controller"]
        assert plan["provider"] == args.provider and plan["kind"] == kind
        wait(
            lambda j: j["controls"]["aisle_task.author_inputs"]["visible"]
            and j["controls"]["aisle_task.author_inputs"]["text"] == input_label,
            "Native Task identifies images and measured inputs",
        )
        previous = row["selected"]
        grab("observed-task-" + str(seed))
        page("run")
        click("aisle_run.start_run")
        row = done(
            lambda w: not w["busy"]
            and w["selected"] != previous
            and w["result"]
            and w["result"].get("kind") == kind,
            "Observed target reached on layout " + str(seed),
            300 if transfer else 120,
        )
        path = Path(row["selected"])
        binding = plan["scene_observation"]
        for name, digest in binding["files"].items():
            assert hashlib.sha256((path / name).read_bytes()).hexdigest() == digest
        public = json.loads((path / SUMMARY).read_text())
        calibration = json.loads((path / CALIBRATION).read_text())
        with np.load(path / PACKET, allow_pickle=False) as packet:
            assert set(packet) == {
                "head",
                "left_wrist",
                "right_wrist",
                "encoders",
                "stamp_ns",
            }
            author = next(
                folder
                for folder in path.parent.glob("*-Nano-prepare")
                if (folder / "controller.json").is_file()
                and hashlib.sha256(
                    (folder / "controller.json").read_bytes()
                ).hexdigest()
                == row["result"]["controller_hash"]
            )
            provider_request = json.loads(
                (author / "controller-request.json").read_bytes()
            )
            receipt = provider_request["onboard_input"]
            assert receipt["provider"] == args.provider
            assert receipt == binding["author_input"]
            assert receipt["packet_sha256"] == binding["files"][PACKET]
            assert receipt["calibration_sha256"] == binding["files"][CALIBRATION]
            assert receipt["measurements_sha256"] == binding["files"][SUMMARY]
            assert json.dumps(public) in provider_request["instruction"]
            assert json.dumps(calibration["cameras"]) in provider_request["instruction"]
            expected = (
                ["head", "left_wrist", "right_wrist"]
                if args.provider == "codex"
                else []
            )
            assert [im["camera"] for im in receipt["images"]] == expected
            for image in receipt["images"]:
                png = path / image["file"]
                assert png.read_bytes() == (author / image["file"]).read_bytes()
                assert hashlib.sha256(png.read_bytes()).hexdigest() == image["sha256"]
                with Image.open(png) as pixels:
                    assert np.array_equal(np.asarray(pixels), packet[image["camera"]])
        assert interpret((path / PACKET).read_bytes(), calibration, model) == public
        colors = {
            name: obj["y_left_m"]
            for name, obj in public["objects"].items()
            if obj["status"] == "localized"
        }
        assert len(colors) == 3
        target = max(colors, key=colors.get)
        assert target == plan["color_order" if transfer else "color"]
        result = row["result"]
        assert (
            result["task_success"]
            and result["wheel_supported_ticks"] == result["physics_ticks"]
        ), result.get("error")
        if transfer:
            assert result["requested"] == [target] and result["cubes"][target]["passed"]
            assert (
                result["root_pose_writes"]
                == result["max_joint_excursion_rad"]
                == result["rejected_motor_writes"]
                == 0
            )
            outcome = {"cube": result["cubes"][target]}
        else:
            assert result["obstacle_contact_ticks"] == 0
            outcome = {"final_distance_m": result["final_distance_m"]}
        selected.append(
            dict(
                layout=seed,
                controller=plan,
                observation=public,
                target=target,
                **outcome,
                physics_ticks=result["physics_ticks"],
                controller_hash=result["controller_hash"],
            )
        )
        page("evidence")
        grab("observed-target-" + str(seed))
    assert selected[0]["target"] != selected[1]["target"]
    checks.append(
        dict(
            check="Same instruction, different camera-selected targets", trials=selected
        )
    )
    (args.out / "journey.json").write_text(json.dumps(checks, indent=2))
