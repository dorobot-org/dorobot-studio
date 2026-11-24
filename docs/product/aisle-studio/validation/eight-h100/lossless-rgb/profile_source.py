"""Replay a segment of the measured motor journal and time blocking calls on a single H100."""

import argparse
import hashlib
import io
import json
from pathlib import Path
import statistics
import sys
import time


def measure(repo, journal):
    import numpy as np

    sys.path.insert(0, str(repo / "tools/aisle-validation"))
    import run_rgb_transfer as transfer

    sim = transfer.LivingSimulation(
        transfer.MODEL,
        specs=transfer.scene_recipe(0),
        resolution=(480, 360),
        posture=transfer.POSTURE,
    )
    dof = sim.robot.get_joint("left_j6_gripper").dofs_idx_local
    sim.robot.set_dofs_kp([5.0], dof)
    sim.robot.set_dofs_kv([0.25], dof)
    judge = transfer.Judge(sim, ["red"], record_labels=True)
    env = transfer.MotorEnvelope(transfer.MODEL, sim.names, sim.scene.dt)
    raw = journal.read_bytes()
    targets = np.frombuffer(raw, dtype="<f8").reshape(-1, len(sim.names))
    writer = transfer.MotorWriter(io.BytesIO(), env, targets[0])
    state = sim.scene.state
    rows = []

    class TimedState:
        def __getattr__(self, method):
            function = getattr(state, method)
            if method not in (
                "debug_rigid_contact_impulses",
                "debug_multibody_contact_impulses",
                "debug_contacts",
                "robot_state",
                "read_body_poses",
            ):
                return function

            def timed(*args, **kwargs):
                t = time.perf_counter_ns()
                result = function(*args, **kwargs)
                subtotal[method] = subtotal.get(method, 0) + time.perf_counter_ns() - t
                return result

            return timed

    proxy = TimedState()
    for tick in range(1, 321):
        subtotal = {}
        t = time.perf_counter_ns()
        writer.apply(sim.robot, sim.dofs, targets[tick])
        t1 = time.perf_counter_ns()
        sim.scene.step()
        sim.tick += 1
        t2 = time.perf_counter_ns()
        sim.scene.state = proxy
        try:
            judge.tick(sim)
        finally:
            sim.scene.state = state
        t3 = time.perf_counter_ns()
        sim.scene.sync()
        t4 = time.perf_counter_ns()
        rgb = sim.rig["head"].render(rgb=True, depth=False, segmentation=False)[0]
        t5 = time.perf_counter_ns()
        assert rgb.shape == (360, 480, 3)
        if tick > 64:
            rows.append(
                dict(
                    tick=tick,
                    write=t1 - t,
                    physics=t2 - t1,
                    score=t3 - t2,
                    sync=t4 - t3,
                    head=t5 - t4,
                    total=t5 - t,
                    **subtotal,
                )
            )
    med = {
        k: statistics.median(r[k] for r in rows) / 1e6 for k in rows[0] if k != "tick"
    }
    return dict(
        motor_sha256=hashlib.sha256(raw).hexdigest(),
        backend=sim.engine.backend_name,
        warmup_ticks=64,
        measured_ticks=len(rows),
        median_ms=med,
        mean_total_ms=statistics.mean(r["total"] for r in rows) / 1e6,
        raw_ns=rows,
    )


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("repo", type=Path)
    p.add_argument("journal", type=Path)
    p.add_argument("output", type=Path)
    a = p.parse_args()
    a.output.write_text(json.dumps(measure(a.repo, a.journal), indent=2) + "\n")
