"""Exercise the native paired-cohort controls and their persisted evidence."""

import hashlib
import json
import sys


def run(args, page, click, remote, done, grab, checks):
    before, after = args.compare_cohorts
    archives = [hashlib.sha256(p.read_bytes()).hexdigest() for p in (before, after)]
    page("evidence")
    click("aisle_evidence.evaluation_mode")

    def fill(control, path):
        click(f"aisle_evidence.{control}")
        remote(
            "key",
            "w=0",
            "c=KeyA",
            "logo=1" if sys.platform == "darwin" else "ctrl=1",
            "wait=1",
        )
        remote("text", "w=0", "t=" + str(path), "wait=1")

    if not args.recover:
        fill("archive_path", after)
        fill("baseline_path", before)
        click("aisle_evidence.compare_archives")

    def matched(state):
        result = state["evaluation"] or {}
        return (
            not state["busy"]
            and result.get("comparison")
            and [result[k]["archive_sha256"] for k in ("baseline", "candidate")]
            == archives
        )

    action = "Cohort comparison restored" if args.recover else "Cohorts compared"
    state = done(matched, action, 120)
    comparison = state["evaluation"]
    assert sum(comparison["counts"].values()) == comparison["cases"]
    assert len(comparison["changes"]) == sum(
        comparison["counts"][key] for key in ("improved", "regressed", "unverified")
    )
    checks[-1]["evaluation"] = comparison
    if not args.recover:
        grab("matched-cohorts")
        for i, row in enumerate(comparison["changes"]):
            if i:
                click("aisle_evidence.later_failure")
                done(lambda state: state["failure_index"] == i, "Next changed case")
            if row["outcome"] == "improved" and all(
                previous["outcome"] != "improved"
                for previous in comparison["changes"][:i]
            ):
                grab("candidate-gain")
        if len(comparison["changes"]) > 1:
            click("aisle_evidence.earlier_failure")
            done(
                lambda state: state["failure_index"] == len(comparison["changes"]) - 2,
                "Previous changed case",
            )
        # Switching to a single archive must clear comparison mode in the operation.
        click("aisle_evidence.verify_archive")
        done(
            lambda state: not state["busy"]
            and (state["evaluation"] or {}).get("archive_sha256") == archives[1],
            "Single evaluation reopened",
            120,
        )
        fill("baseline_path", before)
        click("aisle_evidence.compare_archives")
        assert done(matched, "Comparison reopened", 120)["evaluation"] == comparison
        reject_archive = args.reject_cohort or after
        fill("baseline_path", reject_archive)
        click("aisle_evidence.compare_archives")
        rejected = done(
            lambda state: not state["busy"]
            and any(
                word in state["status"]
                for word in (
                    "same set",
                    "same robot",
                    "separate cohort",
                    "same robot, impact",
                )
            ),
            "Incompatible comparison rejected",
            120,
        )
        checks[-1]["rejected_archive_sha256"] = hashlib.sha256(
            reject_archive.read_bytes()
        ).hexdigest()
        assert rejected["evaluation"] == comparison
    name = "recovery.json" if args.recover else "journey.json"
    (args.out / name).write_text(json.dumps(checks, indent=2))
