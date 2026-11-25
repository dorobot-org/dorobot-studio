"""Collect the measured native and three-cube trials for the wrist timing check."""

import argparse
import json
from pathlib import Path
from zipfile import ZipFile, ZIP_DEFLATED

MODES = ("normal", "intermittent", "disconnected")
FILES = (
    "controller.json",
    "result.json",
    "calibration.json",
    "source.json",
    "motor-targets.f64",
    "wrist-delivery.jsonl",
    "recording.json",
    "phases.json",
    "actions.jsonl",
)


def pack_trial(z, prefix, folder):
    for name in FILES:
        z.write(folder / name, prefix + "/" + name)
    for f in sorted((folder / "wrist-exposures").glob("*.png")):
        z.write(f, prefix + "/wrist-exposures/" + f.name)


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("experiment", type=Path)
    cli.add_argument("archive", type=Path)
    a = cli.parse_args()
    with ZipFile(a.archive, "x", ZIP_DEFLATED, compresslevel=6) as z:
        for tag, backend, group in (
            ("codex", "headless", "native"),
            ("deepseek", "offscreen", "native"),
            ("codex-three", "headless", "final-three-native"),
            ("deepseek-three", "offscreen", "final-three-native"),
        ):
            folder = a.experiment / (group + "-" + backend)
            for name in ("journey.json", "recovery.json", "mode-manifest.json"):
                z.write(folder / "verified" / name, tag + "/" + name)
            (author,) = (folder / "state/aisle").glob("*-Nano-prepare")
            for name in ("controller.json", "provider-metrics.json"):
                z.write(author / name, f"{tag}/author/{name}")
            trial_dirs = sorted((folder / "state/aisle").glob("*-Nano-run"))
            assert len(trial_dirs) == len(MODES)
            for mode, trial_dir in zip(MODES, trial_dirs, strict=True):
                assert (
                    json.loads((trial_dir / "result.json").read_bytes())[
                        "wrist_timing"
                    ]["fixture"]
                    == mode
                )
                pack_trial(z, tag + "/" + mode, trial_dir)
        for group in ("three", "revised"):
            for mode in MODES:
                folder = a.experiment / (group + "-" + mode)
                prefix = group + "/" + mode
                pack_trial(z, prefix, folder)
                if mode == "disconnected":
                    continue
                actions = [
                    json.loads(line)
                    for line in (folder / "actions.jsonl").read_bytes().splitlines()
                ]
                start = next(
                    i
                    for i, row in enumerate(actions)
                    if row.get("name") == "place" and row["color"] == "blue"
                )
                decision = next(
                    row["tick"] for row in actions[start + 1 :] if row["op"] == "sense"
                )
                record = json.loads((folder / "recording.json").read_bytes())
                index, sample = min(
                    enumerate(record["samples"]),
                    key=lambda t: abs(t[1]["step"] - decision),
                )
                z.writestr(
                    prefix + "/release-sample.json",
                    json.dumps(
                        dict(
                            decision_tick=decision,
                            sample_tick=sample["step"],
                            index=index,
                        )
                    ),
                )
                for camera in record["cameras"]:
                    z.write(
                        folder / "camera" / camera / f"{index:04d}.png",
                        prefix + "/release-" + camera + ".png",
                    )
        for mode in ("intermittent", "disconnected"):
            dataset = a.experiment / ("export-" + mode) / "dataset"
            for path in sorted(dataset.rglob("*")):
                if path.is_file():
                    z.write(path, f"exports/{mode}/{path.relative_to(dataset)}")
        z.write(
            a.experiment / "legacy-approach/receipt.json", "fleet/legacy-approach.json"
        )
        for f in ("receipt.json", "wrist-timeout.zip"):
            z.write(a.experiment / "fleet" / f, "fleet/" + f)


if __name__ == "__main__":
    main()
