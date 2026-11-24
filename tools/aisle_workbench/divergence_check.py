"""Check a real pair of trial recordings against Makepad's replay position."""

from bisect import bisect_right
from itertools import zip_longest
import json
from pathlib import Path


def read(path):
    return json.loads(path.read_bytes())


def native_journey(args, page, click, done, wait, grab):
    runs = sorted((args.out.parent / "state/aisle").glob("*-Nano-run"))
    assert len(runs) == 2
    results = [read(p / "result.json") for p in runs]
    actions = [
        [json.loads(line) for line in (p / "actions.jsonl").read_text().splitlines()]
        for p in runs
    ]
    differing = next(
        (
            (i, pair)
            for i, pair in enumerate(zip_longest(*actions), 1)
            if pair[0] != pair[1]
        ),
        None,
    )
    assert differing, "Choose a pair with a measured action divergence"
    index, pair = differing
    page("evidence")
    evidence = done(
        lambda w: w["transfer_comparison"] and not w["busy"], "Paired comparison opened"
    )
    comparison = evidence["transfer_comparison"]
    assert f"First change at action {index} " in comparison["summary"]
    event_ticks = []
    for side, run in enumerate(runs):
        tick = (
            results[side]["physics_ticks"] if pair[side] is None else pair[side]["tick"]
        )
        recording = read(run / "recording.json")
        steps = [sample["step"] for sample in recording["samples"]]
        sample = max(0, bisect_right(steps, tick) - 1)
        frame = comparison["frames"][side]
        assert Path(frame["trial"]) == run
        assert (frame["action_tick"], frame["sample_tick"], frame["index"]) == (
            tick,
            steps[sample],
            sample,
        )
        assert abs(frame["time_s"] - sample / recording["fps"]) < 1e-8
        event_ticks.append(tick)
    grab("paired-evidence" if not args.recover else "restored-pair")
    observation = None
    if any(comparison["packets"]):
        import hashlib
        import numpy as np

        click("aisle_evidence.decision_rgb")
        selected = done(
            lambda w: not w["busy"] and w["decision_observation"],
            "Inspected the policy exposure",
        )
        folder, observation = selected["decision_observation"]
        for side, name in enumerate(("previous", "current")):
            if comparison["packets"][side] is None:
                assert name not in observation["views"]
                continue
            row = observation["views"][name]
            assert row["tick"] == event_ticks[side] and row["trial"] == runs[side].name
            camera_file = runs[side] / row["file"]
            assert (
                hashlib.sha256(camera_file.read_bytes()).hexdigest()
                == row["packet_sha256"]
            )
            with np.load(camera_file, allow_pickle=False) as captured:
                assert int(captured["stamp_ns"]) == row["stamp_ns"]
                rgb = np.concatenate(
                    [captured[c] for c in observation["cameras"]], axis=1
                )
                assert rgb.tobytes() == (Path(folder) / (name + ".rgb")).read_bytes()
        grab("decision-cameras" if not args.recover else "restored-decision-cameras")
        click("aisle_evidence.trial_mode")
        done(
            lambda w: w["decision_observation"] is None, "Returned to the paired trial"
        )
    if not args.recover:
        for side, name in enumerate(("previous", "current")):
            click("aisle_evidence." + name + "_change")

            def has_export(w):
                files = (args.out.parent / "state/aisle").glob("*-export/export.json")
                return not w["busy"] and any(
                    read(file)["trial"] == runs[side].name for file in files
                )

            saved = done(
                has_export, "Exported " + name + " recording from the comparison", 150
            )
            assert Path(saved["selected"]) == runs[1] and (
                side == 1 or not saved["dataset"]
            )
    side = int(args.recover)
    name = ("previous", "current")[side]
    frame = comparison["frames"][side]
    click("aisle_evidence." + name + "_change")
    state = wait(
        lambda j: j["state"]["screen"] == "Replay"
        and j["state"]["replay_frame"] == frame["index"]
        and j["state"]["frames"] == results[side]["frames"],
        "Replay opened at the recorded exposure",
    )["state"]
    assert not state["playing"] and abs(state["time"] - frame["time_s"]) < 1e-8
    grab("replay-" + name)
    result = dict(
        action_number=index,
        policy_observations=observation,
        first_different_actions=pair,
        ticks=event_ticks,
        comparison=comparison,
        trials=[
            dict(
                trial=p.name,
                passed=r["task_success"],
                frames=r["frames"],
                motor_sha256=r["actuator_audit"]["sha256"],
            )
            for p, r in zip(runs, results)
        ],
        replay=dict(
            side=name,
            frame=state["replay_frame"],
            time_s=state["time"],
            frames=state["frames"],
        ),
    )
    (args.out / ("recovery.json" if args.recover else "journey.json")).write_text(
        json.dumps(result, indent=2) + "\n"
    )
