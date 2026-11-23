"""Drive a running Studio via Makepad pointer events and verify retained AISLE results."""

import argparse
import json
import sys
from pathlib import Path
import subprocess
import time


def main():
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
    p.add_argument("--standing", action="store_true")
    p.add_argument("--transfer", choices=["red", "three"])
    p.add_argument("--layout", type=int, choices=[0, 1, 2], default=0)
    p.add_argument("--impact", action="store_true")
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

    def wait(test, label, seconds=30):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            j = report()
            if test(j):
                return j
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

    def done(test, label, seconds=30):
        row = wait(lambda j: test(j["state"]["aisle"]), label, seconds)["state"][
            "aisle"
        ]
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
            assert row["result"]["task_success"] and (
                a.standing or row["result"]["requested"] == wanted
            )
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
        if a.standing:
            request = "Stand on both feet and track a right shoulder displacement of minus 0.08 rad for the standing test."
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
        grab(label + "-task")
        page("scene")
        if a.impact:
            click("aisle_scene.impact")
            done(lambda w: w["impact"], "G1 impact selected")
        if a.transfer:
            for seed in [(a.layout + 1) % 3, a.layout]:
                if report()["state"]["aisle"]["layout_seed"] == seed:
                    continue
                click("aisle_scene.layout" + str(seed))
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
            600,
        )
        result = row["result"]
        page("evidence")
        grab(label + "-evidence")
        checks.append(dict(check=label + " physical verdict", **result))
        (a.out / "journey.json").write_text(json.dumps(checks, indent=2))
        assert result["task_success"], result
        if a.standing:
            assert result["both_feet_loaded_steps"] == result["steps"] == 2000
            assert abs(result["measured_delta"] + 0.08) < 0.012
            if a.impact:
                assert (
                    result["disturbance"]["passed"]
                    and result["disturbance"]["contacts"] > 0
                )
                assert result["estimated_positive_joint_work_j"] > 0
        else:
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
        click("aisle_task.revise")
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
