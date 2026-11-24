"""Compare eight H100 workers at the same wall-clock exposure period, before and after transfer compression."""

import argparse
import hashlib
import gzip
from statistics import mean, median
import json
import sys
from pathlib import Path
from zipfile import ZipFile

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "wall-clock"))
from compare import compare, delivery_intervals


def check_native(snapshot_path, earlier, current):
    with ZipFile(snapshot_path) as recorded:
        raw = recorded.read("snapshot.rgb")
        native = json.loads(recorded.read("native.json"))
        peers = json.loads(recorded.read("peers.json"))
    assert native["backend"] == "headless" and native["requested_size"] == "1024x768"
    assert native["background_stopped"] and native["foreground_passed"]
    assert (
        native["foreground_job"] != native["background_job"]
        and native["foreground_frames"] > 0
    )
    assert native["recovered"] == "Imported worker trial and export recovered"
    assert len(raw) == 4 + 480 * 360 * 3
    assert (
        int.from_bytes(raw[:4], "big")
        == native["snapshot"]["tick"]
        == native["source"]["tick"]
    )
    assert (
        hashlib.sha256(raw[4:]).hexdigest()
        == native["snapshot"]["sha256"]
        == native["source"]["sha256"]
    )
    for name, path in (("previous", earlier), ("updated", current)):
        peer = peers[name]
        with ZipFile(path) as cohort:
            pixels = cohort.read("codex/gpu1/preview.rgb")
            receipt = json.loads(cohort.read("codex/gpu1/delivery.json"))
        assert receipt["id"] == peer["job"]
        assert peer["state"] == receipt["state"] == "completed"
        assert int.from_bytes(pixels[:4], "big") == peer["tick"]
        assert hashlib.sha256(pixels[4:]).hexdigest() == peer["rgb_sha256"]
        assert peer["legacy_image_bytes"] == 691200
    assert peers["previous"]["calls"] == ["observation", "status"]
    assert (
        peers["previous"]["repeated_tick_image_bytes"]
        == peers["previous"]["negotiated_image_bytes"]
        == 691200
    )
    assert peers["updated"]["calls"] == ["observation"]
    assert peers["updated"]["repeated_tick_image_bytes"] == 0
    assert peers["updated"]["negotiated_image_bytes"] < 691200


def check_source_profile(archive):
    record = json.loads(gzip.decompress((HERE / "source-stages.json.gz").read_bytes()))
    rows = record["raw_ns"]
    assert (
        record["backend"] == "webgpu" and len(rows) == record["measured_ticks"] == 256
    )
    assert [r["tick"] for r in rows] == list(
        range(record["warmup_ticks"] + 1, record["warmup_ticks"] + 257)
    )
    with ZipFile(archive) as match:
        assert (
            hashlib.sha256(match.read("codex/gpu1/motor-targets.f64")).hexdigest()
            == record["motor_sha256"]
        )
    stages = ("write", "physics", "score", "sync", "head")
    assert all(sum(row[stage] for stage in stages) == row["total"] for row in rows)
    assert all(
        median(row[stage] for row in rows) / 1e6 == value
        for stage, value in record["median_ms"].items()
    )
    assert mean(row["total"] for row in rows) / 1e6 == record["mean_total_ms"]


def measure(earlier, current, datasets):
    summaries = compare(
        earlier, current, datasets, baseline_clock="wall", baseline_period=0.5
    )
    with ZipFile(earlier) as baseline, ZipFile(current) as measured:
        for name, summary in summaries.items():
            summary["previous_receipt_intervals"] = delivery_intervals(baseline, name)
            wires = []
            for gpu in range(1, 8):
                prefix = f"{name}/gpu{gpu}/"
                received = json.loads(measured.read(prefix + "delivery.json"))
                stats = received["observation_transport"]
                samples = received["samples"]
                assert (
                    stats["frames"] == len(samples)
                    and stats["zlib_frames"] <= stats["frames"]
                )
                assert stats["raw_rgb_bytes"] == len(samples) * 480 * 360 * 3
                assert 0 < stats["base64_bytes"] <= len(samples) * 691200
                wires.append(stats)
            totals = {key: sum(row[key] for row in wires) for key in wires[0]}
            totals["base64_bytes_if_raw"] = 4 * (totals["raw_rgb_bytes"] // 3)
            totals["payload_reduction_percent"] = 100 * (
                1 - totals["base64_bytes"] / totals["base64_bytes_if_raw"]
            )
            summary["transport"] = totals
    return summaries


if __name__ == "__main__":
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument(
        "--baseline", type=Path, default=HERE.parent / "wall-clock/measurements.zip"
    )
    cli.add_argument("--archive", type=Path, default=HERE / "measurements.zip")
    cli.add_argument("--datasets", action="store_true")
    args = cli.parse_args()
    check_source_profile(args.archive)
    check_native(HERE / "native-snapshot.zip", args.baseline, args.archive)
    print(json.dumps(measure(args.baseline, args.archive, args.datasets), indent=2))
