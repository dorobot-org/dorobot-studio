"""Time 480 × 360 Nexus head frames and compare the camera pixels on fixed motor inputs."""

import argparse
import hashlib
import io
import json
import sys
import time
from importlib.metadata import version
from pathlib import Path
from statistics import median, mean

import numpy as np


def run(args):
    sys.path.insert(0, str(args.repo / "tools/aisle-validation"))
    import run_rgb_transfer as transfer

    sim = transfer.LivingSimulation(
        transfer.MODEL,
        specs=transfer.scene_recipe(0),
        resolution=(480, 360),
        posture=transfer.POSTURE,
    )
    assert sim.engine.backend_name == "webgpu"
    v = sim.engine.viewer
    if args.setting == "floor":
        floor = next(b for b, s in zip(sim.bodies, sim.specs) if s["name"] == "floor")
        v.set_body_casts_shadows(0, floor.handles[0], False)
    if args.setting in ("2048", "1024"):
        v.set_sensor_shadow_resolution(int(args.setting), 4)
    dof = sim.robot.get_joint("left_j6_gripper").dofs_idx_local
    sim.robot.set_dofs_kp([5.0], dof)
    sim.robot.set_dofs_kv([0.25], dof)
    judge = transfer.Judge(sim, ["red"], record_labels=True)
    targets = np.frombuffer(args.journal.read_bytes(), dtype="<f8").reshape(
        -1, len(sim.names)
    )
    writer = transfer.MotorWriter(
        io.BytesIO(),
        transfer.MotorEnvelope(transfer.MODEL, sim.names, sim.scene.dt),
        targets[0],
    )
    rows, snapshots, state = [], {}, []
    args.out.mkdir()
    for tick in range(1, 321):
        started = time.perf_counter_ns()
        writer.apply(sim.robot, sim.dofs, targets[tick])
        sim.scene.step()
        sim.tick += 1
        if args.setting != "camera-first":
            judge.tick(sim)
        sensor_start = time.perf_counter_ns()
        head = sim.rig["head"].render(rgb=True, depth=False, segmentation=False)[0]
        finished = time.perf_counter_ns()
        if args.setting == "camera-first":
            judge.tick(sim)
        judged = time.perf_counter_ns()
        if tick > 64:
            rows.append(
                [
                    tick,
                    sensor_start - started + judged - finished,
                    finished - sensor_start,
                ]
            )
        if tick % 40 == 0:
            state.append(np.asarray(sim.robot.get_qpos()).copy())
            for name, camera in sim.rig.items():
                rgb = (
                    head
                    if name == "head"
                    else camera.render(rgb=True, depth=False, segmentation=False)[0]
                )
                assert rgb.shape == (360, 480, 3)
                snapshots[f"{tick}-{name}"] = np.asarray(rgb).copy()
    np.savez_compressed(args.out / "rgb.npz", **snapshots)
    np.save(args.out / "encoder.npy", np.stack(state))
    data = dict(
        setting=args.setting,
        nexus=version("dimforge-nexus3d"),
        motor_sha256=hashlib.sha256(args.journal.read_bytes()).hexdigest(),
        shape=[360, 480, 3],
        rows_ns=rows,
        mean_total_ms=mean(a + b for _, a, b in rows) / 1e6,
        median_rgb_ms=median(b for _, a, b in rows) / 1e6,
    )
    (args.out / "timing.json").write_text(json.dumps(data) + "\n")
    print(
        args.setting,
        data["nexus"],
        data["mean_total_ms"],
        data["median_rgb_ms"],
        flush=True,
    )


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    for name in ("repo", "journal", "out"):
        p.add_argument(name, type=Path)
    p.add_argument(
        "setting", choices=("reference", "floor", "2048", "1024", "camera-first")
    )
    run(p.parse_args())
