"""Bind a measured-device dry run to an existing AISLE simulation trial."""

import json
import os
from pathlib import Path
import secrets
import socket
import subprocess
import sys
import xml.etree.ElementTree as ET

from worker import ROOT, describe, digest, validate

HERE = Path(__file__).parent


def read(path):
    return json.loads(path.read_bytes())


def identity(trial, robot):
    plan, result, source = (
        read(trial / n) for n in ("controller.json", "result.json", "source.json")
    )
    meta = describe(robot)
    sha = digest(trial / "controller.json")
    if not (
        result.get("task_success") is True
        and plan["robot"] == result["robot"] == robot
        and plan["model_hash"] == result["model_hash"] == meta["model_hash"]
        and result["controller_hash"] == source["controller_sha256"] == sha
    ):
        raise ValueError(
            "Select a successful simulation trial with its original controller and model"
        )
    control = validate(plan, meta)
    if result["kind"] != control.get("kind", "joint-inspection"):
        raise ValueError("Controller and simulation task kinds differ")
    return plan, result, source, meta


def replay_samples(path, plan, model, weights=None):
    import numpy as np
    from g1_dds_state import JOINTS
    from g1_stand import StandingController

    rows = [json.loads(line) for line in path.read_text().splitlines()]
    if not 2 <= len(rows) <= 25000:
        raise ValueError("Incomplete LowState stream")
    times = np.asarray([r["source_ns"] for r in rows], dtype=np.int64)
    receive = np.asarray([r["received_ns"] for r in rows], dtype=np.int64)
    ticks = np.asarray([r["tick"] for r in rows], dtype=np.int64)
    gap = np.diff(times)
    arrival = np.diff(receive)
    sequence = np.diff(ticks) % (2**32)
    if (
        len({r["publisher"] for r in rows}) != 1
        or len({r["mode_machine"] for r in rows}) != 1
        or any(r["mode_pr"] != 0 for r in rows)
    ):
        raise ValueError(
            "G1 publisher or machine mode changed, or joint feedback is outside pitch/roll mode"
        )
    if np.any(sequence == 0) or np.any(sequence >= 2**31):
        raise ValueError("G1 board tick repeated or moved backwards")
    if (
        np.any(gap <= 0)
        or gap.max() > 10_000_000
        or np.any(arrival < 0)
        or arrival.max() > 250_000_000
    ):
        raise ValueError(
            "G1 telemetry is reordered or has a gap in the 200 Hz observation window"
        )
    clock_skew_ns = abs((receive[-1] - receive[0]) - (times[-1] - times[0]))
    if clock_skew_ns > 250_000_000:
        raise ValueError(
            "G1 source and receive intervals disagree over the observation window"
        )
    times = times - times[0]
    grid = np.arange(2200, dtype=np.int64) * 5_000_000
    if times[-1] < grid[-1]:
        raise ValueError("G1 telemetry ended before the standing program finished")
    indices = np.searchsorted(times, grid, side="right") - 1
    q, dq, quat = (
        np.asarray([r[k] for r in rows], dtype=np.float32) for k in ("q", "dq", "quat")
    )
    if (
        q.shape != (len(rows), 29)
        or dq.shape != q.shape
        or quat.shape != (len(rows), 4)
        or not all(np.isfinite(a).all() for a in (q, dq, quat))
    ):
        raise ValueError("Invalid G1 IMU or motor measurement")
    if np.max(np.abs(np.linalg.norm(quat, axis=1) - 1.0)) > 0.01:
        raise ValueError("G1 IMU quaternion is not normalized")
    tree = ET.parse(model)
    limits = [tree.find(f"joint[@name='{n}']/limit") for n in JOINTS]
    if any(e is None for e in limits):
        raise ValueError("Unitree G1 29-motor mapping differs from the selected URDF")
    low, high, speed = np.asarray(
        [[float(e.get(k)) for k in ("lower", "upper", "velocity")] for e in limits]
    ).T
    excursion = float(np.maximum(low - q, q - high).max(initial=0))
    if excursion > 0.005:
        raise ValueError(
            f"Measured G1 joint position exceeds the URDF by {excursion:.4f} rad"
        )
    speed_excess = float(np.maximum(np.abs(dq) - speed, 0).max())
    if speed_excess > 1e-5:
        raise ValueError(
            f"Measured G1 joint speed exceeds the URDF by {speed_excess:.4f} rad/s"
        )
    control = StandingController(
        plan, list(JOINTS), low, high, speed, 0.005, 200, weights
    )
    targets = np.asarray([control.advance(q[i], quat[i]).copy() for i in indices])
    rates = np.diff(targets, axis=0, prepend=np.zeros((1, 29))) / 0.005
    if (
        not np.isfinite(targets).all()
        or np.any(targets < low - 1e-9)
        or np.any(targets > high + 1e-9)
        or np.any(np.abs(rates) > speed + 1e-9)
    ):
        raise ValueError(
            "Standing controller produced a target outside the URDF position or rate envelope"
        )
    return (
        targets,
        indices,
        dict(
            samples=len(rows),
            dry_run_steps=len(grid),
            joints=len(JOINTS),
            mode_machine=rows[0]["mode_machine"],
            max_source_gap_ms=float(gap.max() / 1e6),
            max_receive_gap_ms=float(arrival.max() / 1e6),
            max_sample_age_ms=float(np.max(grid - times[indices]) / 1e6),
            measured_excursion_rad=excursion,
            peak_measured_speed_rad_s=float(np.abs(dq).max()),
            measured_speed_excess_rad_s=speed_excess,
            source_receive_duration_difference_ms=float(clock_skew_ns / 1e6),
            peak_target_feedback_difference_rad=float(
                np.max(np.abs(targets - q[indices]))
            ),
            shoulder_joint=plan["joint"],
            requested_delta_rad=plan["delta_rad"],
        ),
    )


def run(args):
    import numpy as np
    from g1_dds_state import JOINTS

    trial, out = args.trial, args.out
    plan, result, source, meta = identity(trial, args.robot)
    receipt = dict(
        trial=trial.name,
        robot=args.robot,
        controller_sha256=digest(trial / "controller.json"),
        model_sha256=meta["model_hash"],
        simulation_result_sha256=digest(trial / "result.json"),
        kind=result["kind"],
        motor_commands_sent=0,
        physical_qualification=False,
    )
    (out / "controller.json").write_bytes((trial / "controller.json").read_bytes())
    if args.robot == "Nano":
        profile_path = ROOT / "robots/adora_nano_feetech/profile.json"
        profile = read(profile_path)
        required = ["arms", "joint_feedback"]
        if result["kind"] in ("visual-approach", "table-transfer"):
            # Both mobile controllers hold the head and wrists at observation
            # poses and localize with timestamped RGB and joint positions.
            required.extend(["mobile_base", "head", "cameras"])
        if result["kind"] == "table-transfer":
            # The transfer uses wheel encoder deltas to travel a metric distance.
            required.append("measured_odometry")
        missing = [
            key for key in required if profile["capabilities"].get(key) is not True
        ]
        detail = ", ".join(key.replace("_", " ") for key in missing)
        return receipt | dict(
            required_capabilities=required,
            missing_capabilities=missing,
            adapter=profile["name"],
            adapter_profile_sha256=digest(profile_path),
            state="Adapter incomplete" if missing else "Profile declared",
            wheels=profile["wheel_joints"],
            capabilities=profile["capabilities"],
            reason=f"The installed profile maps {len(profile['wheel_joints'])} wheels. Missing for this trial: {detail}."
            if missing
            else "The profile declares the requested channels. Physical input and calibration still require validation.",
        )
    if result["kind"] != "standing-inspection":
        return receipt | dict(
            state="Adapter incomplete",
            reason="Select a G1 standing-reach trial for the 29-joint LowState dry run",
        )
    if source.get("control_sha256") != digest(HERE / "g1_stand.py"):
        raise ValueError(
            "Run this G1 revision again in Nexus to bind the same standing controller implementation"
        )
    weights = None
    if "reach_checkpoint" in plan:
        from g1_reach_checkpoint import load

        weights = load(trial, plan)
        for name in ("training.json", "reach-checkpoint.json"):
            (out / name).write_bytes((trial / name).read_bytes())
    loopback = args.device_mode == "loopback"
    sdk = os.environ.get("DOROBOT_G1_DDS_PYTHON", sys.executable)
    if loopback:
        interface = "lo0" if sys.platform == "darwin" else "lo"
        domain = 32 + secrets.randbelow(160)
        recording = trial / "standing-inputs.npz"
        if not recording.is_file() or result.get("standing_inputs_sha256") != digest(
            recording
        ):
            raise ValueError(
                "Replay needs a recorded G1 episode with verified encoder and IMU inputs"
            )
    else:
        interface = args.device_interface
        domain = args.dds_domain
        if interface not in {n for _, n in socket.if_nameindex()}:
            raise ValueError(
                "Choose a local network interface for the G1 DDS subscriber"
            )
    receipt.update(
        adapter="Unitree G1 HG LowState",
        interface=interface,
        domain=domain,
        source_kind="Nexus loopback replay" if loopback else "External DDS telemetry",
        control_sha256=source["control_sha256"],
    )
    sender = None
    try:
        with (out / "transport.log").open("w") as log:
            try:
                if loopback:
                    sender = subprocess.Popen(
                        [
                            sdk,
                            str(HERE / "g1_dds_fixture.py"),
                            str(recording),
                            interface,
                            str(domain),
                        ],
                        stdout=log,
                        stderr=log,
                    )
                sub = subprocess.run(
                    [
                        sdk,
                        str(HERE / "g1_dds_state.py"),
                        interface,
                        str(domain),
                        str(out),
                    ],
                    stdout=log,
                    stderr=log,
                    timeout=30,
                )
                if sub.returncode or (sender and sender.wait(timeout=5)):
                    log.flush()
                    lines = (out / "transport.log").read_text().strip().splitlines()
                    raise ValueError(lines[-1] if lines else "G1 DDS transport failed")
            finally:
                if sender is not None and sender.poll() is None:
                    sender.kill()
                    sender.wait()
        targets, indices, metrics = replay_samples(
            out / "lowstate.jsonl", plan, meta["model"], weights
        )
        if loopback:
            with np.load(recording, allow_pickle=False) as inputs:
                order = [inputs["joints"].tolist().index(n) for n in JOINTS]
                deviation = float(np.abs(inputs["target"][:, order] - targets).max())
            metrics["max_nexus_target_difference_rad"] = deviation
            if deviation > 1e-5:
                raise ValueError(
                    f"DDS dry-run targets differ from the Nexus reference by {deviation:.6g} rad"
                )
        np.savez_compressed(
            out / "shadow-targets.npz",
            joints=JOINTS,
            target_rad=targets,
            sample_index=indices,
            dt_s=0.005,
        )
        return (
            receipt
            | metrics
            | read(out / "dds.json")
            | dict(
                state="Dry run measured",
                targets_sha256=digest(out / "shadow-targets.npz"),
                telemetry_sha256=digest(out / "lowstate.jsonl"),
            )
        )
    except (OSError, ValueError, subprocess.SubprocessError) as exc:
        return receipt | dict(state="Dry run stopped", reason=str(exc))


def verify_evidence(directory, trial):
    import numpy as np
    from g1_dds_state import JOINTS
    from g1_reach_checkpoint import load

    receipt = read(directory / "preflight.json")
    plan, result, source, meta = identity(trial, receipt["robot"])
    if (
        receipt["state"] != "Dry run measured"
        or receipt["motor_commands_sent"] != 0
        or receipt["physical_qualification"] is not False
        or receipt["controller_sha256"] != digest(directory / "controller.json")
        or receipt["controller_sha256"] != result["controller_hash"]
        or receipt["model_sha256"] != meta["model_hash"]
        or receipt["trial"] != trial.name
        or receipt["simulation_result_sha256"] != digest(trial / "result.json")
        or receipt["control_sha256"] != source["control_sha256"]
        or receipt["control_sha256"] != digest(HERE / "g1_stand.py")
    ):
        raise ValueError(
            "Preflight identity differs from its selected simulation or controller"
        )
    raw, target = directory / "lowstate.jsonl", directory / "shadow-targets.npz"
    if (
        digest(raw) != receipt["telemetry_sha256"]
        or digest(target) != receipt["targets_sha256"]
    ):
        raise ValueError("Preflight evidence digest mismatch")
    weights = load(trial, plan) if "reach_checkpoint" in plan else None
    replay, idx, metrics = replay_samples(raw, plan, meta["model"], weights)
    with np.load(target, allow_pickle=False) as saved:
        if saved["joints"].tolist() != list(JOINTS) or float(saved["dt_s"]) != 0.005:
            raise ValueError("Preflight target axes or clock differ")
        np.testing.assert_array_equal(replay, saved["target_rad"])
        np.testing.assert_array_equal(idx, saved["sample_index"])
    if receipt["source_kind"] == "Nexus loopback replay":
        original = trial / "standing-inputs.npz"
        if digest(original) != result["standing_inputs_sha256"]:
            raise ValueError("Simulation input digest mismatch")
        with np.load(original, allow_pickle=False) as sim:
            order = [sim["joints"].tolist().index(n) for n in JOINTS]
            delta = float(np.max(np.abs(replay - sim["target"][:, order])))
        if delta > 1e-5 or delta != receipt["max_nexus_target_difference_rad"]:
            raise ValueError(
                "Reconstructed targets differ from the original Nexus targets"
            )
    elif receipt["source_kind"] != "External DDS telemetry":
        raise ValueError("Unknown source of device telemetry")
    if any(receipt[k] != v for k, v in metrics.items()):
        raise ValueError("Recorded preflight metrics differ from replay")
    return metrics


if __name__ == "__main__":
    import argparse

    p = argparse.ArgumentParser(
        description="Verify a G1 dry run against its raw telemetry and simulation trial"
    )
    p.add_argument("directory", type=Path)
    p.add_argument("trial", type=Path)
    args = p.parse_args()
    print(json.dumps(verify_evidence(args.directory, args.trial), indent=2))
