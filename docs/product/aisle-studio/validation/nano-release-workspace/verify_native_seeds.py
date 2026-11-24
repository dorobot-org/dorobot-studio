"""Check seed, export metadata, physical verdict and motor bytes for native reruns."""

import hashlib
import json
from pathlib import Path
from zipfile import ZipFile


def audit(path):
    with ZipFile(path) as archive:

        def read(name):
            return json.loads(archive.read(name))

        rows = []
        for author in ("codex", "octos-deepseek"):

            def item(file):
                return read(f"{author}/{file}")

            plan = item("controller.json")
            result = item("result.json")
            assert plan["provider"] == author and result["task_success"]
            seed = item("layout-seed.json")
            assert seed == result["seed"] == item("source.json")["seed"]
            assert seed == item("dataset.json")["simulation_source"]["seed"]
            assert any(
                x["check"] == "Out-of-range layout rejected"
                for x in item("journey.json")
            )
            assert any(x.get("outcome") for x in item("recovery.json"))
            assert (
                hashlib.sha256(archive.read(f"{author}/controller.json")).hexdigest()
                == result["controller_hash"]
            )
            motor = archive.read(f"{author}/motor-targets.f64")
            assert len(motor) == (result["physics_ticks"] + 1) * 17 * 8
            motor_hash = hashlib.sha256(motor).hexdigest()
            assert motor_hash == result["actuator_audit"]["sha256"]
            assert result["wheel_supported_ticks"] == result["physics_ticks"]
            rows.append(
                dict(
                    provider=author,
                    seed=seed,
                    cubes=result["requested"],
                    ticks=result["physics_ticks"],
                    motor_sha256=motor_hash,
                )
            )
        fleet = read("fleet.json")
        assert fleet["lease"]["state"] == "completed"
        assert fleet["submission"]["seed"] == fleet["result"]["seed"] == rows[0]["seed"]
        assert fleet["result"]["task_success"]
        assert fleet["result"]["actuator_audit"]["sha256"] == rows[0]["motor_sha256"]
        return rows


if __name__ == "__main__":
    print(json.dumps(audit(Path(__file__).with_name("native-seeds.zip")), indent=2))
