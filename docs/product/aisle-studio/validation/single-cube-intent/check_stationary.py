"""Check the authored modes, admitted bytes, measured outcomes and retained motor targets."""

import base64
import hashlib
import json
from pathlib import Path
from zipfile import ZipFile

ROOT = Path(__file__).resolve().parent


def main():
    with (
        ZipFile(ROOT / "stationary-fleet.zip") as z,
        ZipFile(ROOT / "paired-traces.zip") as old,
    ):

        def read(name):
            return json.loads(z.read(name))

        outcomes = []
        for tag in ("before", "after", "codex-stationary", "deepseek-moving"):
            reply = read(f"{tag}/author/reply.json")
            schema = read(f"{tag}/author/controller-request.json")["output_schema"]
            controller = read(f"{tag}/author/controller.json")
            tracks = tag in ("before", "deepseek-moving")
            assert reply["track_grasp"] is tracks
            assert controller.get("track_grasp", False) is tracks
            assert schema["properties"]["track_grasp"].get("enum") == (
                None if tracks else [False]
            )
            assert (
                controller["kind"] == "table-transfer"
                and controller["color_order"] == "red"
            )
            if tag != "deepseek-moving":
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
            outcomes.append(outcome)
        assert [row["physics_ticks"] for row in outcomes] == [4364, 6848]
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
        print(
            "Original 4,364-tick failed pickup retained. Revised 6,848-tick transfer and native restart verified."
        )


if __name__ == "__main__":
    main()
