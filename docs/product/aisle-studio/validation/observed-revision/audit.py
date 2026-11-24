"""Replay all published Nano pickup exposures, contact traces and motor journals."""

import argparse
import hashlib
import json
from pathlib import Path
import sys
from tempfile import TemporaryDirectory
from zipfile import ZipFile

PROOF = Path(__file__).resolve().parent
sys.path.insert(0, str(PROOF.parents[4] / "tools/aisle_workbench"))
from moving_grasp_check import verify_trial


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--archive", type=Path, default=PROOF / "trials.zip")
    args = p.parse_args()
    expected = json.loads((PROOF / "measurements.json").read_bytes())
    assert (
        hashlib.sha256(args.archive.read_bytes()).hexdigest()
        == expected["archive_sha256"]
    )
    with ZipFile(args.archive) as archive:
        index = json.loads(archive.read("index.json"))
        assert set(index) == set(expected["trials"])
        for case, files in index.items():
            with TemporaryDirectory() as temp:
                folder = Path(temp)
                for name, digest in files.items():
                    assert not Path(name).is_absolute() and ".." not in Path(name).parts
                    data = archive.read("objects/" + digest)
                    assert hashlib.sha256(data).hexdigest() == digest
                    dest = folder / name
                    dest.parent.mkdir(exist_ok=True, parents=True)
                    dest.write_bytes(data)
                recorded = expected["trials"][case]
                controller = (folder / "controller.json").read_bytes()
                assert (
                    controller
                    == (PROOF / recorded["program"] / "controller.json").read_bytes()
                )
                scored = verify_trial(folder)
                assert scored["seed"] == recorded["seed"]
                assert scored["motor_sha256"] == recorded["motor_sha256"]
                assert scored["passed"] == recorded["task_success"]
                assert scored["placements"] == recorded["placements"]
                assert {
                    c: scored["cubes"].get(c, {}).get("passed", False)
                    for c in recorded["placements"]
                } == recorded["moving_grasp"]
                print(
                    f"{case}: {sum(scored['placements'].values())}/3 placed, task passed={scored['passed']}"
                )


if __name__ == "__main__":
    main()
