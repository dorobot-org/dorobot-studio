"""Verify which recorded attachments reach the actual author command."""

import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from worker import digest, request_controller


class AuthorInputTests(unittest.TestCase):
    def test_codex_receives_the_saved_images_in_camera_order(self):
        with tempfile.TemporaryDirectory() as folder:
            out = Path(folder)
            images = []
            expected = []
            for camera in ("head", "left_wrist", "right_wrist"):
                path = out / f"author-{camera}.png"
                path.write_bytes(camera.encode())
                images.append(dict(camera=camera, file=path.name, sha256=digest(path)))
                expected.append(str(path.resolve()))
            receipt = dict(images=images)

            def run(command, **kwargs):
                self.assertEqual(
                    [
                        command[i + 1]
                        for i, arg in enumerate(command)
                        if arg == "--image"
                    ],
                    expected,
                )
                self.assertEqual(kwargs["input"], "use onboard views")
                (out / "reply.json").write_text('{"supported":true}')
                return subprocess.CompletedProcess(command, 0)

            with patch("worker.subprocess.run", side_effect=run):
                request_controller(
                    "codex", "use onboard views", {}, out, camera_input=receipt
                )
            self.assertEqual(
                json.loads((out / "controller-request.json").read_bytes())[
                    "onboard_input"
                ],
                receipt,
            )
            with patch("worker.subprocess.run") as launch:
                with self.assertRaisesRegex(ValueError, "attachment"):
                    request_controller(
                        "octos-deepseek",
                        "use onboard views",
                        {},
                        out,
                        camera_input=receipt,
                    )
                (out / images[0]["file"]).write_bytes(b"changed")
                with self.assertRaisesRegex(ValueError, "attachment"):
                    request_controller(
                        "codex", "use onboard views", {}, out, camera_input=receipt
                    )
                launch.assert_not_called()

    def test_capture_material_survives_fleet_and_rejects_changed_camera_roles(self):
        import base64
        from copy import deepcopy

        import numpy as np
        from fleet_client import submission
        from onboard_scene import (
            AUTHOR_CAMERAS,
            CALIBRATION,
            PACKET,
            PREVIEW,
            SUMMARY,
            author_input,
            bind,
            observation_files,
            source_hashes,
            verify_observation_files,
        )

        with tempfile.TemporaryDirectory() as folder:
            out = Path(folder)
            np.savez_compressed(
                out / PACKET,
                **{
                    name: np.full((360, 480, 3), i, dtype=np.uint8)
                    for i, name in enumerate(AUTHOR_CAMERAS)
                },
            )
            for name in (PREVIEW, CALIBRATION, SUMMARY):
                (out / name).write_bytes(b"recorded")
            files = {
                name: digest(out / name)
                for name in (PACKET, CALIBRATION, SUMMARY, PREVIEW)
            }
            self.assertEqual(observation_files(dict(files=files)), files)
            legacy = {name: h for name, h in files.items() if name != PREVIEW}
            self.assertEqual(observation_files(dict(files=legacy)), legacy)
            for provider in ("octos-deepseek", "codex"):
                binding = dict(
                    files=dict(files),
                    source_sha256=source_hashes(),
                    layout=0,
                    room_revision_sha256=None,
                )
                binding["author_input"] = author_input(out, binding, provider)
                path = out / "controller.json"
                path.write_text(
                    json.dumps(dict(robot="Nano", scene_observation=binding))
                )
                payload = submission(path)["files"]
                trial = out / provider
                trial.mkdir()
                bind(out, trial, binding, 0, None)
                self.assertEqual(
                    set(payload), set(verify_observation_files(trial, binding))
                )
                for name, encoded in payload.items():
                    self.assertEqual(
                        (trial / name).read_bytes(), base64.b64decode(encoded)
                    )
            for bad_name in ("../escape.png", "author-right_wrist.png"):
                invalid = deepcopy(binding)
                invalid["author_input"]["images"][0]["file"] = bad_name
                with self.assertRaisesRegex(ValueError, "attachments"):
                    observation_files(invalid)
            image = trial / binding["author_input"]["images"][0]["file"]
            image.write_bytes((trial / "author-right_wrist.png").read_bytes())
            with self.assertRaisesRegex(ValueError, "changed"):
                verify_observation_files(trial, binding)
            binding["author_input"]["images"][0]["sha256"] = binding["files"][
                image.name
            ] = digest(image)
            with self.assertRaisesRegex(ValueError, "differs from onboard camera"):
                verify_observation_files(trial, binding)
