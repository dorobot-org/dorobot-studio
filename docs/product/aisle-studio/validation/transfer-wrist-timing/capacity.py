"""Pack or audit the two registered ten-session repeats using the original capacity verifier."""

import argparse
import json
from pathlib import Path
import sys
from zipfile import ZIP_DEFLATED, ZipFile

previous = Path(__file__).resolve().parent.parent / "wall-clock-rgb"
sys.path.insert(0, str(previous))
from archive import add_case
from audit import builds, case

TAGS = ("10-half-deepseek", "10-half-codex")


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("archive", type=Path)
    cli.add_argument("--pack-from", type=Path)
    cli.add_argument("--git-sources", action="store_true")
    cli.add_argument("--case", choices=TAGS, help="Pack or verify just this cohort")
    a = cli.parse_args()
    tags = (a.case,) if a.case else TAGS
    if a.pack_from:
        with ZipFile(a.archive, "x", ZIP_DEFLATED) as z:
            for tag in tags:
                add_case(z, a.pack_from, tag, True)
    with ZipFile(a.archive) as z:
        result = dict(builds=builds(z, tags, a.git_sources))
        for tag in tags:
            measured = case(z, tag, True)
            for prefix in ("foreground", *(f"observer-{i}" for i in range(9))):
                run = json.loads(z.read(f"{tag}/{prefix}/result.json"))
                assert run["wrist_timing"]["fixture"] == "normal"
                assert run["wrist_timing"]["received"] > 0
                assert not run["wrist_timing"]["rejected"]
            result[tag] = measured
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
