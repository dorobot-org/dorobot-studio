"""Exercise scene-grounded task generation with physical approach and stale-scene rejection."""

import hashlib
import json
from pathlib import Path
import sys


def journey(args, checks, page, click, remote, done, grab):
    from onboard_scene import CALIBRATION, PACKET, SUMMARY, interpret
    import numpy as np

    validation = Path(__file__).resolve().parents[1] / "aisle-validation"
    sys.path.insert(0, str(validation))
    from worker import describe

    model = describe("Nano")["model"]
    if args.recover:
        page("evidence")
        row = done(
            lambda w: w["result"]
            and w["result"].get("task_success")
            and w["result"].get("kind") == "visual-approach",
            "Observed target approach reopened",
        )
        assert (Path(row["selected"]) / PACKET).is_file()
        (args.out / "recovery.json").write_text(json.dumps(checks, indent=2))
        return
    request = "Approach the leftmost visible floor cube as seen by the robot and stop 0.45 metres away."
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
            assert not (Path(rejected["selected"]) / "trace.jsonl").exists()
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
        assert plan["provider"] == args.provider
        previous = row["selected"]
        grab("observed-task-" + str(seed))
        page("run")
        click("aisle_run.start_run")
        row = done(
            lambda w: not w["busy"]
            and w["selected"] != previous
            and w["result"]
            and w["result"].get("task_success"),
            "Observed target reached on layout " + str(seed),
            120,
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
        assert interpret((path / PACKET).read_bytes(), calibration, model) == public
        colors = {
            name: obj["y_left_m"]
            for name, obj in public["objects"].items()
            if obj["status"] == "localized"
        }
        assert len(colors) == 3 and max(colors, key=colors.get) == plan["color"]
        result = row["result"]
        assert (
            result["wheel_supported_ticks"] == result["physics_ticks"]
            and result["obstacle_contact_ticks"] == 0
        )
        selected.append(
            dict(
                layout=seed,
                controller=plan,
                observation=public,
                final_distance_m=result["final_distance_m"],
                physics_ticks=result["physics_ticks"],
                controller_hash=result["controller_hash"],
            )
        )
        page("evidence")
        grab("observed-target-" + str(seed))
    assert selected[0]["controller"]["color"] != selected[1]["controller"]["color"]
    checks.append(
        dict(
            check="Same instruction, different camera-selected targets", trials=selected
        )
    )
    (args.out / "journey.json").write_text(json.dumps(checks, indent=2))
