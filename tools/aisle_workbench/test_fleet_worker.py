import base64
import io
import os
import zlib
import json
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from uuid import uuid4

from fleet_client import decode_rgb, poll_observation, submission
from fleet_worker import dispatch, initialize, stream_rgb, transaction


class WorkerLeaseTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        initialize(self.root, 2)
        controller = (
            Path(__file__).resolve().parents[2]
            / "docs/product/aisle-studio/validation/nano-holdout-c/controller.json"
        )
        self.request = submission(controller)

    def test_slot_claim_and_repeat_are_atomic(self):
        def submit(index):
            try:
                return dispatch(self.root, self.request | {"id": str(uuid4())})
            except ValueError as exc:
                self.assertIn("slots", str(exc))
                return None

        with (
            patch("fleet_worker.subprocess.Popen") as spawn,
            ThreadPoolExecutor(6) as pool,
        ):
            rows = [r for r in pool.map(submit, range(8)) if r]
            self.assertEqual(len(rows), 2)
            self.assertEqual(spawn.call_count, 2)
            req = self.request | {"id": rows[0]["id"]}
            self.assertEqual(dispatch(self.root, req)["state"], "reserved")
            with self.assertRaisesRegex(ValueError, "different request"):
                dispatch(self.root, req | {"seed": 2})
            self.assertEqual(spawn.call_count, 2)

    def test_foreground_quota_and_expired_cancel(self):
        req = self.request | {"mode": "stream"}
        with patch("fleet_worker.subprocess.Popen"):
            dispatch(self.root, req)
            with self.assertRaisesRegex(ValueError, "foreground"):
                dispatch(self.root, req | {"id": str(uuid4())})
            self.assertEqual(
                dispatch(self.root, {"id": req["id"], "op": "cancel"})["state"],
                "stopping",
            )
            with transaction(self.root) as db:
                db.execute("UPDATE jobs SET deadline=0")
            self.assertEqual(
                dispatch(self.root, {"id": req["id"], "op": "status"})["state"], "lost"
            )
            self.assertEqual(dispatch(self.root, req)["state"], "lost")
            self.assertEqual(
                dispatch(self.root, req | {"id": str(uuid4())})["state"], "reserved"
            )

    def test_lease_retry_preserves_legacy_clock_and_rejects_changed_clock(self):
        from fleet_client import check_background_period

        with patch(
            "fleet_client.send", return_value={"background_rgb_periods_s": [1, 2, 5]}
        ):
            check_background_period({}, 1)
            with self.assertRaisesRegex(ValueError, "wall-clock"):
                check_background_period({}, 1, "wall")
        request = self.request | {"mode": "observations"}
        with patch("fleet_worker.subprocess.Popen"):
            ident = dispatch(self.root, request)["id"]
            self.assertEqual(
                dispatch(self.root, request | {"observation_clock": "simulation"})[
                    "id"
                ],
                ident,
            )
            with self.assertRaisesRegex(ValueError, "different request"):
                dispatch(self.root, request | {"observation_clock": "wall"})
        for bad in ("device", None, True):
            with self.assertRaisesRegex(ValueError, "clock"):
                dispatch(self.root, request | {"observation_clock": bad})
        with self.assertRaisesRegex(ValueError, "background"):
            dispatch(
                self.root,
                self.request | {"mode": "stream", "observation_clock": "wall"},
            )

    def test_half_second_schedule_requires_advertised_wall_time(self):
        from fleet_client import check_background_period
        from worker import observation_seconds

        self.assertIs(type(observation_seconds("2.0")), int)
        self.assertEqual(observation_seconds("0.5"), 0.5)
        caps = {
            "background_rgb_periods_s": [1, 2, 5],
            "background_rgb_clocks": ["simulation", "wall"],
        }
        with patch("fleet_client.send", return_value=caps):
            with self.assertRaisesRegex(ValueError, "period"):
                check_background_period({}, 0.5, "wall")
        with patch(
            "fleet_client.send",
            return_value=caps | {"background_wall_rgb_periods_s": [0.5, 1, 2, 5]},
        ):
            check_background_period({}, 0.5, "wall")
        req = self.request | {"mode": "observations", "observation_period_s": 0.5}
        with self.assertRaisesRegex(ValueError, "period"):
            dispatch(self.root, req)
        with patch("fleet_worker.subprocess.Popen"):
            self.assertEqual(
                dispatch(self.root, req | {"observation_clock": "wall"})["state"],
                "reserved",
            )

    def test_background_clock_and_stream_are_distinct(self):
        for invalid in (True, 0, 3, 10):
            with self.assertRaisesRegex(ValueError, "observation period"):
                dispatch(
                    self.root,
                    self.request
                    | {"mode": "observations", "observation_period_s": invalid},
                )
        with patch("fleet_worker.subprocess.Popen"):
            ident = dispatch(
                self.root,
                self.request | {"mode": "observations", "observation_period_s": 2},
            )["id"]
            self.assertFalse(
                dispatch(self.root, {"op": "observation", "id": ident})["ready"]
            )
            with self.assertRaisesRegex(ValueError, "foreground"):
                dispatch(self.root, {"op": "frame", "id": ident})
            with self.assertRaisesRegex(ValueError, "different request"):
                dispatch(
                    self.root,
                    self.request
                    | {"id": ident, "mode": "observations", "observation_period_s": 5},
                )

    def test_background_poll_combines_status_and_lossless_rgb_including_terminal_frame(
        self,
    ):
        ident = str(uuid4())
        trial = self.root / ident / "trial"
        trial.mkdir(parents=True)
        path = trial / "preview.rgb"
        job = dict(id=ident, stream=False, state="completed")
        with patch("fleet_worker.status", return_value=job):

            def rpc(_, request):
                return dispatch(self.root, request)

            _, empty = poll_observation(None, ident, rpc=rpc)
            self.assertFalse(empty["ready"])
            for pixels, encoding in [
                (bytes(480 * 360 * 3), "zlib"),
                (os.urandom(480 * 320 * 3), "raw"),
            ]:
                path.write_bytes((17).to_bytes(4, "big") + pixels)
                state, frame = poll_observation(None, ident, rpc=rpc)
                self.assertEqual(state, job)
                self.assertEqual(frame.get("encoding", "raw"), encoding)
                self.assertEqual(decode_rgb(frame), pixels)
                _, duplicate = poll_observation(None, ident, 17, rpc=rpc)
                self.assertFalse(duplicate["ready"])
                self.assertNotIn("data", duplicate)
                raw = dispatch(self.root, dict(op="observation", id=ident))
                self.assertEqual(base64.b64decode(raw["data"], validate=True), pixels)
                self.assertNotIn("job", raw)

    def test_older_worker_observation_and_invalid_compressed_frames(self):
        from unittest.mock import Mock

        ident = str(uuid4())
        job = dict(id=ident, state="running")
        rpc = Mock(side_effect=[dict(ready=False), job])
        self.assertEqual(
            poll_observation(None, ident, 5, rpc=rpc), (job, dict(ready=False))
        )
        self.assertEqual(rpc.call_args.args[1], dict(op="status", id=ident))
        with self.assertRaisesRegex(ValueError, "different job"):
            poll_observation(
                None, ident, rpc=Mock(return_value=dict(job={"id": "other"}))
            )
        raw = bytes(480 * 360 * 3)
        for bad in (
            zlib.compress(raw)[:-1],
            zlib.compress(raw) + b"x",
            zlib.compress(raw + b"x"),
        ):
            with self.assertRaises(ValueError):
                decode_rgb(
                    dict(
                        width=480,
                        height=360,
                        encoding="zlib",
                        data=base64.b64encode(bad).decode(),
                    )
                )

    def test_rgb_stream_finishes_with_the_last_full_frame_and_rejects_backgrounds(self):
        ident = self.request["id"]
        trial = self.root / ident / "trial"
        trial.mkdir(parents=True)
        raw = (40).to_bytes(4, "big") + bytes(480 * 360 * 3)
        (trial / "preview.rgb").write_bytes(raw)
        final = dict(id=ident, stream=True, state="completed")
        out = io.BytesIO()
        with patch("fleet_worker.status", return_value=final):
            stream_rgb(self.root, ident, out)
        out.seek(0)
        self.assertEqual(json.loads(out.readline()), dict(job=final, bytes=len(raw)))
        self.assertEqual(out.read(), raw)
        with patch("fleet_worker.status", return_value=final | {"stream": False}):
            with self.assertRaisesRegex(ValueError, "foreground"):
                stream_rgb(self.root, ident, io.BytesIO())
        (trial / "preview.rgb").write_bytes(raw[:15])
        with patch("fleet_worker.status", return_value=final):
            with self.assertRaisesRegex(ValueError, "RGB plane"):
                stream_rgb(self.root, ident, io.BytesIO())

    def test_unknown_companion_file_and_controller_digest_are_rejected(self):
        for change in [
            {"files": {"../unknown": "AA=="}},
            {"controller_sha256": "0" * 64},
        ]:
            with self.assertRaises(ValueError):
                dispatch(self.root, self.request | change)


class FleetArtifactTests(unittest.TestCase):
    def test_download_validates_chunks_before_publishing_the_archive(self):
        import random
        from fleet_artifacts import download, metadata, piece

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "trial").mkdir()
            (root / "trial" / "evidence.bin").write_bytes(
                random.Random(7).randbytes(2_500_000)
            )

            def transport(node, request):
                return (
                    metadata(root)
                    if request["op"] == "archive"
                    else piece(root, request)
                )

            path = root / "copy.zip"
            receipt = download(transport, {}, "test", path)
            self.assertGreater(receipt["bytes"], 2 * 1024 * 1024)
            self.assertEqual(path.read_bytes(), (root / "trial.zip").read_bytes())
            with self.assertRaisesRegex(ValueError, "new output"):
                download(transport, {}, "test", path)

            def truncated(node, request):
                if request["op"] == "read-archive":
                    return dict(offset=request["offset"], data="")
                return metadata(root)

            with self.assertRaisesRegex(ValueError, "Incomplete"):
                download(truncated, {}, "test", root / "partial.zip")
            self.assertFalse((root / "partial.zip").exists())
