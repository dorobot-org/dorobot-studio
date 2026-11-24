"""Package the two complete capacity measurements for public offline review."""

import argparse
import json
from pathlib import Path
from zipfile import ZipFile, ZIP_DEFLATED


def pack(source, target, name):
    folder = source / name
    run = folder / "measurement"
    state = run / "native/state/aisle"
    prefix = name + "/"

    def add(path, rel):
        target.write(path, prefix + rel)

    for file in ("timing.json", "draws.jsonl", "gpu.jsonl", "gpu-processes.txt"):
        add(run / file, file)
    add(folder / "authors/results.json", "authors.json")
    for i, author in enumerate(sorted((folder / "authors").glob("*-Nano-prepare"))):
        base = f"gpu{i}/"
        for filename in (
            "controller.json",
            "controller-request.json",
            "provider-metrics.json",
            "provider-response.json",
            "reply.json",
            "feedback.json",
        ):
            path = author / filename
            if path.exists():
                add(path, base + "author/" + filename)
        if i:
            add(run / f"gpu{i}.json", base + "delivery.json")
            with ZipFile(run / f"gpu{i}.zip") as worker:
                for filename in (
                    "source.json",
                    "result.json",
                    "controller.json",
                    "motor-targets.f64",
                    "background-observations.jsonl",
                ):
                    target.writestr(prefix + base + filename, worker.read(filename))
        else:
            verified = run / "native/verified"
            for filename in (
                "journey.json",
                "recovery.json",
                "foreground.png",
                "imported-replay.png",
            ):
                add(verified / filename, base + filename)
            manifest = json.loads((verified / "mode-manifest.json").read_bytes())
            fields = (
                "backend",
                "requested_size",
                "provider",
                "native_binary_sha256",
                "studio_source_commit",
                "source_files_sha256",
            )
            target.writestr(
                prefix + base + "native-build.json",
                json.dumps({k: manifest[k] for k in fields}, indent=2),
            )
            proof = json.loads((verified / "journey.json").read_bytes())[-1]
            trial = state / proof["trial"]
            for filename in (
                "source.json",
                "result.json",
                "controller.json",
                "motor-targets.f64",
                "preview-timing.json",
            ):
                add(trial / filename, base + filename)
            (dataset,) = state.glob("*-Nano-export/dataset")
            for info in ("meta/info.json", "meta/episodes.jsonl"):
                add(dataset / info, base + "dataset/" + info)


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("source", type=Path)
    cli.add_argument("output", type=Path)
    cli.add_argument("--cohorts", nargs="+", default=["deepseek", "codex"])
    args = cli.parse_args()
    with ZipFile(args.output, "x", ZIP_DEFLATED, compresslevel=6) as target:
        for name in args.cohorts:
            pack(args.source, target, name)


if __name__ == "__main__":
    main()
