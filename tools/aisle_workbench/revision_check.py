"""Drive the measured Nano pickup revision from Task through Evidence and recovery."""

import hashlib
import json
from pathlib import Path
import sys


def read(path):
    return json.loads(path.read_bytes())


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def save(path, data):
    path.write_text(json.dumps(data, indent=2) + "\n")


def native_journey(args, page, click, remote, done, grab, wait):
    from controller_review_check import inspect

    trials = args.out.parent / "state/aisle"
    parent = trials / "0001-Nano-run"
    previous = read(parent / "result.json")
    if args.recover:
        page("evidence")
        receipt = read(args.out / "journey.json")
        row = done(
            lambda w: w["selected"]
            and Path(w["selected"]).name == receipt["trial"]
            and w["result"],
            "Reopened revised evidence",
        )
        assert row["result"]["controller_hash"] == receipt["controller_sha256"]
        assert (
            row["controller"]["pickup_revision"]
            == read(args.out / "author.json")["revision"]
        )
        save(
            args.out / "recovery.json",
            dict(
                trial=receipt["trial"],
                controller_sha256=row["result"]["controller_hash"],
            ),
        )
        return

    protocol = read(
        Path(__file__).resolve().parents[2]
        / "docs/product/aisle-studio/validation/observed-revision/protocol.json"
    )

    def complete(w, test):
        for error in trials.glob("*/error.json"):
            if error.parent != parent:
                raise RuntimeError(read(error)["message"])
        return not w["busy"] and test(w)

    page("task")
    done(lambda w: w["selected"] == str(parent), "Selected recorded failed pickup")
    if not args.prepared_pickup:
        click("aisle_task.instruction")
        remote(
            "key",
            "w=0",
            "c=KeyA",
            "logo=1" if sys.platform == "darwin" else "ctrl=1",
            "wait=1",
        )
        remote("text", "w=0", "t=" + protocol["task"], "wait=1")
        click(
            "aisle_task.revise_codex"
            if args.provider == "codex"
            else "aisle_task.revise"
        )
    row = done(
        lambda w: complete(
            w, lambda s: (s["controller"] or {}).get("provider") == args.provider
        ),
        "Author returned revised controller",
        190,
    )
    (prepared,) = trials.glob("*-Nano-prepare/controller.json")
    plan = read(prepared)
    assert plan["task"] == protocol["task"]
    assert all(
        plan[key] == row["controller"][key]
        for key in (
            "task",
            "provider",
            "policy_model",
            "pickup_revision",
            "color_order",
            "track_grasp",
        )
    )
    assert (
        plan["kind"] == "table-transfer"
        and plan["track_grasp"]
        and plan["color_order"].split(",") == protocol["order"]
    )
    revision = plan["pickup_revision"]
    assert (
        previous["controller_hash"]
        == digest(parent / "controller.json")
        == revision["parent_controller_sha256"]
    )
    assert (
        revision["packet_sha256"]
        == digest(parent / "last-pickup-observation.npz")
        == digest(prepared.with_name("revision-observation.npz"))
    )
    inspect(
        args,
        click,
        wait,
        grab,
        "nano-pickup-changes",
        "parent",
        previous["controller_hash"],
        digest(prepared),
        f"Pickup association · {revision['previous_association']} → {revision['grasp_association']}",
    )
    metrics = read(prepared.with_name("provider-metrics.json"))
    assert (
        metrics["provider"] == args.provider
        and metrics["model"] == plan["policy_model"]
    )
    save(
        args.out / "author.json",
        dict(
            provider=args.provider,
            model=metrics["model"],
            revision=revision,
            controller_sha256=digest(prepared),
            reply_sha256=digest(prepared.with_name("reply.json")),
        ),
    )
    grab("revised-controller")
    page("scene")
    click("aisle_scene.slide_neg")
    done(
        lambda w: w["target_motion"] == protocol["fixture"]
        and w["layout_seed"] == protocol["development_seed"],
        "Restored the development fixture",
    )
    page("run")
    click("aisle_run.start_run")
    row = done(
        lambda w: complete(
            w,
            lambda s: s["selected"] != str(parent)
            and (s["result"] or {}).get("controller_hash") == digest(prepared),
        ),
        "Scored revised controller",
        650,
    )
    trial = Path(row["selected"])
    result = read(trial / "result.json")
    assert result["seed"] == protocol["development_seed"]
    assert all(
        result[key] == row["result"][key]
        for key in (
            "controller_hash",
            "task_success",
            "physics_ticks",
            "actuator_audit",
        )
    )
    assert (
        read(trial / "calibration.json")["grasp_rgb_association"]
        == revision["grasp_association"]
    )
    assert digest(trial / "revision-observation.npz") == revision["packet_sha256"]
    page("evidence")
    grab("revised-evidence")
    save(
        args.out / "journey.json",
        dict(
            trial=trial.name,
            controller_sha256=digest(prepared),
            task_success=result["task_success"],
            moving_grasp={
                c: result["moving_grasp"]["cubes"].get(c, {}).get("passed", False)
                for c in result["requested"]
            },
            cubes={c: r["passed"] for c, r in result["cubes"].items()},
        ),
    )
