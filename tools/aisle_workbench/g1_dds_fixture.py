"""Publish a recorded Nexus sensor episode as G1 LowState on a loopback DDS domain."""

import argparse
from pathlib import Path
import threading
import time

import numpy as np

from g1_dds_state import channel, JOINTS


def publish(path, interface, domain_id):
    from cyclonedds.core import Listener
    from cyclonedds.pub import DataWriter
    from unitree_sdk2py.idl.default import unitree_hg_msg_dds__LowState_

    if interface not in ("lo", "lo0") or domain_id == 0:
        raise ValueError(
            "Choose a loopback interface and a dedicated nonzero DDS domain for replay"
        )
    with np.load(path, allow_pickle=False) as sensors:
        names = sensors["joints"].tolist()
        if set(names) != set(JOINTS) or len(names) != len(JOINTS):
            raise ValueError("Recording must contain all 29 G1 joints")
        indices = [names.index(n) for n in JOINTS]
        q, dq, quat = (
            sensors["q"][:, indices],
            sensors["dq"][:, indices],
            sensors["quat"],
        )
        if (
            q.shape != (2200, 29)
            or dq.shape != q.shape
            or quat.shape != (2200, 4)
            or sensors["dt"] != 0.005
        ):
            raise ValueError("Expected a complete 200 Hz standing observation trace")
    domain, participant, topic = channel(interface, domain_id)
    ready = threading.Event()

    def matched(writer, status):
        if status.current_count:
            ready.set()

    writer = DataWriter(
        participant, topic, listener=Listener(on_publication_matched=matched)
    )
    if not ready.wait(5):
        raise ValueError("No matching loopback G1 LowState subscriber")
    msg = unitree_hg_msg_dds__LowState_()
    start = time.monotonic()
    epoch = time.time_ns()
    for i in range(len(q)):
        remaining = start + i * 0.005 - time.monotonic()
        if remaining > 0:
            time.sleep(remaining)
        msg.tick = i * 5
        msg.imu_state.quaternion = quat[i].tolist()
        for motor, pos, vel in zip(msg.motor_state, q[i], dq[i]):
            motor.q, motor.dq = float(pos), float(vel)
        writer.write(msg, timestamp=epoch + i * 5_000_000)
    time.sleep(0.1)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("recording", type=Path)
    p.add_argument("interface")
    p.add_argument("domain", type=int)
    args = p.parse_args()
    publish(args.recording, args.interface, args.domain)
