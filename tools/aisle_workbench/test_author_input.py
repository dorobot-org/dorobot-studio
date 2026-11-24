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
