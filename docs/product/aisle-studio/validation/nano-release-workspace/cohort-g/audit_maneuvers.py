"""Check the G development gate, placement commands and separation from cohort F."""

from collections import Counter
from hashlib import sha256
import json
from pathlib import Path
import subprocess
import sys
from tempfile import TemporaryDirectory
from zipfile import ZipFile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[5]
sys.path.insert(0, str(ROOT / "tools/aisle-validation"))
from nano_suite_evidence import verify


def development_gate(revision):
    results = {}
    for name in ("landmark-development", "landmark-controls"):
        path = (HERE.parent / name).relative_to(ROOT)

        def recorded(filename):
            return subprocess.check_output(
                ["git", "-C", str(ROOT), "show", f"{revision}:{path}/{filename}"]
            )

        manifest = sha256(recorded("suite.json")).hexdigest()
        receipt = recorded("evidence.zip.sha256").decode().strip()
        with TemporaryDirectory() as temp:
            archive = Path(temp) / "evidence.zip"
            archive.write_bytes(recorded("evidence.zip"))
            replay = verify(archive, manifest, expected_archive=receipt)
        results[name] = {
            k: replay[k]
            for k in ("cases", "successes", "verified", "verified_guard_bypasses")
        }
    disclosed = results["landmark-development"]
    controls = results["landmark-controls"]
    if not (
        disclosed["cases"] == disclosed["verified"] == 5
        and disclosed["successes"] >= 4
        and controls["cases"] == controls["verified"] == controls["successes"] == 4
        and disclosed["verified_guard_bypasses"]
        == controls["verified_guard_bypasses"]
        == 0
    ):
        raise ValueError(
            "The committed predispatch development results fail the G protocol"
        )
    return results


def summarize(cohort):
    counts = Counter()
    successes = Counter()
    peak_lateral = 0.0
    gaps = collisions = 0
    with ZipFile(cohort / "evidence.zip") as z:
        rows = json.loads(z.read("report.json"))["rows"]
        seeds = {r["seed"] for r in rows}
        if len(rows) != 50 or len(seeds) != 50:
            raise ValueError("Expected 50 distinct, completed layout records")
        for row in rows:
            prefix = f"case-{row['index']:03d}/"
            result = json.loads(z.read(prefix + "result.json"))
            gaps += int(result["wheel_supported_ticks"] < result["physics_ticks"])
            collisions += int(bool(result["robot_furniture_contacts"]))
            maneuvers = set()
            travel = Counter()
            color = None
            for line in z.read(prefix + "actions.jsonl").splitlines():
                action = json.loads(line)
                if action["op"] == "phase":
                    color = action["color"]
                if "lateral_goal_m" in action:
                    maneuvers.add("sidestep")
                if "placement_yaw_goal_rad" in action:
                    maneuvers.add("turn")
                if action.get("lateral_m_s"):
                    if color is None:
                        raise ValueError(
                            "Lateral wheel command precedes the first task phase"
                        )
                    # Each move command advances 20 physics steps of 10 ms.
                    travel[color] += abs(action["lateral_m_s"]) * 0.2
            peak_lateral = max(peak_lateral, max(travel.values(), default=0))
            key = " and ".join(sorted(maneuvers)) or "no base reposition requested"
            counts[key] += 1
            successes[key] += int(row["passed"])
    return seeds, dict(
        maneuvers={
            k: dict(layouts=counts[k], successes=successes[k]) for k in sorted(counts)
        },
        max_commanded_lateral_path_per_cube_m=peak_lateral,
        layouts_with_wheel_support_gap=gaps,
        layouts_with_robot_furniture_contact=collisions,
    )


def main():
    cohort_f, f_summary = summarize(HERE.parent / "cohort-f")
    cohort_g, g_summary = summarize(HERE)
    if cohort_f & cohort_g:
        raise ValueError("The concealed F and G layout sets overlap")
    revision = json.loads((HERE / "provenance.json").read_text())["runtime_revision"]
    output = dict(
        development_gate=development_gate(revision),
        shared_seeds=0,
        cohort_f=f_summary,
        cohort_g=g_summary,
    )
    if "--check" in sys.argv and output != json.loads(
        (HERE / "maneuvers.json").read_text()
    ):
        raise ValueError(
            "Computed maneuver statistics differ from the published result"
        )
    print(json.dumps(output, indent=2))


if __name__ == "__main__":
    main()
