"""Reproduce the pre- and post-restart controller and displacement checks."""

import hashlib
import json
from pathlib import Path
from zipfile import ZipFile


def main():
    with ZipFile(Path(__file__).with_name("measurements.zip")) as archive:

        def read(name):
            return json.loads(archive.read(name))

        for author in ("codex", "octos-deepseek"):
            prefix = author + "/"
            native = read(prefix + "native/mode-manifest.json")
            assert native["provider"] == author and len(native["sessions"]) == 2
            first, second = native["sessions"]
            assert [first["stage"], second["stage"]] == ["journey", "recovery"]
            assert first["pid"] != second["pid"]
            assert first["observed_unix_ns"] <= second["started_unix_ns"]
            original = read(prefix + "native/journey.json")[-1]["profiles"]
            reopened = {
                entry["robot"]: entry
                for entry in read(prefix + "native/recovery.json")
                if entry["check"] == "Measured restored controller"
            }
            (rejected,) = (
                row
                for row in read(prefix + "native/recovery.json")
                if row["check"] == "Rejected changed controller"
            )
            assert rejected["artifacts"] == {
                "trajectory.json": False,
                "result.json": False,
            }
            for robot in ("Nano", "G1"):
                for session, trial in (
                    (first, original[robot]["measured"]["trial"]),
                    (second, reopened[robot]["trial"]),
                    (second, rejected["trial"]),
                ):
                    assert trial in native["trials"]
                    started = int(trial.split("-", 1)[0])
                    assert (
                        session["started_unix_ns"]
                        <= started
                        <= session["observed_unix_ns"]
                    )
                root = prefix + robot + "/"
                selected = read(root + "selected.json")
                assert selected["revision"] == original[robot]["earlier"]["revision"]
                for revision, provider, requested in (
                    ("later", "reference", 0.10),
                    ("earlier", author, -0.06),
                ):
                    folder = root + revision + "/"
                    prepared = archive.read(folder + "prepared.json")
                    assert archive.read(folder + "controller.json") == prepared
                    digest = hashlib.sha256(prepared).hexdigest()
                    controller = json.loads(prepared)
                    result = read(folder + "result.json")
                    trajectory = read(folder + "trajectory.json")
                    assert (
                        controller["provider"] == provider
                        and controller["robot"] == robot
                    )
                    assert (
                        result["controller_hash"]
                        == digest
                        == original[robot][revision]["sha256"]
                    )
                    assert (
                        result["task_success"]
                        and result["within_limits"]
                        and result["robot"] == robot
                    )
                    assert result["model_hash"] == controller["model_hash"]
                    assert (
                        result["requested_delta"]
                        == controller["delta_rad"]
                        == requested
                    )
                    assert (
                        trajectory[0]["step"] == 1
                        and trajectory[-1]["step"] == result["steps"] == 121
                    )
                    displacement = (
                        trajectory[-1]["measured_rad"] - trajectory[0]["measured_rad"]
                    )
                    assert abs(displacement - result["measured_delta"]) < 1e-8
                    assert abs(displacement - requested) <= result["tolerance_rad"]
                    if revision == "earlier":
                        assert (
                            digest
                            == selected["sha256"]
                            == reopened[robot]["controller_sha256"]
                        )
                        assert read(folder + "provider.json")["provider"] == author
                    print(
                        f"{author} {robot} {revision} {requested:+.3f} → {displacement:+.6f} rad"
                    )
            assert (
                "controller contents changed"
                in read(prefix + "rejected-change.json")["message"].lower()
            )


if __name__ == "__main__":
    main()
