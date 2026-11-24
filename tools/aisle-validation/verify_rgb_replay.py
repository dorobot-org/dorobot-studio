"""Replay a recorded controller from RGB and encoders and compare its emitted decisions."""

import argparse
import json
from pathlib import Path

import numpy as np

from nano_rgb_policy import VisualServo
from run_rgb_approach import timing_fault


def replay(run, model):
    import hashlib

    calibration = json.loads((run / "calibration.json").read_text())
    result = json.loads((run / "result.json").read_text())
    if hashlib.sha256(model.read_bytes()).hexdigest() != result["model_sha256"]:
        raise ValueError("URDF and recorded robot differ")
    servo = VisualServo(calibration | {"model": str(model)})
    packets = sorted((run / "observations").glob("*.npz"))
    if not packets or len(packets) != result["decisions"]:
        raise ValueError("Incomplete observation sequence")
    with (run / "trace.jsonl").open() as trace:
        for index, (packet, line) in enumerate(zip(packets, trace, strict=True)):
            expected = json.loads(line)
            with np.load(packet, allow_pickle=False) as data:
                if "receive_ns" in expected:
                    captured = int(data["stamp_ns"])
                    received = int(packet.stem) * 10_000_000
                    fault = timing_fault(captured, received, servo.last_stamp)
                    if (captured, received, fault) != (
                        expected["capture_ns"],
                        expected["receive_ns"],
                        expected["sensor_fault"],
                    ):
                        raise ValueError(
                            f"Recorded camera timing differs at observation {index}"
                        )
                else:
                    fault = None
                generated = (
                    servo.stop("waiting for current RGB")
                    if fault
                    else servo.command(data)
                )
            if index != expected["sequence"] or generated != expected["command"]:
                raise ValueError(f"First differing command at observation {index}")
    return len(packets)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run", type=Path)
    parser.add_argument(
        "--model",
        type=Path,
        default=Path(__file__).resolve().parents[2] / "data/adora_nano/adora_nano.urdf",
    )
    args = parser.parse_args()
    print(f"Reproduced {replay(args.run, args.model)} visual servo decisions")
