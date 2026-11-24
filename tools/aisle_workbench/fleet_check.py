"""Exercise two overlapping Nexus jobs via native Run controls, then reopen the trial."""

import hashlib
import json
from pathlib import Path
import sys


def native_journey(args, checks, page, click, remote, done, grab, wait):
    source = args.out / "journey.json"

    def run_button(key):
        click("aisle_run." + key)

    def selected(w):
        return (w.get("fleet") or {}).get("selected") or {}

    def observed(w):
        return (w.get("worker_observation") or {}).get("job") or {}

    def enter_workers():
        page("run")
        run_button("workers")
        done(lambda w: w["workers_open"] and not w["busy"], "Workers page opened")

    def import_and_replay(foreground, result):
        run_button("fetch_job")
        row = done(
            lambda w: not w["busy"]
            and (w["result"] or {}).get("controller_hash") == result["controller_hash"]
            and w["selected"],
            "Downloaded and selected the worker trial",
            180,
        )
        trial = Path(row["selected"])
        origin = json.loads((trial / "worker-origin.json").read_bytes())
        assert (
            origin["job"] == foreground
            and hashlib.sha256((trial / "controller.json").read_bytes()).hexdigest()
            == result["controller_hash"]
        )
        page("evidence")
        grab("imported-evidence")
        click("aisle_evidence.export")
        row = done(
            lambda w: not w["busy"] and w["dataset"], "Exported downloaded episode", 180
        )
        dataset = Path(row["dataset"])
        assert (dataset / "meta/info.json").is_file()
        click("aisle_evidence.replay")
        wait(
            lambda j: j["state"]["screen"] == "Replay"
            and j["state"]["frames"] == result["frames"],
            "Opened the complete imported episode in Replay",
        )
        grab("imported-replay")
        return origin, trial

    if args.recover:
        proof = json.loads(source.read_bytes())[-1]
        enter_workers()
        row = done(
            lambda w: selected(w).get("id") == proof["foreground_id"]
            and selected(w)["state"] == "completed",
            "Worker selection recovered",
        )
        assert len(row["fleet"]["jobs"]) == (1 if args.foreground_cadence else 2)
        page("evidence")
        row = done(
            lambda w: (w["result"] or {}).get("task_success") and w["dataset"],
            "Imported worker trial and export recovered",
        )
        click("aisle_evidence.replay")
        wait(
            lambda j: j["state"]["screen"] == "Replay"
            and j["state"]["frames"] == proof["result"]["frames"],
            "Reopened worker episode in Replay",
        )
        (args.out / "recovery.json").write_text(json.dumps(checks, indent=2))
        return

    page("task")
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
        "t=Move the red floor cube onto the coffee table.",
        "wait=1",
    )
    click("aisle_task.codex" if args.provider == "codex" else "aisle_task.octos")
    row = done(
        lambda w: not w["busy"]
        and (w["controller"] or {}).get("kind") == "table-transfer",
        "Author prepared a Nano transfer",
        180,
    )
    assert row["controller"]["color_order"] == "red"
    enter_workers()
    if args.foreground_cadence:
        from fleet_cadence import ALIAS, while_streaming, verify_viewport

        run_button("node")
        remote(
            "key",
            "w=0",
            "c=KeyA",
            "logo=1" if sys.platform == "darwin" else "ctrl=1",
            "wait=1",
        )
        remote("text", "w=0", "t=" + ALIAS, "wait=1")
    else:
        ALIAS = "local"
    run_button("connect")
    row = done(
        lambda w: not w["busy"] and (w["fleet"] or {}).get("node") == ALIAS,
        "Connected to Nexus worker",
    )
    assert row["fleet"]["capabilities"]["slots"] == (args.foreground_cadence or 2)
    run_button("queue_stream")
    row = done(
        lambda w: not w["busy"] and selected(w).get("mode") == "stream",
        "Submitted foreground",
    )
    foreground = selected(row)["id"]
    if args.foreground_cadence:
        done(
            lambda w: ((w.get("worker_observation") or {}).get("frame") or {}).get(
                "tick", 0
            )
            > 0,
            "Foreground stream delivered measured RGB",
            120,
        )
        result, backgrounds = while_streaming(args, foreground, done, grab)
        origin, trial = import_and_replay(foreground, result)
        cadence = verify_viewport(args.out, trial, backgrounds)
        checks.append(
            dict(
                check=f"{args.foreground_cadence} simultaneous simulations with measured viewport cadence",
                foreground_id=foreground,
                origin=origin,
                result=result,
                cadence=cadence,
            )
        )
        source.write_text(json.dumps(checks, indent=2))
        return
    frame = done(
        lambda w: ((w.get("worker_observation") or {}).get("frame") or {}).get(
            "tick", 0
        )
        > 0,
        "Received measured RGB frame",
        180,
    )
    tick = frame["worker_observation"]["frame"]["tick"]
    done(
        lambda w: ((w.get("worker_observation") or {}).get("frame") or {}).get(
            "tick", 0
        )
        > tick,
        "Received a later frame",
        180,
    )
    grab("foreground")
    run_button("queue_stream")
    done(
        lambda w: not w["busy"] and "foreground stream" in w["status"],
        "Studio rejected a second foreground",
    )
    run_button("period")
    done(
        lambda w: w["background_period_s"] == 2,
        "Selected one RGB observation every two simulated seconds",
    )
    run_button("queue_background")
    row = done(
        lambda w: not w["busy"] and selected(w).get("mode") == "observations",
        "Submitted a background run",
    )
    background = selected(row)["id"]
    assert background != foreground and len(row["fleet"]["capabilities"]["active"]) == 2
    row = done(
        lambda w: (observed(w).get("progress") or {}).get("step", 0) >= 600
        and observed(w).get("id") == background,
        "Background simulation advanced while foreground ran",
        180,
    )
    step = observed(row)["progress"]["step"]
    run_button("sample_job")
    sample = done(
        lambda w: not w["busy"]
        and (w["fleet"].get("sample") or {}).get("tick", 0) >= 600,
        "Retrieved one background head-camera snapshot",
    )
    snapshot = sample["fleet"]["sample"]
    assert selected(sample)["observation_period_s"] == 2 and snapshot["tick"] % 200 == 0
    grab("background-snapshot")
    grab("overlap")
    run_button("cancel_job")
    done(
        lambda w: observed(w).get("state") == "cancelled"
        and observed(w).get("id") == background,
        "Cancelled only the background job",
        90,
    )
    run_button("fetch_job")
    done(
        lambda w: not w["busy"] and (w["result"] or {}).get("outcome") == "Stopped",
        "Imported the cancelled job receipt",
    )
    cancelled = done(
        lambda w: (w["result"] or {}).get("outcome") == "Stopped",
        "Stopped episode reopened",
    )
    path = Path(cancelled["selected"])
    observations = [
        json.loads(line)
        for line in (path / "background-observations.jsonl").read_text().splitlines()
    ]
    assert [r["tick"] for r in observations] == list(
        range(200, observations[-1]["tick"] + 1, 200)
    )
    assert any(
        r["tick"] == snapshot["tick"] and r["sha256"] == snapshot["sha256"]
        for r in observations
    )
    assert not (path / "recording.json").exists()
    run_button("older_job")
    row = done(
        lambda w: not w["busy"]
        and selected(w).get("id") == foreground
        and observed(w).get("state") == "completed",
        "Foreground completed independently",
        600,
    )
    result = observed(row)["result"]
    assert (
        result["task_success"]
        and result["wheel_supported_ticks"] == result["physics_ticks"]
    )
    origin, _ = import_and_replay(foreground, result)
    checks.append(
        dict(
            check="Two leased simulations through native Studio",
            foreground_id=foreground,
            background_id=background,
            background_step=step,
            observation_ticks=[o["tick"] for o in observations],
            snapshot=snapshot,
            origin=origin,
            result=result,
        )
    )
    source.write_text(json.dumps(checks, indent=2))
