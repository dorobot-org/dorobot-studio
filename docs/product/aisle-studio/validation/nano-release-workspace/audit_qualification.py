"""Replay a published Nano qualification and print its full case-level verdict."""

import argparse
from hashlib import sha256
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[5]
sys.path.insert(0, str(ROOT / "tools/aisle-validation"))
from nano_suite_evidence import verify


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("cohort", type=Path)
    cohort = parser.parse_args().cohort
    manifest = sha256((cohort / "suite.json").read_bytes()).hexdigest()
    catalog = json.loads(
        (ROOT / "tools/aisle-validation/nano_published_outputs.json").read_text()
    )
    receipt = (cohort / "evidence.zip.sha256").read_text().strip()
    if catalog.get(manifest) != receipt:
        raise ValueError("Output receipt differs from the published qualification")
    result = verify(
        cohort / "evidence.zip", manifest, include_trials=True, expected_archive=receipt
    )
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
