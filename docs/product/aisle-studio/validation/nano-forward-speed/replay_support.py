"""Re-execute a stationary Nano motor journal and measure wheel-floor load at every tick."""

import argparse
import hashlib
import json
from pathlib import Path
import sys

import numpy as np

ROOT = Path(__file__).resolve().parents[5]
sys.path.insert(0, str(ROOT / "tools/aisle-validation"))
from nano_actuator_audit import MotorEnvelope, replay
from nano_living import LivingSimulation
from run_rgb_transfer import MODEL, POSTURE, scene_recipe


def run(trial, out):
    result = json.loads((trial / "result.json").read_bytes())
    source = json.loads((trial / "source.json").read_bytes())
    if (
        result.get("kind") != "table-transfer"
        or result.get("room_revision_sha256")
        or result.get("moving_grasp")
    ):
        raise ValueError(
            "Select a stationary Nano transfer in the reference living room"
        )
    if hashlib.sha256(MODEL.read_bytes()).hexdigest() != result["model_hash"]:
        raise ValueError("Nano URDF has changed")
    sim = LivingSimulation(
        MODEL,
        specs=scene_recipe(result["seed"]),
        resolution=(480, 360),
        posture=POSTURE,
    )
    if sim.engine.backend_name != source["backend"]:
        raise ValueError("Replay this physics journal on its recorded Nexus backend")
    grip = sim.robot.get_joint("left_j6_gripper").dofs_idx_local
    sim.robot.set_dofs_kp([5.0], grip)
    sim.robot.set_dofs_kv([0.25], grip)
    journal = trial / "motor-targets.f64"
    assert (
        replay(
            journal,
            MotorEnvelope(MODEL, sim.names, sim.scene.dt),
            result["physics_ticks"],
        )
        == result["actuator_audit"]
    )
    targets = np.fromfile(journal, dtype="<f8").reshape(-1, len(sim.names))
    np.testing.assert_allclose(targets[0], sim.targets, rtol=0, atol=1e-12)
    floor = next(b for spec, b in zip(sim.specs, sim.bodies) if spec["name"] == "floor")
    floor_id = sim.scene.gpu_index(0, floor.handles[0])
    wheel_names = ("wheel_1", "wheel_2", "wheel_3")
    wheels = {
        link.index: wheel_names.index(link.name)
        for link in sim.robot.links
        if link.name in wheel_names
    }
    assert len(wheels) == 3
    peaks = np.zeros((len(targets) - 1, 3), dtype="<f8")
    for tick, row in enumerate(targets[1:]):
        sim.robot.control_dofs_position(row.tolist(), sim.dofs)
        sim.scene.step()
        for contact in sim.scene.state.debug_multibody_contact_impulses(
            sim.engine.viewer
        ):
            which = wheels.get(contact["link"])
            if (
                which is not None
                and contact["free_body"] == floor_id
                and contact["kind"] == "normal"
            ):
                peaks[tick, which] = max(peaks[tick, which], contact["impulse"])
    unsupported = (1 + np.flatnonzero(peaks.max(axis=1) <= 1e-6)).tolist()
    expected = [
        tick
        for gap in result["unsupported_wheel_ranges"]
        for tick in range(gap["first_step"], gap["last_step"] + 1)
    ]
    out.mkdir(parents=True, exist_ok=False)
    np.savez_compressed(out / "wheel-impulses.npz", peak_normal_impulse_ns=peaks)
    report = dict(
        backend=source["backend"],
        nexus=source["nexus"],
        seed=result["seed"],
        ticks=len(peaks),
        wheel_order=wheel_names,
        quantity="maximum individual wheel-floor normal contact impulse per wheel and physics tick",
        unit="N s",
        threshold_ns=1e-6,
        minimum_supporting_impulse_ns=float(peaks.max(axis=1).min()),
        unsupported_ticks=unsupported,
        scored_unsupported_ticks=expected,
        agrees=expected == unsupported,
        motor_sha256=result["actuator_audit"]["sha256"],
        controller_sha256=result["controller_hash"],
        impulses_sha256=hashlib.sha256(
            (out / "wheel-impulses.npz").read_bytes()
        ).hexdigest(),
    )
    (out / "replay.json").write_text(json.dumps(report, indent=2) + "\n")
    if not report["agrees"]:
        raise ValueError(
            "Replayed contact intervals differ from the scored trial; see replay.json"
        )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("trial", type=Path)
    parser.add_argument("out", type=Path)
    args = parser.parse_args()
    run(args.trial, args.out)
