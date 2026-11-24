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

    def received_frame(w, after_tick):
        row = w.get("worker_observation") or {}
        job = row.get("job") or {}
        if (row.get("frame") or {}).get("tick", 0) > after_tick:
            return True
        if job.get("state") in ("failed", "cancelled", "completed"):
            reason = (
                (job.get("result") or {}).get("error")
                or job.get("message")
                or job["state"]
            )
            raise RuntimeError(
                f"Foreground job ended before the requested RGB frame: {reason}"
            )
        return False

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
        assert len(row["fleet"]["jobs"]) == (
            1 if args.foreground_cadence or args.fleet_controller else 2
        )
        if args.background_clock == "wall" and not args.foreground_cadence:
            run_button("newer_job")
            done(
                lambda w: selected(w).get("id") == proof["background_id"]
                and selected(w).get("observation_clock") == "wall",
                "Recovered wall-clock setting for the submitted observer",
            )
            run_button("older_job")
            done(
                lambda w: selected(w).get("id") == proof["foreground_id"],
                "Restored foreground selection",
            )
        page("evidence")
        row = done(
            lambda w: (w["result"] or {}).get("controller_hash")
            == proof["result"]["controller_hash"]
            and w["result"]["task_success"] == proof["result"]["task_success"]
            and w["dataset"],
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

    if args.resume_fleet:
        # The disconnected Studio already retained its background observation and cancellation.
        root = args.out.parent / "state/aisle"
        fleet = json.loads((root / "worker-selection.json").read_bytes())
        assert (
            fleet["node"] == args.fleet_node and fleet["selected"]["mode"] == "stream"
        )
        foreground = fleet["selected"]["id"]
        (background,) = (j for j in fleet["jobs"] if j != foreground)
        candidates = [
            json.loads(p.read_bytes()) for p in root.glob("*-Nano-fleet/fleet.json")
        ]
        snapshot = next(
            r["sample"]
            for r in candidates
            if r.get("sample") and r["selected"]["id"] == background
        )
        stopped = next(
            p.parent
            for p in root.glob("*-Nano-run/worker-origin.json")
            if json.loads(p.read_bytes())["job"] == background
        )
        assert (stopped / "stopped.json").is_file()
        observations = [
            json.loads(l)
            for l in (stopped / "background-observations.jsonl")
            .read_bytes()
            .splitlines()
        ]
        assert any(
            o["sha256"] == snapshot["sha256"] and o["tick"] == snapshot["tick"]
            for o in observations
        )
        enter_workers()
        row = done(
            lambda w: selected(w).get("id") == foreground
            and observed(w).get("state") == "completed",
            "Reconnected to the completed SSH foreground",
            100,
        )
        result = observed(row)["result"]
        origin, _ = import_and_replay(foreground, result)
        checks.append(
            dict(
                check="Reconnected and fetched the remote trial",
                foreground_id=foreground,
                background_id=background,
                observation_ticks=[o["tick"] for o in observations],
                snapshot=snapshot,
                origin=origin,
                result=result,
            )
        )
        source.write_text(json.dumps(checks, indent=2))
        return

    page("task")
    if not args.fleet_controller:
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

    else:
        ALIAS = args.fleet_node or "local"
    if ALIAS != "local":
        run_button("node")
        remote(
            "key",
            "w=0",
            "c=KeyA",
            "logo=1" if sys.platform == "darwin" else "ctrl=1",
            "wait=1",
        )
        remote("text", "w=0", "t=" + ALIAS, "wait=1")
    run_button("connect")
    row = done(
        lambda w: not w["busy"] and (w["fleet"] or {}).get("node") == ALIAS,
        "Connected to Nexus worker",
    )
    slots = row["fleet"]["capabilities"]["slots"]
    if args.fleet_node:
        assert slots >= (1 if args.fleet_controller else 2)
    else:
        assert slots == (args.foreground_cadence or 2)
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
        cadence = verify_viewport(args.out, trial, backgrounds, args.background_clock)
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
        lambda w: received_frame(w, 0),
        "Received measured RGB frame",
        180,
    )
    tick = frame["worker_observation"]["frame"]["tick"]
    done(
        lambda w: received_frame(w, tick),
        "Received a later frame",
        180,
    )
    grab("foreground")
    if args.fleet_controller:
        (prepared,) = (args.out.parent / "state/aisle").glob(
            "*-Nano-prepare/controller.json"
        )
        expected = hashlib.sha256(prepared.read_bytes()).hexdigest()
        row = done(
            lambda w: observed(w).get("id") == foreground
            and observed(w).get("state") == "completed",
            "Registered worker scored the original controller",
            650,
        )
        result = observed(row)["result"]
        assert result["controller_hash"] == expected and isinstance(
            result["task_success"], bool
        )
        origin, trial = import_and_replay(foreground, result)
        checks.append(
            dict(
                check="Submitted the frozen controller, streamed and recovered its original recording",
                foreground_id=foreground,
                origin=origin,
                trial=trial.name,
                result=result,
            )
        )
        source.write_text(json.dumps(checks, indent=2))
        return
    run_button("queue_stream")
    done(
        lambda w: not w["busy"] and "foreground stream" in w["status"],
        "Studio rejected a second foreground",
    )
    if args.background_clock == "wall":
        run_button("clock")
        done(
            lambda w: w["observation_clock"] == "wall"
            and w["background_period_s"] == 1,
            "Selected one RGB observation per wall-clock second",
        )
    else:
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
        and (w["fleet"].get("sample") or {}).get("tick", 0)
        >= (1 if args.background_clock == "wall" else 600),
        "Retrieved one background head-camera snapshot",
    )
    snapshot = sample["fleet"]["sample"]
    assert (
        selected(sample).get("observation_clock", "simulation") == args.background_clock
    )
    if args.background_clock == "wall":
        assert selected(sample)["observation_period_s"] == 1
    else:
        assert (
            selected(sample)["observation_period_s"] == 2
            and snapshot["tick"] % 200 == 0
        )
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
    if args.background_clock == "wall":
        history = json.loads((path / "source.json").read_bytes())
        assert (
            history["observation_clock"] == "wall"
            and history["observation_period_s"] == 1
        )
        assert len(observations) >= 2 and any(r["tick"] % 100 for r in observations)
        assert all(
            a["tick"] < b["tick"]
            and a["published_monotonic_ns"] < b["published_monotonic_ns"]
            for a, b in zip(observations, observations[1:])
        )
    else:
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
    if not args.fleet_node:
        assert (
            result["task_success"]
            and result["wheel_supported_ticks"] == result["physics_ticks"]
        )
    else:
        assert isinstance(result["task_success"], bool) and result["physics_ticks"] > 0
    origin, _ = import_and_replay(foreground, result)
    checks.append(
        dict(
            check="Two leased simulations through native Studio",
            foreground_id=foreground,
            background_id=background,
            background_step=step,
            observation_clock=args.background_clock,
            observation_ticks=[o["tick"] for o in observations],
            snapshot=snapshot,
            origin=origin,
            result=result,
        )
    )
    source.write_text(json.dumps(checks, indent=2))
