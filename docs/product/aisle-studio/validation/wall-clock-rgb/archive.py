"""Extract the measured native and robot records used in the observation-clock audit."""

import argparse
import hashlib
import json
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile

BASE_CASES = (
    "eight-wall-deepseek",
    "eight-wall-codex",
    "eight-half-deepseek",
    "eight-half-codex",
)
EXTENDED_CASES = (
    "10-half-deepseek",
    "10-half-codex",
    "12-half-deepseek",
    "16-half-deepseek",
    "12-half-codex",
    "16-half-codex",
)
NATIVE = (
    "native-wall-offscreen",
    "native-wall-headless",
    "final-offscreen",
    "final-headless",
    "snapshot-first-offscreen",
    "snapshot-actions-offscreen",
    "snapshot-actions-headless",
)
G1 = (
    "r1-simulation",
    "r1-wall",
    "paired-sim-a",
    "paired-wall-a",
    "paired-wall-b",
    "paired-sim-b",
    "half",
)


def add_case(z, root, case, capacity):
    run = root / case
    verified = run / "verified"
    state = run / "state/aisle"
    proof = json.loads((verified / "journey.json").read_bytes())[-1]

    def add(path, name):
        z.write(path, case + "/" + name)

    for f in ("journey.json", "recovery.json"):
        add(verified / f, f)
    manifest = json.loads((verified / "mode-manifest.json").read_bytes())
    keys = (
        "backend",
        "requested_size",
        "provider",
        "native_binary_sha256",
        "studio_source_commit",
        "source_files_sha256",
    )
    z.writestr(
        case + "/native-build.json",
        json.dumps({k: manifest[k] for k in keys}, indent=2),
    )
    origins = {
        json.loads(p.read_bytes())["job"]: p.parent
        for p in state.glob("*-run/worker-origin.json")
    }
    foreground = origins[proof["foreground_id"]]
    for f in (
        "controller.json",
        "source.json",
        "result.json",
        "motor-targets.f64",
        "preview-timing.json",
    ):
        add(foreground / f, "foreground/" + f)
    (author,) = state.glob("*-Nano-prepare")
    add(author / "provider-metrics.json", "foreground/author.json")
    (dataset,) = state.glob("*-Nano-export/dataset")
    for f in ("meta/info.json", "meta/aisle.json"):
        add(dataset / f, "foreground/" + f)
    if case == "eight-half-deepseek":
        for f in dataset.rglob("*"):
            if f.is_file():
                add(f, "dataset/" + str(f.relative_to(dataset)))
    if not capacity:
        original = origins[proof["background_id"]]
        for f in ("source.json", "background-observations.jsonl"):
            add(original / f, "selected-observer/" + f)
        return
    for f in ("preview-cadence.json", "background-verdicts.json"):
        add(verified / f, f)
    draws = [
        json.loads(line)
        for line in (verified / "journey-draws.jsonl").read_text().splitlines()
    ]
    for draw in draws:
        draw["source"] = Path(draw["source"]).name
    z.writestr(case + "/draws.jsonl", "".join(json.dumps(row) + "\n" for row in draws))
    jobs = json.loads((verified / "background-verdicts.json").read_bytes())
    for i, job in enumerate(jobs):
        original = run / f"background-{i}.zip"
        assert (
            hashlib.sha256(original.read_bytes()).hexdigest() == job["archive_sha256"]
        )
        with ZipFile(original) as trial:
            for f in (
                "background-observations.jsonl",
                "motor-targets.f64",
                "controller.json",
                "source.json",
                "result.json",
                "preview.rgb",
            ):
                z.writestr(f"{case}/observer-{i}/{f}", trial.read(f))
        add(
            run / f"background-author-{i}/provider-metrics.json",
            f"observer-{i}/author.json",
        )


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("experiment", type=Path)
    cli.add_argument("out", type=Path)
    a = cli.parse_args()
    with ZipFile(a.out, "x", ZIP_DEFLATED, compresslevel=6) as z:
        capacity = list(BASE_CASES) + [
            c for c in EXTENDED_CASES if (a.experiment / c).is_dir()
        ]
        z.writestr("capacity-cases.json", json.dumps(capacity))
        for case in capacity:
            add_case(z, a.experiment, case, True)
        for case in NATIVE:
            add_case(z, a.experiment, case, False)
        single = a.experiment / "runner-metal-smoke"
        z.write(single / "metal.json", "runner/receipt.json")
        z.write(single / "metal.zip", "runner/worker.zip")
        for tag in G1:
            folder = a.experiment / ("g1-" + tag)
            for name in (
                "source.json",
                "result.json",
                "controller.json",
                "motor-work.npz",
                "wrist-trace.npz",
                "background-observations.jsonl",
                "preview.rgb",
            ):
                z.write(folder / name, f"g1/{tag}/{name}")


if __name__ == "__main__":
    main()
