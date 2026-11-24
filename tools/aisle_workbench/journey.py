"""Drive a running Studio via Makepad pointer events and verify retained AISLE results."""

import argparse
import json
import sys
from pathlib import Path
import subprocess
import time


def layout_seed(value):
    seed = int(value)
    if not 0 <= seed < 2**32:
        raise argparse.ArgumentTypeError("Layout seeds range from 0 to 4294967295")
    return seed


def main():
    from worker import observation_seconds

    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--remote", required=True)
    p.add_argument("--pid", required=True)
    p.add_argument("--report", type=Path, required=True)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--screenshots", action="store_true")
    models = p.add_mutually_exclusive_group()
    models.add_argument("--codex", dest="provider", action="store_const", const="codex")
    models.add_argument(
        "--octos-deepseek",
        dest="provider",
        action="store_const",
        const="octos-deepseek",
    )
    p.add_argument("--size", choices=["1024x768", "1440x900"])
    p.add_argument(
        "--recover",
        action="store_true",
        help="Verify a previously stopped run after restarting Studio",
    )
    p.add_argument("--resume-fleet", action="store_true")
    p.add_argument("--fleet-controller", action="store_true")
    p.add_argument("--fleet-node")
    p.add_argument("--fleet", action="store_true")
    p.add_argument(
        "--background-period-s", type=observation_seconds, choices=(0.5, 1, 2, 5)
    )
    p.add_argument(
        "--background-clock", choices=("simulation", "wall"), default="simulation"
    )
    p.add_argument("--foreground-cadence", type=int, choices=range(2, 65))
    p.add_argument("--standing", action="store_true")
    p.add_argument("--device-preflight", action="store_true")
    p.add_argument("--pickup-motion", choices=["slide-positive", "slide-negative"])
    p.add_argument("--transfer", choices=["red", "three"])
    p.add_argument("--layout", type=layout_seed, default=0)
    p.add_argument("--wrist-clearance", action="store_true")
    p.add_argument("--heading", nargs="?", const=20, type=int, choices=(20, 30))
    p.add_argument("--upright", action="store_true")
    p.add_argument("--impact", action="store_true")
    p.add_argument("--train-grasp", action="store_true")
    p.add_argument("--train-reach", action="store_true")
    p.add_argument("--photo-input", type=Path)
    p.add_argument("--survey", type=Path)
    p.add_argument("--evaluation", type=Path)
    p.add_argument("--compare-cohorts", nargs=2, type=Path)
    p.add_argument("--reject-cohort", type=Path)
    p.add_argument("--handoff", action="store_true")
    p.add_argument("--revision-selection", action="store_true")
    p.add_argument("--wrist-faults", nargs="?", const="red", choices=["red", "three"])
    p.add_argument("--camera-faults", action="store_true")
    p.add_argument("--moving-target", action="store_true")
    p.add_argument("--observed-author", action="store_true")
    p.add_argument("--prepared-pickup", action="store_true")
    p.add_argument("--revise-drive", action="store_true")
    p.add_argument("--revise-pickup", action="store_true")
    p.add_argument("--compare-runs", action="store_true")
    a = p.parse_args()
    a.out.mkdir(parents=True, exist_ok=True)
    checks = []

    def report():
        for _ in range(40):
            try:
                return json.loads(a.report.read_text())
            except (FileNotFoundError, json.JSONDecodeError):
                time.sleep(0.1)
        raise RuntimeError("Studio did not write an instrumentation report")

    def wait(test, label, seconds=30, *, advancing=False):
        deadline = time.monotonic() + seconds
        last_step = -1
        while time.monotonic() < deadline:
            j = report()
            if test(j):
                return j
            if advancing:
                workbench = j["state"]["aisle"]
                step = (workbench.get("progress") or {}).get("step", -1)
                if workbench["busy"] and step > last_step:
                    deadline = time.monotonic() + seconds
                    last_step = step
            time.sleep(0.12)
        raise RuntimeError(f"{label}: {report()['state'].get('aisle')}")

    def remote(*args):
        result = subprocess.run(
            [a.remote, "app", a.pid, *args],
            capture_output=True,
            text=True,
            check=True,
            timeout=40,
        )
        for line in result.stdout.splitlines():
            response = json.loads(line)
            if "err" in response or response.get("verified") is False:
                raise RuntimeError(response)

    def click(key):
        j = wait(
            lambda j: j["controls"][key]["visible"]
            and j["controls"][key]["rect"][2] > 0
            and j["controls"][key]["enabled"] is not False,
            key,
        )
        # The probe writes during sync. Wait for Makepad to draw the new layout
        # before choosing screen coordinates for a newly enabled widget.
        for _ in range(8):
            prior = j["controls"][key]["rect"]
            time.sleep(0.12)
            j = report()
            if j["controls"][key]["rect"] == prior and j["controls"][key]["visible"]:
                break
        x, y, w, h = j["controls"][key]["rect"]
        remote("click", "w=0", f"x={x + w / 2}", f"y={y + h / 2}", "wait=1")

    def page(name):
        click("workspace_tabs.plugin_tabs.plugin_tab_aisle_" + name)
        wait(lambda j: j["state"]["aisle_panel"] == "aisle_" + name, name)

    def done(test, label, seconds=30, *, advancing=False):
        row = wait(
            lambda j: test(j["state"]["aisle"]), label, seconds, advancing=advancing
        )["state"]["aisle"]
        checks.append(dict(check=label, robot=row["robot"], status=row["status"]))
        return row

    def grab(name):
        if a.screenshots:
            time.sleep(0.4)
            remote("grab", "w=0", "out=" + str((a.out / (name + ".png")).resolve()))

    if a.size:
        width, height = a.size.split("x")
        remote(
            "window", "k=resize", "width=" + width, "height=" + height, "w=0", "wait=1"
        )
    if a.revise_drive:
        from drive_check import native_journey

        native_journey(a, page, click, remote, done, grab)
        return
    if a.revise_pickup:
        from revision_check import native_journey

        native_journey(a, page, click, remote, done, grab)
        return
    if a.compare_runs:
        from divergence_check import native_journey

        native_journey(a, page, click, done, wait, grab)
        return
    if a.fleet:
        from fleet_check import native_journey

        native_journey(a, checks, page, click, remote, done, grab, wait)
        return
    if a.observed_author:
        from observed_author_check import journey

        journey(a, checks, page, click, remote, done, grab)
        return
    if a.moving_target:
        from moving_target_check import native_journey

        native_journey(a, checks, page, click, remote, done, grab, wait)
        return
    if a.wrist_faults:
        from wrist_timing_check import native_journey

        return native_journey(a, checks, page, click, remote, done, grab)
    if a.camera_faults:
        from sensor_timing_check import native_journey

        native_journey(a, checks, page, click, remote, done, grab)
        return
    if a.revision_selection:
        from selection_check import native_journey

        return native_journey(a, checks, page, click, remote, done, grab)
    if a.handoff:

        def enter_task(instruction):
            click("aisle_task.instruction")
            remote(
                "key",
                "w=0",
                "c=KeyA",
                "logo=1" if sys.platform == "darwin" else "ctrl=1",
                "wait=1",
            )
            remote("text", "w=0", "t=" + instruction, "wait=1")

        def inspect_with(planner, button, instruction, delta):
            page("task")
            if planner == "codex":
                click("aisle_task.g1")
            enter_task(instruction)
            click("aisle_task." + button)
            row = done(
                lambda w: not w["busy"]
                and w["controller"]
                and w["controller"].get("provider") == planner,
                planner + " revision prepared",
                180,
            )
            plan = row["controller"]
            assert (
                plan["joint"] == "right_shoulder_pitch_joint"
                and abs(plan["delta_rad"] - delta) < 1e-5
            )
            previous = row["selected"]
            page("run")
            click("aisle_run.start_run")
            row = done(
                lambda w: not w["busy"]
                and w["selected"] != previous
                and w["result"]
                and w["result"].get("task_success"),
                planner + " shoulder trial scored",
                120,
            )
            return plan, row["result"]

        if a.recover:
            page("evidence")
            row = done(
                lambda w: w["result"]
                and w["result"].get("task_success")
                and w["robot"] == "G1",
                "Revised G1 trial reopened",
            )
            saved = json.loads((a.out / "journey.json").read_text())[-1]
            assert (
                row["result"]["controller_hash"]
                == saved["revised_result"]["controller_hash"]
            )
            (a.out / "recovery.json").write_text(json.dumps(checks, indent=2))
            return
        original, measured = inspect_with(
            "codex",
            "codex",
            "Inspect the right shoulder pitch using minus 0.08 rad.",
            -0.08,
        )
        revised, result = inspect_with(
            "octos-deepseek",
            "revise",
            "Repeat the preceding inspection at three quarters of its signed displacement.",
            -0.06,
        )
        assert (
            revised["previous_trial"]["controller_hash"] == measured["controller_hash"]
        )
        assert abs(result["measured_delta"] + 0.06) < 0.003
        page("evidence")
        grab("provider-handoff")
        checks.append(
            dict(
                check="Measured cross-provider revision",
                original=original,
                original_result=measured,
                revised=revised,
                revised_result=result,
            )
        )
        (a.out / "journey.json").write_text(json.dumps(checks, indent=2))
        return
    if a.compare_cohorts:
        from cohort_journey import run

        run(a, page, click, remote, done, grab, checks)
        return
    if a.evaluation:
        import hashlib

        expected = hashlib.sha256(
            a.evaluation.with_name("suite.json").read_bytes()
        ).hexdigest()
        page("evidence")
        click("aisle_evidence.evaluation_mode")
        if not a.recover:
            click("aisle_evidence.archive_path")
            remote("text", "w=0", "t=" + str(a.evaluation), "wait=1")
            click("aisle_evidence.verify_archive")
        row = done(
            lambda w: not w["busy"]
            and w["evaluation"]
            and w["evaluation"]["manifest_sha256"] == expected,
            "Layout evaluation restored" if a.recover else "Layout archive verified",
            100,
        )
        evaluation = row["evaluation"]
        assert (
            len(evaluation["failures"]) + evaluation["successes"] == evaluation["cases"]
        )
        assert (
            evaluation["archive_sha256"]
            == hashlib.sha256(a.evaluation.read_bytes()).hexdigest()
        )
        if not a.recover:
            profile = (
                "G1" if evaluation.get("profile") == "g1-standing-reach-v1" else "Nano"
            )
            assert row["status"] == f"Evaluation verified · {profile}"
        checks[-1]["evaluation"] = evaluation
        if len(evaluation["failures"]) > 1 and not a.recover:
            click("aisle_evidence.later_failure")
            done(lambda w: w["failure_index"] == 1, "Second failed layout opened")
            click("aisle_evidence.earlier_failure")
            done(lambda w: w["failure_index"] == 0, "First failed layout reopened")
        if not a.recover:
            grab("layout-evaluation")
            missing = a.evaluation.with_name("missing-archive-for-ui-check.zip")
            assert not missing.exists()
            click("aisle_evidence.archive_path")
            remote(
                "key",
                "w=0",
                "c=KeyA",
                "logo=1" if sys.platform == "darwin" else "ctrl=1",
                "wait=1",
            )
            remote("text", "w=0", "t=" + str(missing), "wait=1")
            click("aisle_evidence.verify_archive")
            rejected = done(
                lambda w: not w["busy"] and missing.name in w["status"],
                "Unreadable archive rejected",
            )
            assert rejected["evaluation"] == evaluation
        name = "recovery.json" if a.recover else "journey.json"
        (a.out / name).write_text(json.dumps(checks, indent=2))
        return
    if a.standing or a.transfer:
        kind = "standing-inspection" if a.standing else "table-transfer"
        label = "standing" if a.standing else "transfer"
        robot = "g1" if a.standing else "nano"
        wanted = ["red"] if a.transfer == "red" else ["red", "green", "blue"]
        if a.recover:
            page("evidence")
            row = done(
                lambda w: w["result"] and w["result"].get("kind") == kind,
                label + " evidence restored",
            )
            assert (row["result"]["task_success"] or a.train_grasp) and (
                a.standing or row["result"]["requested"] == wanted
            )
            assert (
                row["robot"]
                == row["result"]["robot"]
                == ("G1" if a.standing else "Nano")
            )
            checks[-1].update(
                restored_robot=row["result"]["robot"],
                controller_hash=row["result"]["controller_hash"],
                fixture=row["result"]["fixture"],
                outcome=row["result"]["task_success"],
            )
            if a.transfer and a.layout > 2 and not a.train_grasp:
                assert row["layout_seed"] == row["result"]["seed"] == a.layout
            if a.device_preflight:
                click("aisle_evidence.device_mode")
                pre = done(
                    lambda w: w["show_device"]
                    and (w["preflight"] or {}).get("controller_sha256")
                    == row["result"]["controller_hash"],
                    "Device report restored with the matching trial",
                )
                checks[-1]["preflight"] = pre["preflight"]
            if a.train_grasp:
                ck = row["controller"]["grasp_checkpoint"]
                assert ck and row["result"]["grasp_checkpoint"] == ck
                assert row["result"]["seed"] == (a.layout + 1) % 3
                checks[-1]["grasp_checkpoint"] = ck
            if a.train_reach:
                assert (
                    row["result"]["reach_checkpoint"]
                    == row["controller"]["reach_checkpoint"]
                )
                checks[-1]["reach_checkpoint"] = row["result"]["reach_checkpoint"]
            if a.photo_input:
                assert (
                    row["room_info"]["photo_draft"]
                    and row["result"]["room_revision_sha256"]
                )
                checks[-1]["photo_room"] = row["room_info"]
            if a.survey:
                assert row["room_info"]["obstacles"][0]["position"][:2] == [1.38, -0.45]
                assert row["result"]["room_revision_sha256"]
            (a.out / "recovery.json").write_text(json.dumps(checks, indent=2))
            return
        page("task")
        click("aisle_task." + robot)
        click("aisle_task.instruction")
        remote(
            "key",
            "w=0",
            "c=KeyA",
            "logo=1" if sys.platform == "darwin" else "ctrl=1",
            "wait=1",
        )
        request = (
            "Move the red floor cube onto the coffee table."
            if a.transfer == "red"
            else "Move all three floor cubes onto the coffee table in red, green, blue order."
        )
        if a.wrist_clearance:
            request += " Use experimental wrist clearance."
        if a.pickup_motion:
            request += " Track the moving cube with the wrist camera as the arm descends for pickup."
        if a.standing:
            request = "Stand on both feet and track a right shoulder displacement of minus 0.08 rad for the standing test."
        if a.heading:
            request += (
                f" Turn the waist {a.heading} degrees to the left, with neutral pitch."
            )
        if a.upright:
            request += " Use the upright trunk posture."
        remote("text", "w=0", "t=" + request, "wait=1")
        click(
            "aisle_task.octos" if a.provider == "octos-deepseek" else "aisle_task.codex"
        )
        row = done(
            lambda w: w["controller"]
            and w["controller"].get("kind") == kind
            and not w["busy"],
            label + " controller authored",
            180,
        )
        assert (
            a.standing or row["controller"]["color_order"] == ",".join(wanted)
        ) and row["controller"]["provider"] == a.provider
        if a.wrist_clearance:
            assert row["controller"].get("wrist_clearance") is True
        grab(label + "-task")
        page("scene")
        if a.photo_input:
            from photo_room_check import draft

            photo_object, photo_revision = draft(a, checks, click, remote, done, grab)
        if a.survey:
            click("aisle_scene.room_mode")
            done(lambda w: w["room_editor"], "Room survey editor opened")

            def fill_survey(field, text):
                click("aisle_scene." + field)
                remote(
                    "key",
                    "w=0",
                    "c=KeyA",
                    "logo=1" if sys.platform == "darwin" else "ctrl=1",
                    "wait=1",
                )
                remote("text", "w=0", "t=" + text, "wait=1")

            fill_survey("survey_path", str(a.survey))
            click("aisle_scene.import_survey")
            saved = done(
                lambda w: w["room_path"] and not w["busy"],
                "Surveyed footprint imported",
            )
            first = saved["room_path"]
            mounts = (
                saved["room_info"].get("settings", {}).get("camera_mount_checks", {})
            )
            if mounts:
                labels = {
                    "head": "Head",
                    "left_wrist": "Left wrist",
                    "right_wrist": "Right wrist",
                }
                lines = [
                    f"{labels[k]} · {v['max_check_px']:.2f} px max on {v['check_points']} check points"
                    for k, v in mounts.items()
                ]
                wait(
                    lambda j: all(
                        line in j["controls"]["aisle_scene.room_detail"]["text"]
                        for line in lines
                    ),
                    "Camera validation visible on Scene",
                )
                checks.append(
                    dict(
                        check="Displayed independent camera reprojection checks",
                        cameras=mounts,
                    )
                )
                grab("survey-camera-fit")
                archived = Path(first).with_name("survey.json")
                bad = json.loads(archived.read_bytes())
                bad["image"] = str(archived.with_name("reference-image"))
                name = next(iter(mounts))
                point = next(
                    m
                    for m in bad["camera_calibration"]["cameras"][name]["landmarks"]
                    if m["use"] == "check"
                )
                point["pixel"][0] += 10 if point["pixel"][0] < 240 else -10
                incorrect = a.out.parent / "incorrect-camera-survey.json"
                incorrect.write_text(json.dumps(bad))
                fill_survey("survey_path", str(incorrect))
                click("aisle_scene.import_survey")
                stopped = done(
                    lambda w: not w["busy"]
                    and "check landmark reprojection" in w["status"],
                    "Rejected the incorrect camera measurement",
                )
                assert stopped["room_path"] == first
                checks[-1]["reason"] = stopped["status"]
                # The camera report can push the geometry editor below the
                # scroll viewport. Use the same wheel gesture as an operator.
                x, y, width, height = report()["controls"]["aisle_scene.room_detail"]["rect"]
                remote("mouse", "k=scroll", "w=0", f"x={x + width / 2}",
                       f"y={y + height / 2}", "dy=320", "wait=1")
            fill_survey("geometry", "ottoman 0 0 0.35")
            click("aisle_scene.save_geometry")
            failed = done(
                lambda w: "conflicts with Nano" in w["status"] and not w["busy"],
                "Overlapping edit rejected",
            )
            assert failed["room_path"] == first
            fill_survey("geometry", "ottoman 1.38 -0.45 0.35")
            click("aisle_scene.save_geometry")
            saved = done(
                lambda w: w["room_path"] != first and not w["busy"],
                "Edited geometry saved as a revision",
            )
            assert saved["room_info"]["obstacles"][0]["position"][:2] == [1.38, -0.45]
            grab("survey-edited")
            click("aisle_scene.robot_mode")
        if a.impact:
            click("aisle_scene.impact")
            done(lambda w: w["impact"], "G1 impact selected")
        if a.pickup_motion:
            assert row["controller"]["track_grasp"] is True
            click(
                "aisle_scene.slide_pos"
                if a.pickup_motion == "slide-positive"
                else "aisle_scene.slide_neg"
            )
            done(
                lambda w: w["target_motion"] == a.pickup_motion,
                "Selected moving floor cube",
            )
        if a.transfer:
            for seed in [(a.layout + 1) % 3, a.layout]:
                if report()["state"]["aisle"]["layout_seed"] == seed:
                    continue
                if seed <= 2:
                    click("aisle_scene.layout" + str(seed))
                else:
                    click("aisle_scene.seed_input")
                    remote("text", "w=0", "t=4294967296", "wait=1")
                    click("aisle_scene.apply_seed")
                    unchanged = done(
                        lambda w: "Enter a layout seed" in w["status"],
                        "Out-of-range layout rejected",
                    )
                    assert unchanged["layout_seed"] == (a.layout + 1) % 3
                    click("aisle_scene.seed_input")
                    remote(
                        "key",
                        "w=0",
                        "c=KeyA",
                        "logo=1" if sys.platform == "darwin" else "ctrl=1",
                        "wait=1",
                    )
                    remote("text", "w=0", "t=" + str(seed), "wait=1")
                    click("aisle_scene.apply_seed")
                done(
                    lambda w: w["layout_seed"] == seed,
                    f"Living-room layout {seed} selected",
                )

        click("aisle_scene.inspect")
        done(
            lambda w: not w["busy"] and w["scene_joints"] == (29 if a.standing else 17),
            robot + " scene checked",
        )
        grab(label + "-scene")
        page("run")
        click("aisle_run.start_run")
        live = done(
            lambda w: w["busy"] and w["progress"] and w["progress"].get("kind") == kind,
            label + " stream active",
            90,
        )
        step = live["progress"]["step"]
        done(
            lambda w: w["busy"] and w["progress"]["step"] > step, label + " fresh frame"
        )
        grab(label + "-run")
        row = done(
            lambda w: not w["busy"] and w["result"] and w["result"].get("kind") == kind,
            label + " adjudicated",
            180 if a.transfer == "three" else 600,
            advancing=a.transfer == "three",
        )
        result = row["result"]
        if a.pickup_motion:
            from moving_grasp_check import verify_trial

            proof = verify_trial(Path(row["selected"]))
            checks.append(
                dict(check="Onboard descent and physical moving grasp", audit=proof)
            )
        page("evidence")
        grab(label + "-evidence")
        checks.append(dict(check=label + " physical verdict", **result))
        (a.out / "journey.json").write_text(json.dumps(checks, indent=2))
        assert result["task_success"], result
        if a.device_preflight:
            click("aisle_evidence.device_mode")
            done(lambda w: w["show_device"], "Device preflight opened")
            if a.standing:
                click("aisle_evidence.device_endpoint")
                remote(
                    "text",
                    "w=0",
                    "t=" + ("lo0" if sys.platform == "darwin" else "lo") + "@227",
                    "wait=1",
                )
                click("aisle_evidence.device_listen")
                absence = done(
                    lambda w: not w["busy"]
                    and (w["preflight"] or {}).get("state") == "Dry run stopped",
                    "Missing external publisher reported",
                    35,
                )
                assert (
                    absence["preflight"]["source_kind"] == "External DDS telemetry"
                    and absence["preflight"]["motor_commands_sent"] == 0
                )
                checks[-1]["receipt"] = absence["preflight"]
                click("aisle_evidence.device_replay")
                active = done(
                    lambda w: w["busy"] and w["worker_pid"], "Device preflight running"
                )
                group = active["worker_pid"]

                def capture_has_started(_):
                    preflights = sorted(
                        Path(row["selected"]).parent.glob("*-G1-device-preflight")
                    )
                    return preflights and (preflights[-1] / "lowstate.jsonl").is_file()

                wait(capture_has_started, "DDS subscriber joined")
                click("aisle_evidence.device_stop")
                cancelled = done(
                    lambda w: not w["busy"]
                    and (w["preflight"] or {}).get("reason") == "Stopped by operator",
                    "Operator stopped DDS capture",
                )
                assert cancelled["preflight"]["motor_commands_sent"] == 0

                def capture_has_exited(_):
                    output = subprocess.check_output(
                        ["ps", "-axo", "pgid=,stat="], text=True
                    )
                    return not any(
                        int(fields[0]) == group and not fields[1].startswith("Z")
                        for fields in (line.split() for line in output.splitlines())
                        if len(fields) == 2
                    )

                wait(capture_has_exited, "All DDS processes exited")
                checks[-1]["process_group_exited"] = True
            click("aisle_evidence.device_replay")
            expected = "Dry run measured" if a.standing else "Adapter incomplete"
            observed = done(
                lambda w: not w["busy"]
                and (w["preflight"] or {}).get("state") == expected,
                "Selected controller device preflight",
                55,
            )
            pre = observed["preflight"]
            assert (
                pre["controller_sha256"] == result["controller_hash"]
                and pre["motor_commands_sent"] == 0
                and pre["physical_qualification"] is False
            )
            if a.standing:
                assert (
                    pre["dry_run_steps"] == 2200
                    and pre["joints"] == 29
                    and pre["max_nexus_target_difference_rad"] <= 1e-5
                )
                assert pre["source_kind"] == "Nexus loopback replay"
            else:
                assert pre["capabilities"]["arms"] is False and len(pre["wheels"]) == 3
            checks[-1]["receipt"] = pre
            grab("device-preflight")
            if a.screenshots:
                from PIL import Image

                controls = report()["controls"]
                x, y, _, h = controls["aisle_evidence.device_replay"]["rect"]
                bottom = controls["aisle_evidence.device_folder"]["rect"][1]
                with Image.open(a.out / "device-preflight.png") as shot:
                    region = shot.convert("L").crop(
                        (int(x), int(y + h + 4), shot.width - 50, int(bottom - 4))
                    )
                    histogram = region.histogram()
                background = max(range(256), key=histogram.__getitem__)
                legible_pixels = sum(
                    n for i, n in enumerate(histogram) if abs(i - background) > 100
                )
                assert legible_pixels >= 150, (
                    "Preflight measurements lack visible text contrast"
                )
                checks[-1]["contrasting_result_pixels"] = legible_pixels
            click("aisle_evidence.trial_mode")
            done(
                lambda w: not w["show_device"] and not w["show_evaluation"],
                "Simulation evidence reopened",
            )

        if a.standing:
            if a.heading:
                import numpy as np

                heading = {20: 0.34906585, 30: 0.52359878}[a.heading]
                assert result["trunk_yaw_target_rad"] == heading
                assert abs(result["settled_trunk_yaw_rad"] - heading) < 0.015
                with np.load(Path(row["selected"]) / "motor-work.npz") as trace:
                    joint = trace["joints"].tolist().index("waist_yaw_joint")
                    assert np.allclose(
                        trace["target"][:, joint], heading, atol=1e-7, rtol=0
                    )
            if a.upright:
                assert result["trunk_pitch_target_rad"] == -0.10
                import numpy as np

                with np.load(Path(row["selected"]) / "motor-work.npz") as motors:
                    i = list(motors["joints"]).index("waist_pitch_joint")
                    np.testing.assert_allclose(motors["target"][:, i], -0.10, atol=1e-7)
                    np.testing.assert_allclose(
                        motors["q_after"][-100:, i].mean(),
                        result["settled_trunk_pitch_rad"],
                    )
            assert result["both_feet_loaded_steps"] == result["steps"] == 2000
            assert abs(result["measured_delta"] + 0.08) < 0.012
            if a.impact:
                assert (
                    result["disturbance"]["passed"]
                    and result["disturbance"]["contacts"] > 0
                )
                assert result["estimated_positive_joint_work_j"] > 0
            click("aisle_evidence.export")
            saved = done(
                lambda w: w["dataset"] and not w["busy"],
                "G1 standing episode exported",
                120,
            )
            import numpy as np
            import pyarrow.parquet as pq

            dataset = Path(saved["dataset"])
            trial = Path(saved["selected"])
            info = json.loads((dataset / "meta/info.json").read_text())
            lineage = json.loads((dataset / "meta/aisle.json").read_text())
            assert (
                lineage["first_physics_step"] == 20
                and lineage["physics_step_s"] == 0.005
            )
            assert info["total_frames"] == 100 and info["fps"] == 10
            assert (
                info["features"]["action"]["shape"] == [29]
                and lineage["controller_hash"] == result["controller_hash"]
            )
            table = pq.read_table(next(dataset.glob("data/*/*.parquet"))).to_pydict()
            with np.load(trial / "motor-work.npz", allow_pickle=False) as motors:
                np.testing.assert_allclose(
                    table["action"], motors["target"][19::20], atol=1e-7
                )
                np.testing.assert_allclose(
                    table["observation.state"], motors["q_after"][19::20], atol=1e-7
                )
            if a.train_reach:
                click("aisle_evidence.train_reach")
                trained = done(
                    lambda w: not w["busy"] and w["controller"].get("reach_checkpoint"),
                    "G1 shoulder correction trained from exported dataset",
                )
                checkpoint = trained["controller"]["reach_checkpoint"]
                assert (
                    trained["controller"]["parent_controller_sha256"]
                    == result["controller_hash"]
                )
                page("run")
                click("aisle_run.start_run")
                learned = done(
                    lambda w: not w["busy"]
                    and w["result"].get("reach_checkpoint") == checkpoint,
                    "G1 checkpoint evaluated in Nexus",
                    180,
                )
                assert learned["result"]["task_success"]
                assert (
                    learned["result"]["training_dataset"]
                    == trained["controller"]["training_dataset"]
                )
                checks[-1]["measured"] = learned["result"]
                page("evidence")
                grab("standing-checkpoint")
                click("aisle_evidence.previous")
                done(
                    lambda w: w["selected"] == str(trial), "Teacher comparison reopened"
                )
            click("aisle_evidence.replay")
            wait(
                lambda j: j["state"]["screen"] == "Replay"
                and j["state"]["frames"] == 100,
                "G1 standing episode opened in native Replay",
            )
            grab("standing-replay")
            checks.append(
                dict(
                    check="G1 native Replay and synchronized encoder/action columns",
                    cameras=["operator"],
                    frames_per_camera=100,
                    joints=29,
                )
            )
            (a.out / "journey.json").write_text(json.dumps(checks, indent=2))
        else:
            if a.photo_input:
                from photo_room_check import verify_run

                body = verify_run(result, photo_object, photo_revision)
                checks.append(
                    dict(
                        check="Nexus measured the approved photographic geometry",
                        body=body,
                        room_sha256=photo_revision,
                    )
                )
            if a.survey:
                assert result["room_revision_sha256"] and result["room_obstacles"] == [
                    "room_ottoman"
                ]
                measured = result["measured_room_bodies"]["room_ottoman"]["center_m"]
                assert all(
                    abs(x - y) < 1e-5
                    for x, y in zip(measured, [1.38, -0.45, 0.1407], strict=True)
                )
                run = Path(row["selected"])
                calibration = json.loads((run / "calibration.json").read_text())
                scene = json.loads((run / "room.json").read_text())
                assert calibration["cameras"] == scene["settings"]["cameras"]
            assert result["seed"] == a.layout
            assert all(result["cubes"][c]["passed"] for c in wanted)
            import numpy as np
            import pyarrow.parquet as pq

            click("aisle_evidence.export")
            saved = done(
                lambda w: w["dataset"] and not w["busy"],
                "Onboard episode exported",
                180,
            )
            dataset = Path(saved["dataset"])

            def read(p):
                return json.loads(p.read_text())

            record = read(Path(row["selected"]) / "recording.json")
            info = read(dataset / "meta/info.json")
            count = result["physics_ticks"] // 20
            assert count == result["frames"] == info["total_frames"]
            assert info["fps"] == 5 and info["total_videos"] == 3
            signals = pq.read_table(next(dataset.glob("data/*/*.parquet"))).to_pydict()
            for feature, field in (
                ("observation.state", "state"),
                ("action", "action"),
            ):
                assert info["features"][feature]["names"] == record["joints"]
                np.testing.assert_allclose(
                    signals[feature],
                    [v[field] for v in record["samples"]],
                    rtol=1e-6,
                    atol=1e-6,
                )
            np.testing.assert_allclose(
                signals["timestamp"], np.arange(count) / 5, atol=0.00003
            )
            for cam in ("head", "left_wrist", "right_wrist"):
                video = (
                    dataset
                    / "videos/chunk-000"
                    / f"observation.images.{cam}"
                    / "episode_000000.mp4"
                )
                data = subprocess.check_output(
                    [
                        "ffprobe",
                        "-v",
                        "error",
                        "-count_frames",
                        "-show_entries",
                        "stream=nb_read_frames,width,height",
                        "-of",
                        "json",
                        str(video),
                    ],
                    text=True,
                )
                stream = json.loads(data)["streams"][0]
                assert (
                    int(stream["nb_read_frames"]),
                    stream["width"],
                    stream["height"],
                ) == (count, 480, 360)
            if a.train_grasp:
                from nano_grasp_check import native_journey

                np.testing.assert_array_equal(
                    signals["annotation.gripper_carry"],
                    [r["airborne_in_jaw"] for r in record["samples"]],
                )
                native_journey(
                    a, Path(row["selected"]), result, checks, page, click, done, grab
                )
            click("aisle_evidence.replay")
            wait(
                lambda j: j["state"]["screen"] == "Replay"
                and j["state"]["frames"] == count,
                "Onboard episode opened in native Replay",
            )
            grab("transfer-replay")
            checks.append(
                dict(
                    check="Native Replay and synchronized encoder/action columns",
                    cameras=record["cameras"],
                    frames_per_camera=count,
                )
            )
            (a.out / "journey.json").write_text(json.dumps(checks, indent=2))
        return
    if a.recover:
        page("evidence")
        row = done(
            lambda w: w["result"] and w["result"].get("outcome") == "Stopped",
            "stopped run recovered on restart",
        )
        assert (Path(row["selected"]) / "stopped.json").exists()
        page("task")
        click("aisle_task.g1")
        done(
            lambda w: w["controller"] and w["controller"]["robot"] == "G1",
            "G1 controller restored",
        )
        (a.out / "recovery.json").write_text(json.dumps(checks, indent=2))
        return
    if a.provider:
        page("task")
        click("aisle_task.nano")
        click("aisle_task.instruction")
        remote(
            "key",
            "w=0",
            "c=KeyA",
            "logo=1" if sys.platform == "darwin" else "ctrl=1",
            "wait=1",
        )
        remote(
            "text",
            "w=0",
            "t=Drive within 45 cm of the red floor cube and stop.",
            "wait=1",
        )
        click(
            "aisle_task.octos" if a.provider == "octos-deepseek" else "aisle_task.codex"
        )
        done(
            lambda w: w["controller"]
            and w["controller"].get("kind") == "visual-approach"
            and w["controller"]["provider"] == a.provider
            and not w["busy"],
            "Nano RGB approach authored",
            180,
        )
        page("scene")
        grab("Nano-approach-scene")
        page("run")
        click("aisle_run.start_run")
        live = done(
            lambda w: w["busy"]
            and w.get("progress")
            and w["progress"].get("kind") == "visual-approach"
            and w["progress"]["state"] == "approaching",
            "Nano live RGB feedback while wheels move",
            120,
        )
        first_step = live["progress"]["step"]
        done(
            lambda w: w["busy"]
            and w.get("progress")
            and w["progress"]["step"] > first_step,
            "Nano streamed a second onboard observation",
            30,
        )
        grab("Nano-approach-moving")
        row = done(
            lambda w: w["result"]
            and w["result"].get("kind") == "visual-approach"
            and not w["busy"],
            "Nano RGB approach completed",
            140,
        )
        result = row["result"]
        assert result["task_success"] and result["controller"]["provider"] == a.provider
        assert result["color"] == "red" and result["standoff_m"] == 0.45
        assert (
            result["obstacle_contact_ticks"] == 0
            and result["wheel_supported_ticks"] == result["physics_ticks"]
        )
        assert abs(result["final_distance_m"] - 0.45) < 0.035
        assert (
            Path(row["selected"]) / "preview.rgb"
        ).stat().st_size == 4 + 480 * 360 * 3
        run_dir = Path(row["selected"])
        import numpy as np

        packets = sorted((run_dir / "observations").glob("*.npz"))
        assert len(packets) == result["decisions"]
        for packet in packets:
            with np.load(packet, allow_pickle=False) as samples:
                assert set(samples) == {
                    "head",
                    "left_wrist",
                    "right_wrist",
                    "encoders",
                    "stamp_ns",
                }
                assert samples["encoders"].shape == (17,)
                assert samples["head"].shape == (360, 480, 3)
        grab("Nano-approach-run")
        page("evidence")
        grab("Nano-approach-evidence")
        checks.append(dict(check="Nano RGB approach independently measured", **result))
    for robot, joints in [("Nano", 17), ("G1", 29)]:
        page("task")
        click("aisle_task." + robot.lower())
        done(lambda w: w["robot"] == robot, robot + " profile")
        if a.provider and robot == "G1":
            click("aisle_task.instruction")
            remote(
                "key",
                "w=0",
                "c=KeyA",
                "logo=1" if sys.platform == "darwin" else "ctrl=1",
                "wait=1",
            )
            remote(
                "text",
                "w=0",
                "t=Inspect the right shoulder with a displacement of minus 0.08 rad",
                "wait=1",
            )
            click(
                "aisle_task.octos"
                if a.provider == "octos-deepseek"
                else "aisle_task.codex"
            )
        else:
            click("aisle_task.reference")
        row = done(
            lambda w: w["controller"]
            and w["controller"]["robot"] == robot
            and not w["busy"],
            robot + " prepare",
            180,
        )
        if a.provider and robot == "G1":
            assert (
                row["controller"]["provider"] == a.provider
                and row["controller"]["delta_rad"] < 0
            )
        grab(robot + "-task")
        page("scene")
        click("aisle_scene.inspect")
        done(
            lambda w: w["scene_joints"] == joints
            and not w["busy"]
            and w["status"].endswith("scene"),
            robot + " model",
        )
        if robot == "G1":
            click("aisle_scene.compliant")
            done(lambda w: w["compliant"], "G1 compliant fixture selected")
        grab(robot + "-scene")
        page("run")
        before = report()["state"]["aisle"]["selected"]
        click("aisle_run.start_run")
        row = done(
            lambda w: w["selected"] != before
            and w["result"]
            and w["result"]["robot"] == robot
            and not w["busy"],
            robot + " run",
            100,
        )
        path = Path(row["selected"])
        assert row["live"] == row["selected"]
        result = json.loads((path / "result.json").read_text())
        assert (
            result == row["result"]
            and result["task_success"]
            and result["within_limits"]
        )
        assert result["fixture"] == ("standard" if robot == "Nano" else "compliant")
        assert (
            len(json.loads((path / "trajectory.json").read_text())) == result["frames"]
            and (path / "frame.png").stat().st_size > 300
        )
        grab(robot + "-run")
        page("evidence")
        click("aisle_evidence.export")
        saved = done(
            lambda w: w["dataset"] and not w["busy"], robot + " episode exported", 60
        )
        dataset = Path(saved["dataset"])
        assert dataset.parent != path and not (path / "dataset").exists()
        import pyarrow.parquet as pq
        import numpy as np

        source = json.loads((path / "recording.json").read_text())
        table = pq.read_table(next(dataset.glob("data/*/*.parquet"))).to_pydict()
        for name, raw in [("observation.state", "state"), ("action", "action")]:
            np.testing.assert_allclose(
                table[name], [r[raw] for r in source["samples"]], atol=1e-7, rtol=0
            )
        np.testing.assert_allclose(
            table["timestamp"], np.arange(25) / 20, atol=1e-7, rtol=0
        )
        info = json.loads((dataset / "meta/info.json").read_text())
        assert info["total_frames"] == 25 and info["features"]["observation.state"][
            "shape"
        ] == [joints]
        video = (
            dataset / "videos/chunk-000/observation.images.operator/episode_000000.mp4"
        )
        measured = subprocess.check_output(
            [
                "ffprobe",
                "-v",
                "error",
                "-count_frames",
                "-show_entries",
                "stream=nb_read_frames,width,height",
                "-of",
                "json",
                str(video),
            ],
            text=True,
        )
        stream = json.loads(measured)["streams"][0]
        assert (int(stream["nb_read_frames"]), stream["width"], stream["height"]) == (
            25,
            480,
            320,
        )
        grab(robot + "-evidence")
        checks.append(dict(check=robot + " scored and retained", **result))
    click("aisle_evidence.previous")
    done(lambda w: w["result"] and w["result"]["robot"] == "Nano", "previous evidence")
    click("aisle_evidence.next")
    done(lambda w: w["result"] and w["result"]["robot"] == "G1", "next evidence")
    page("task")
    if a.provider:
        click("aisle_task.instruction")
        remote(
            "key",
            "w=0",
            "c=KeyA",
            "logo=1" if sys.platform == "darwin" else "ctrl=1",
            "wait=1",
        )
        remote(
            "text",
            "w=0",
            "t=Repeat the previous trial on the same shoulder with three quarters of the previously requested signed displacement",
            "wait=1",
        )
        click(
            "aisle_task.revise"
            if a.provider == "octos-deepseek"
            else "aisle_task.revise_codex"
        )
    else:
        click("aisle_task.reference")
    revision = done(
        lambda w: not w["busy"] and w["revision"] == 1, "G1 second revision", 180
    )
    if a.provider:
        plan = revision["controller"]
        assert plan["provider"] == a.provider
        assert plan["previous_trial"]["controller_hash"] == result["controller_hash"]
        assert plan["joint"] == result["joint"] and abs(plan["delta_rad"] + 0.06) < 1e-6
        page("run")
        before = revision["selected"]
        click("aisle_run.start_run")
        updated = done(
            lambda w: w["selected"] != before
            and not w["busy"]
            and w["result"].get("task_success"),
            "revised controller measured",
            100,
        )
        checks.append(dict(check="G1 evidence-informed revision", **updated["result"]))
        page("task")
    click("aisle_task.older")
    done(lambda w: w["revision"] == 0, "G1 earlier revision")
    click("aisle_task.newer")
    done(lambda w: w["revision"] == 1, "G1 later revision")
    page("run")
    click("aisle_run.start_run")
    group = wait(lambda j: j["state"]["aisle"]["worker_pid"], "run worker started")[
        "state"
    ]["aisle"]["worker_pid"]
    click("aisle_run.stop_run")
    row = done(
        lambda w: not w["busy"] and w["result"].get("outcome") == "Stopped",
        "operator stop retained",
    )
    assert (Path(row["selected"]) / "stopped.json").is_file()

    def group_is_stopped(_):
        processes = subprocess.check_output(["ps", "-axo", "pgid=,stat="], text=True)
        return not any(
            p and int(p[0]) == group and not p[1].startswith("Z")
            for p in (line.split() for line in processes.splitlines())
        )

    wait(group_is_stopped, "cancelled worker group exited", 5)
    checks.append(dict(check="cancelled worker group exited", process_group=group))
    page("evidence")
    click("aisle_evidence.previous")
    prior = done(lambda w: w["result"].get("task_success"), "reopen scored evidence")
    if not prior["dataset"]:
        click("aisle_evidence.export")
        done(lambda w: w["dataset"] and not w["busy"], "revised episode exported", 60)
    click("aisle_evidence.replay")
    wait(
        lambda j: j["state"]["screen"] == "Replay" and j["state"]["frames"] == 25,
        "episode opened in Studio Replay",
    )
    checks.append(dict(check="native Replay loaded episode", robot="G1", frames=25))
    grab("G1-replay")
    (a.out / "journey.json").write_text(json.dumps(checks, indent=2))


if __name__ == "__main__":
    main()
