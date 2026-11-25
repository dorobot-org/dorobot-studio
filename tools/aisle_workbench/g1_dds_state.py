"""Capture Unitree HG LowState measurements on an explicitly selected DDS interface."""

import argparse
import json
from pathlib import Path
import socket
import time
from xml.sax.saxutils import quoteattr


TOPIC = "rt/lowstate"
# Unitree SDK2 G1JointIndex, series pitch/roll configuration (mode_pr == 0).
JOINTS = (
    *[
        f"{side}_{joint}_joint"
        for side in ("left", "right")
        for joint in (
            "hip_pitch",
            "hip_roll",
            "hip_yaw",
            "knee",
            "ankle_pitch",
            "ankle_roll",
        )
    ],
    "waist_yaw_joint",
    "waist_roll_joint",
    "waist_pitch_joint",
    *[
        f"{side}_{joint}_joint"
        for side in ("left", "right")
        for joint in (
            "shoulder_pitch",
            "shoulder_roll",
            "shoulder_yaw",
            "elbow",
            "wrist_roll",
            "wrist_pitch",
            "wrist_yaw",
        )
    ],
)


def channel(interface, domain_id):
    from cyclonedds.domain import Domain, DomainParticipant
    from cyclonedds.topic import Topic
    from unitree_sdk2py.idl.unitree_hg.msg.dds_ import LowState_

    if interface not in {name for _, name in socket.if_nameindex()}:
        raise ValueError(f"Unknown network interface {interface!r}")
    if not 0 <= domain_id <= 232:
        raise ValueError("DDS domain must be between 0 and 232")
    net = f"<NetworkInterface name={quoteattr(interface)}/>"
    peers = (
        '<Discovery><Peers><Peer address="127.0.0.1"/></Peers></Discovery>'
        if interface in ("lo", "lo0")
        else ""
    )
    config = f"<CycloneDDS><Domain id='any'><General><Interfaces>{net}</Interfaces></General>{peers}</Domain></CycloneDDS>"
    domain = Domain(domain_id, config)
    participant = DomainParticipant(domain_id)
    return domain, participant, Topic(participant, TOPIC, LowState_)


def capture(interface, domain_id, out):
    from cyclonedds.qos import Policy, Qos
    from cyclonedds.sub import DataReader
    from cyclonedds.util import duration
    from importlib.metadata import version

    domain, participant, topic = channel(interface, domain_id)
    reader = DataReader(participant, topic, qos=Qos(Policy.History.KeepLast(4096)))
    raw = out / "lowstate.jsonl"
    first = None
    rows = 0
    with raw.open("x") as log:
        while rows < 25000:
            # First publisher discovery and subsequent stream silence have separate deadlines.
            sample = next(
                reader.take_iter(
                    timeout=duration(seconds=5 if first is None else 0.25)
                ),
                None,
            )
            if sample is None or not hasattr(sample, "motor_state"):
                raise ValueError(
                    "G1 LowState stream timed out or publisher disconnected"
                )
            stamp = sample.sample_info.source_timestamp
            if first is None:
                first = stamp
            motors = sample.motor_state[: len(JOINTS)]
            row = dict(
                source_ns=stamp,
                received_ns=time.monotonic_ns(),
                tick=sample.tick,
                publisher=sample.sample_info.publication_handle,
                mode_pr=sample.mode_pr,
                mode_machine=sample.mode_machine,
                quat=sample.imu_state.quaternion,
                q=[m.q for m in motors],
                dq=[m.dq for m in motors],
            )
            print(json.dumps(row, allow_nan=False, separators=(",", ":")), file=log)
            rows += 1
            if stamp - first >= 10_995_000_000:
                return dict(
                    samples=rows,
                    sdk=version("unitree-sdk2py"),
                    cyclonedds=version("cyclonedds"),
                )
    raise ValueError(
        "G1 stream exceeded the sample budget before the observation window elapsed"
    )


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("interface")
    p.add_argument("domain", type=int)
    p.add_argument("output", type=Path)
    a = p.parse_args()
    (a.output / "dds.json").write_text(
        json.dumps(capture(a.interface, a.domain, a.output), indent=2)
    )
