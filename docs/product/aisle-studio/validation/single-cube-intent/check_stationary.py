"""Check the authored modes, admitted bytes, measured outcomes and retained motor targets."""

import argparse
import base64
import hashlib
import json
import runpy
from pathlib import Path
from zipfile import ZipFile

ROOT = Path(__file__).resolve().parent


def main():
    if not __debug__:
        raise RuntimeError("Run the evidence checks with assertions enabled")
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument(
        "--datasets",
        action="store_true",
        help="Decode all camera frames and synchronized Parquet rows",
    )
    args = cli.parse_args()
    with (
        ZipFile(ROOT / "stationary-fleet.zip") as z,
        ZipFile(ROOT / "paired-traces.zip") as old,
    ):

        def read(name):
            return json.loads(z.read(name))

        outcomes = []
        for tag in (
            "before",
            "after",
            "codex-stationary",
            "deepseek-moving",
            "deepseek-sliding",
        ):
            reply = read(f"{tag}/author/reply.json")
            schema = read(f"{tag}/author/controller-request.json")["output_schema"]
            controller = read(f"{tag}/author/controller.json")
            tracks = tag in ("before", "deepseek-moving", "deepseek-sliding")
            assert reply["track_grasp"] is tracks
            assert controller.get("track_grasp", False) is tracks
            assert schema["properties"]["track_grasp"].get("enum") == (
                ([True] if tag == "deepseek-sliding" else None) if tracks else [False]
            )
            assert (
                controller["kind"] == "table-transfer"
                and controller["color_order"] == "red"
            )
            if tag == "deepseek-moving":
                assert (
                    controller["task"]
                    == "Track the moving red cube from onboard RGB while the arm descends, pick it up and put it on the coffee table."
                )
            elif tag == "deepseek-sliding":
                assert (
                    controller["task"]
                    == "Move the sliding red floor cube onto the coffee table."
                )
                assert schema["properties"]["color_order"]["enum"] == ["red"]
            else:
                assert (
                    controller["task"]
                    == "Move the red floor cube onto the coffee table."
                )
            if tag not in ("before", "after"):
                continue
            raw = z.read(f"{tag}/trial/controller.json")
            lease = read(f"{tag}/lease.json")
            outcome = read(f"{tag}/trial/result.json")
            source = read(f"{tag}/trial/source.json")
            assert (
                raw
                == base64.b64decode(lease["controller"], validate=True)
                == z.read(f"{tag}/author/controller.json")
            )
            assert (
                hashlib.sha256(raw).hexdigest()
                == lease["controller_sha256"]
                == source["controller_sha256"]
                == outcome["controller_hash"]
            )
            assert (
                source["seed"] == outcome["seed"] == 0
                and lease["target_motion"] == "stationary"
            )
            assert outcome["model_hash"] == controller["model_hash"] and outcome[
                "requested"
            ] == ["red"]
            motor = z.read(f"{tag}/trial/motor-targets.f64")
            assert (
                hashlib.sha256(motor).hexdigest() == outcome["actuator_audit"]["sha256"]
            )
            assert (
                outcome["physics_ticks"]
                == outcome["wheel_supported_ticks"]
                == outcome["actuator_audit"]["applied_rows"]
            )
            assert (
                outcome["actuator_audit"]["passed"] and not outcome["root_pose_writes"]
            )
            assert outcome["task_success"] is (tag == "after")
            if tag == "before":
                assert "after two grasps" in outcome["error"]
            else:
                assert outcome["cubes"]["red"]["table_contact_tail_ticks"] == 100
                assert motor == old.read("octos-deepseek/trial/motor-targets.f64")
            assert not outcome["unsupported_wheel_ranges"]
            outcomes.append(outcome)
        assert [row["physics_ticks"] for row in outcomes] == [4364, 6848]
        assert [r["frames"] for r in outcomes] == [218, 342]
        assert [
            r["cubes"]["red"]["opposing_finger_airborne_ticks"] for r in outcomes
        ] == [0, 3182]
        assert [r["cubes"]["red"]["table_contact_tail_ticks"] for r in outcomes] == [
            0,
            100,
        ]
        assert outcomes[0]["model_hash"] == outcomes[1]["model_hash"]
        native = read("after/native/journey.json")[-1]
        assert (
            native["result"] == outcomes[1]
            and native["foreground_id"] == read("after/lease.json")["id"]
        )
        assert (
            read("after/native/recovery.json")[-1]["check"]
            == "Imported worker trial and export recovered"
        )
        previous = read("revised-parent/feedback.json")
        revision = read("revised-parent/controller.json")
        assert previous["controller_hash"] == outcomes[0]["controller_hash"]
        assert (
            previous["previous_track_grasp"]
            and previous["stopped_pickup"]["stopped_tick"] == 4364
        )
        assert revision["previous_trial"] == previous and revision["track_grasp"]
        assert (
            "enum"
            not in read("revised-parent/schema.json")["properties"]["track_grasp"]
        )
        with ZipFile(ROOT / "sliding-trial.zip") as slide:
            assert slide.read("controller.json") == z.read(
                "deepseek-sliding/author/controller.json"
            )
            moved = json.loads(slide.read("result.json"))
            assert (
                moved["controller_hash"]
                == hashlib.sha256(slide.read("controller.json")).hexdigest()
            )
            assert (
                moved["task_success"]
                and moved["wheel_supported_ticks"] == moved["physics_ticks"] == 6692
            )
        live = read("after/native/reopened.json")
        reopened_dataset = Path(live["dataset"])
        export = read(f"after/{reopened_dataset.parent.name}/export.json")
        assert reopened_dataset.name == export["dataset"]
        assert (
            Path(live["selected"]).name
            == export["trial"]
            == read("after/dataset/meta/aisle.json")["trial"]
        )
        assert live["result"] == outcomes[1]
        assert live["fleet"]["selected"]["id"] == native["foreground_id"]
        assert any(
            row["id"] == native["background_id"] and row["state"] == "cancelled"
            for row in read("after/native/leases.json")
        )
        exposure = dict(read("after/trial/preview-timing.json")["tick_time"])
        draws = [
            json.loads(row)
            for row in z.read("after/native/journey-draws.jsonl").splitlines()
        ]
        assert len({r["tick"] for r in draws if r["tick"] in exposure}) >= 2
        manifest = read("after/native/mode-manifest.json")
        assert manifest["sessions"][0]["pid"] != manifest["sessions"][1]["pid"]
        assert (
            manifest["studio_source_commit"]
            == "cda37d2c5976b4810ff6847072f26d879e272957"
        )
        data = read("after/dataset/meta/info.json")
        assert data["total_frames"] == 342 and data["total_episodes"] == 1
        if args.datasets:
            verify = runpy.run_path(str(ROOT.parent / "eight-h100/audit.py"))[
                "check_dataset"
            ]
            verify(
                z.read,
                "after/dataset/",
                data["features"],
                data["fps"],
                data["total_frames"],
            )
        print(
            "Original 4,364-tick failed pickup retained. Revised 6,848-tick transfer and native restart verified."
        )


if __name__ == "__main__":
    main()
