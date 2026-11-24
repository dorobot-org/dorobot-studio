"""Verify a native, evidence-conditioned speed revision on the reference Linux failure."""

from pathlib import Path
import sys

from revision_check import digest, read, save


def native_journey(args, page, click, remote, done, grab):
    root = args.out.parent / "state/aisle"
    parent = root / "0001-Nano-run"
    previous = read(parent / "result.json")
    record = args.out / "journey.json"
    if args.recover:
        saved = read(record)
        page("evidence")
        state = done(
            lambda w: (w["result"] or {}).get("controller_hash")
            == saved["controller_sha256"],
            "Reopened the speed revision",
        )
        assert state["result"]["task_success"] == saved["task_success"]
        assert state["controller"]["max_forward_speed_m_s"] == saved["speed_m_s"]
        save(
            args.out / "recovery.json",
            dict(
                controller_sha256=saved["controller_sha256"],
                trial=Path(state["selected"]).name,
            ),
        )
        return

    if previous["seed"] != 0 or previous["requested"] != ["red"]:
        raise ValueError("Select the red Linux reference transfer on layout 0")
    page("evidence")
    done(
        lambda w: w["selected"] == str(parent) and w["result"]["task_success"] is False,
        "Opened the scored loss of wheel support",
    )
    grab("support-intervals")
    page("task")
    click("aisle_task.instruction")
    remote(
        "key",
        "w=0",
        "c=KeyA",
        "logo=1" if sys.platform == "darwin" else "ctrl=1",
        "wait=1",
    )
    task = "Revise the red floor cube to coffee table transfer after the recorded wheel-support loss. Limit forward speed to 0.080 m/s and preserve the original physical acceptance criteria."
    remote("text", "w=0", "t=" + task, "wait=1")
    click(
        "aisle_task.revise_codex" if args.provider == "codex" else "aisle_task.revise"
    )

    def prepared(w):
        for error in root.glob("*-Nano-prepare/error.json"):
            raise RuntimeError(read(error)["message"])
        return (
            not w["busy"] and (w["controller"] or {}).get("provider") == args.provider
        )

    row = done(prepared, "Received the requested forward limit from the provider", 180)
    (controller,) = root.glob("*-Nano-prepare/controller.json")
    plan = read(controller)
    assert (
        plan["task"] == task
        and plan["color_order"] == "red"
        and plan["max_forward_speed_m_s"]
        == row["controller"]["max_forward_speed_m_s"]
        == 0.08
    )
    feedback = read(controller.with_name("feedback.json"))
    assert (
        feedback["controller_hash"]
        == previous["controller_hash"]
        == digest(parent / "controller.json")
    )
    evidence = feedback["wheel_support"]
    assert (
        evidence["unsupported_ticks"]
        == previous["physics_ticks"] - previous["wheel_supported_ticks"]
    )
    assert [e["first_tick"] for e in evidence["first_intervals"]] == [
        e["first_step"] for e in previous["unsupported_wheel_ranges"][:8]
    ]
    assert read(controller.with_name("reply.json"))["max_forward_speed_m_s"] == 0.08
    grab("drive-revision")
    page("run")
    click("aisle_run.start_run")
    row = done(
        lambda w: not w["busy"]
        and (w["result"] or {}).get("controller_hash") == digest(controller),
        "Scored the revised Nano program",
        600,
    )
    result = row["result"]
    trial = Path(row["selected"])
    assert read(trial / "calibration.json")["max_forward_speed_m_s"] == 0.08
    assert (
        result["requested"] == ["red"]
        and result["seed"] == previous["seed"]
        and result["actuator_audit"]["passed"]
    )
    page("evidence")
    grab("revised-result")
    save(
        record,
        dict(
            provider=args.provider,
            model=plan["policy_model"],
            parent_sha256=feedback["controller_hash"],
            controller_sha256=digest(controller),
            trial=trial.name,
            speed_m_s=plan["max_forward_speed_m_s"],
            task_success=result["task_success"],
            ticks=result["physics_ticks"],
            supported_ticks=result["wheel_supported_ticks"],
            motor_sha256=result["actuator_audit"]["sha256"],
        ),
    )
