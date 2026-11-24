from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from uuid import uuid4

from fleet_client import submission
from fleet_worker import dispatch, initialize, transaction


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
