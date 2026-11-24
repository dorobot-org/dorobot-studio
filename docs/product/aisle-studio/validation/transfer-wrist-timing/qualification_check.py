"""Verify the committed hidden layouts, motor records and normal wrist deliveries."""

import argparse
import json
from pathlib import Path
import sys
from zipfile import ZipFile

GUIDE = Path(__file__).resolve().parent
sys.path.insert(0, str(GUIDE.parents[4] / "tools/aisle-validation"))
from nano_suite_evidence import verify
from nano_transfer_suite import worker


def main():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("archive", type=Path)
    a = cli.parse_args()
    digest = worker.digest(GUIDE / "qualification-manifest.json")
    verdict = verify(a.archive, digest)
    totals = dict(
        physics_ticks=0, received_wrist_exposures=0, zero_joint_excursion_cases=0
    )
    with ZipFile(a.archive) as z:
        names = set(z.namelist())
        for i in range(verdict["cases"]):
            prefix = f"case-{i:03}/"
            result = json.loads(z.read(prefix + "result.json"))
            timing = result["wrist_timing"]
            assert (
                timing["camera"] == "left_wrist"
                and timing["phase"] == "pick"
                and timing["fixture"] == "normal"
            )
            assert (
                not timing["rejected"]
                and not timing["timed_out"]
                and not timing["recoveries"]
            )
            rows = (
                []
                if prefix + "wrist-delivery.jsonl" not in names
                else [
                    json.loads(line)
                    for line in z.read(prefix + "wrist-delivery.jsonl").splitlines()
                ]
            )
            assert len(rows) == timing["received"]
            if result.get("task_success", result["passed"]):
                assert rows
            prior = -1
            for row in rows:
                assert (
                    prior
                    < row["capture_tick"]
                    == row["tick"]
                    <= result["physics_ticks"]
                )
                assert (
                    row["receive_ns"] == row["capture_ns"] == row["tick"] * 10_000_000
                )
                assert (
                    row["fault"] is None
                    and row["source_sha256"] == row["delivered_sha256"]
                )
                prior = row["tick"]
            totals["physics_ticks"] += result["physics_ticks"]
            totals["received_wrist_exposures"] += len(rows)
            totals["zero_joint_excursion_cases"] += int(
                result["max_joint_excursion_rad"] == 0
            )
    print(
        json.dumps(
            dict(**verdict, **totals, archive_sha256=worker.digest(a.archive)), indent=2
        )
    )


if __name__ == "__main__":
    main()
