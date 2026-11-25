"""Check the default and explicitly selected release methods against their measured originals."""

import argparse
import hashlib
import json
from pathlib import Path
from zipfile import ZipFile, ZIP_DEFLATED

GUIDE = Path(__file__).resolve().parent
MODES = ("normal", "intermittent", "disconnected")
GROUPS = {"default": None, "opt-in-codex": "headless", "opt-in-deepseek": "offscreen"}
FILES = (
    "result.json",
    "source.json",
    "controller.json",
    "motor-targets.f64",
    "wrist-delivery.jsonl",
)


def pack(root, archive):
    with ZipFile(archive, "x", ZIP_DEFLATED, compresslevel=6) as z:
        for group, backend in GROUPS.items():
            if backend:
                experiment = root / ("qualified-selection-" + backend)
                (author,) = (experiment / "state/aisle").glob("*-Nano-prepare")
                for name in ("controller.json", "provider-metrics.json"):
                    z.write(author / name, f"{group}/author/{name}")
                for name in ("journey.json", "mode-manifest.json", "recovery.json"):
                    z.write(experiment / "verified" / name, group + "/" + name)
                trials = sorted((experiment / "state/aisle").glob("*-Nano-run"))
            else:
                trials = [root / ("default-" + mode) for mode in MODES]
            for mode, folder in zip(MODES, trials, strict=True):
                for name in FILES:
                    z.write(folder / name, f"{group}/{mode}/{name}")


def verify(archive):
    reports = {}
    with ZipFile(archive) as z, ZipFile(GUIDE / "measurements.zip") as original:
        for group, backend in GROUPS.items():
            experimental = backend is not None
            baseline = "revised" if experimental else "three"
            reports[group] = {}
            for mode in MODES:
                prefix = f"{group}/{mode}/"
                old = f"{baseline}/{mode}/"
                result = json.loads(z.read(prefix + "result.json"))
                previous = json.loads(original.read(old + "result.json"))
                src = json.loads(z.read(prefix + "source.json"))
                raw = z.read(prefix + "controller.json")
                plan = json.loads(raw)
                assert plan.get("wrist_clearance", False) is experimental
                assert (
                    src["wrist_clearance"] is result["wrist_clearance"] is experimental
                )
                assert (
                    result["wrist_timing"]["fixture"] == src["wrist_rgb_timing"] == mode
                )
                assert (
                    hashlib.sha256(raw).hexdigest()
                    == src["controller_sha256"]
                    == result["controller_hash"]
                )
                assert (
                    result["requested"]
                    == ["red", "green", "blue"]
                    == previous["requested"]
                )
                assert result["task_success"] is previous["task_success"]
                assert (
                    result["physics_ticks"]
                    == previous["physics_ticks"]
                    == result["wheel_supported_ticks"]
                )
                assert set(result["cubes"]) == set(previous["cubes"])
                assert all(
                    result["cubes"][color]["passed"]
                    == previous["cubes"][color]["passed"]
                    for color in result["requested"]
                )
                motors = z.read(prefix + "motor-targets.f64")
                assert motors == original.read(old + "motor-targets.f64")
                assert (
                    hashlib.sha256(motors).hexdigest()
                    == result["actuator_audit"]["sha256"]
                )
                assert (
                    result["max_joint_excursion_rad"] == 0
                    and result["actuator_audit"]["passed"]
                )
                assert z.read(prefix + "wrist-delivery.jsonl") == original.read(
                    old + "wrist-delivery.jsonl"
                )
                if backend:
                    assert raw == z.read(group + "/author/controller.json")
                else:
                    assert raw == original.read(old + "controller.json")
                reports[group][mode] = dict(
                    placed=[
                        c for c in result["requested"] if result["cubes"][c]["passed"]
                    ],
                    success=result["task_success"],
                    physics_ticks=result["physics_ticks"],
                    motor_sha256=result["actuator_audit"]["sha256"],
                    controller_sha256=result["controller_hash"],
                )
            if backend:
                manifest = json.loads(z.read(group + "/mode-manifest.json"))
                metrics = json.loads(z.read(group + "/author/provider-metrics.json"))
                assert (
                    manifest["backend"] == backend
                    and manifest["provider"] == metrics["provider"]
                )
                recover = json.loads(z.read(group + "/recovery.json"))
                assert {r["check"] for r in recover} == {
                    "Wrist timeout recovered after restart",
                    "Restored on-time wrist delivery",
                }
                reports[group].update(
                    author=metrics, source_commit=manifest["studio_source_commit"]
                )
    return reports


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("archive", type=Path)
    cli.add_argument("--pack-from", type=Path)
    a = cli.parse_args()
    if a.pack_from:
        pack(a.pack_from, a.archive)
    print(json.dumps(verify(a.archive), indent=2))


if __name__ == "__main__":
    main()
