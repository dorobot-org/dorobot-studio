"""Compare independently scored Nano layouts or G1 impact recipes for native Evidence."""

import hashlib
import json

from g1_reach_suite import verify as verify_g1
from nano_suite_evidence import compare_archives


def manifest(path):
    if not path.is_file():
        raise ValueError(f"Cannot open cohort archive {path}")
    sidecar = path.with_name("suite.json")
    if not sidecar.is_file():
        raise ValueError(f"Place the public suite.json beside {path}")
    public = sidecar.read_bytes()
    return json.loads(public), hashlib.sha256(public).hexdigest()


def compare_g1(baseline, candidate, manifests):
    (left, left_hash), (right, right_hash) = manifests
    for spec in (left, right):
        if (
            not isinstance(spec.get("criteria"), dict)
            or not isinstance(spec.get("inputs"), dict)
            or not {"urdf", "meshes"} <= spec["inputs"].keys()
        ):
            raise ValueError(
                "G1 comparison needs the robot, meshes and scoring criteria in each manifest"
            )
    if (
        left.get("impact_aim", "pelvis-v1") != right.get("impact_aim", "pelvis-v1")
        or left["criteria"] != right["criteria"]
        or left["inputs"]["urdf"] != right["inputs"]["urdf"]
        or left["inputs"]["meshes"] != right["inputs"]["meshes"]
    ):
        raise ValueError(
            "Choose G1 suites with the same robot, impact fixture and scoring criteria"
        )
    before = verify_g1(baseline, left_hash, include_rows=True)
    after = verify_g1(candidate, right_hash, include_rows=True)

    # The salt commits a case before disclosure. Its physical identity is the recipe.
    def by_recipe(summary):
        rows = summary.pop("rows")
        by_key = {tuple(sorted(r["recipe"].items())): r for r in rows}
        if len(by_key) != len(rows):
            raise ValueError("A G1 suite contains duplicate impact recipes")
        return by_key

    old, new = by_recipe(before), by_recipe(after)
    if old.keys() != new.keys():
        raise ValueError("Each G1 archive must contain the same set of impact recipes")
    pairs = []
    for key, a in old.items():
        b = new[key]
        pairs.append(
            dict(
                recipe=a["recipe"],
                baseline_index=a["index"],
                candidate_index=b["index"],
                both_verified=a["verified"] and b["verified"],
                baseline_passed=a["passed"],
                candidate_passed=b["passed"],
            )
        )
    before["controller_sha256"] = left["controller_sha256"]
    after["controller_sha256"] = right["controller_sha256"]
    return before, after, pairs


def compare(baseline, candidate):
    left, right = manifest(baseline), manifest(candidate)
    if baseline.samefile(candidate):
        raise ValueError("Choose two separate cohort archives")
    profile = left[0].get("profile")
    if profile != right[0].get("profile"):
        raise ValueError("Choose two cohort archives for the same robot")
    if profile == "g1-standing-reach-v1":
        before, after, pairs = compare_g1(baseline, candidate, (left, right))
        robot = "G1"
    elif profile is None:
        result = compare_archives(
            baseline, candidate, left[1], right[1], include_trials=True
        )
        before, after, pairs = result["baseline"], result["candidate"], result["pairs"]
        robot = "Nano"
    else:
        raise ValueError("Unsupported cohort profile")

    failures = [
        {r["index"]: r["reasons"] for r in summary["failures"]}
        for summary in (before, after)
    ]
    counts = dict(both_passed=0, improved=0, regressed=0, both_failed=0, unverified=0)
    changes = []
    for pair in pairs:
        a, b = pair["baseline_passed"], pair["candidate_passed"]
        if pair["both_verified"]:
            outcome = {
                (True, True): "both_passed",
                (False, True): "improved",
                (True, False): "regressed",
                (False, False): "both_failed",
            }[a, b]
        else:
            outcome = "unverified"
        counts[outcome] += 1
        if outcome in ("improved", "regressed", "unverified"):
            changes.append(
                dict(
                    **pair,
                    outcome=outcome,
                    baseline_reasons=failures[0].get(pair["baseline_index"], []),
                    candidate_reasons=failures[1].get(pair["candidate_index"], []),
                )
            )

    def identity(summary, path):
        return {
            "file": path.name,
            **{
                key: summary[key]
                for key in (
                    "archive_sha256",
                    "manifest_sha256",
                    "controller_sha256",
                    "successes",
                    "verified",
                )
            },
        }

    return dict(
        comparison=True,
        robot=robot,
        cases=len(pairs),
        counts=counts,
        changes=changes,
        baseline=identity(before, baseline),
        candidate=identity(after, candidate),
        pairs=pairs,
    )
