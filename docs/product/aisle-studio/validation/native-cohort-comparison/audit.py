"""Check native journeys and restart outcomes against the published simulation reports."""

from collections import Counter
import hashlib
import json
from pathlib import Path
from zipfile import ZipFile

GUIDE = Path(__file__).resolve().parent
VALIDATION = GUIDE.parent
NANO = VALIDATION / "transfer-wrist-timing"
G1 = VALIDATION / "g1-thirty-degree-reach"


def report(archive):
    with ZipFile(archive) as z:
        return json.loads(z.read("report.json"))


def compare_recorded(result, archives):
    rows = []
    for side, archive in zip(("baseline", "candidate"), archives, strict=True):
        stored = result[side]
        with archive.open("rb") as f:
            assert (
                stored["archive_sha256"] == hashlib.file_digest(f, "sha256").hexdigest()
            )
        scored = report(archive)
        assert stored["successes"] == scored["successes"]
        assert stored["manifest_sha256"] == scored["manifest_sha256"]
        rows.append(scored["rows"])
    expected = Counter()
    assert len(result["pairs"]) == len(rows[0]) == len(rows[1])
    for side in ("baseline_index", "candidate_index"):
        assert sorted(p[side] for p in result["pairs"]) == list(
            range(len(result["pairs"]))
        )
    for pair in result["pairs"]:
        a = rows[0][pair["baseline_index"]]
        b = rows[1][pair["candidate_index"]]
        key = "seed" if result["robot"] == "Nano" else "recipe"
        assert pair[key] == a[key] == b[key]
        assert pair["both_verified"] == (a["verified"] and b["verified"])
        assert (pair["baseline_passed"], pair["candidate_passed"]) == (
            a["passed"],
            b["passed"],
        )
        expected[(a["passed"], b["passed"])] += 1
    assert result["counts"] == dict(
        both_passed=expected[True, True],
        improved=expected[False, True],
        regressed=expected[True, False],
        both_failed=expected[False, False],
        unverified=0,
    )


def main():
    source = {
        "nano": (NANO / "matched-baseline.zip", NANO / "qualification.zip"),
        "g1": (
            VALIDATION / "g1-upright-trunk/matched.zip",
            VALIDATION / "g1-standing-heading/matched.zip",
        ),
        "repeat": (G1 / "a.zip", G1 / "b.zip"),
    }
    runs = {
        "nano-desktop": "nano",
        "nano-compact": "nano",
        "small-rendered": "nano",
        "g1-desktop": "g1",
        "g1-compact": "g1",
        "g1-repeat": "repeat",
        "incomplete": "missing",
    }
    binaries = set()
    with ZipFile(GUIDE / "native.zip") as z:
        for name, study in runs.items():
            first, restored, build = [
                json.loads(z.read(f"{name}/{f}.json"))
                for f in ("journey", "recovery", "mode-manifest")
            ]
            result = first[0]["evaluation"]
            assert result == restored[0]["evaluation"]
            assert restored[0]["check"] == "Cohort comparison restored"
            assert len(set(s["pid"] for s in build["sessions"])) == 2
            binaries.add(build["native_binary_sha256"])
            labels = [r["check"] for r in first]
            assert labels[-3:] == [
                "Incompatible comparison rejected",
                "Single evaluation reopened",
                "Comparison reopened",
            ]
            assert labels.count("Next changed case") == max(
                0, len(result["changes"]) - 1
            )
            if study in source:
                compare_recorded(result, source[study])
            else:
                assert result["counts"] == dict(
                    both_passed=1, both_failed=0, improved=0, regressed=0, unverified=1
                )
                (change,) = result["changes"]
                assert (change["baseline_index"], change["candidate_index"]) == (0, 1)
                assert change["candidate_reasons"] == [
                    "Missing complete physics result"
                ]
            print(
                name,
                result["robot"],
                result["baseline"]["successes"],
                result["candidate"]["successes"],
                result["counts"],
            )
    assert len(binaries) == 1


if __name__ == "__main__":
    main()
