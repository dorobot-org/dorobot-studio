"""Studio-level admission across nodes and recovery after an uncertain submission."""

from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
import tempfile
import json
import sys
import unittest
from unittest.mock import patch
from uuid import uuid4

from fleet_bridge import journal, reconcile, record, submit, update, watch_foreground


class WorkspaceAdmissionTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.nodes = {"a": "a", "b": "b"}
        self.rows = {}

    def transport(self, node, request):
        key = (node, request["id"])
        if request["op"] == "status":
            if key not in self.rows:
                raise ValueError("Unknown simulation lease")
        else:
            self.rows.setdefault(key, dict(id=key[1], state="running"))
        return self.rows[key]

    def request(self, mode="stream"):
        return dict(op="submit", id=str(uuid4()), robot="Nano", mode=mode)

    def test_a_truncated_rgb_stream_never_replaces_the_last_complete_frame(self):
        ident = str(uuid4())
        packet = dict(job=dict(id=ident, state="running"), bytes=4 + 480 * 320 * 3)
        path = self.root / "preview.rgb"
        path.write_bytes(b"last complete frame")
        data = json.dumps(packet).encode() + b"\nshort"
        cmd = [sys.executable, "-c", f"import sys; sys.stdout.buffer.write({data!r})"]
        with patch("fleet_bridge.worker_command", return_value=cmd):
            with self.assertRaisesRegex(ValueError, "Incomplete"):
                watch_foreground({}, ident, self.root, self.root)
        self.assertEqual(path.read_bytes(), b"last complete frame")

    def test_one_foreground_across_two_simultaneous_node_connections(self):
        def start(alias):
            try:
                return submit(self.root, self.nodes, alias, self.request())
            except ValueError as exc:
                self.assertIn("foreground stream", str(exc))

        with patch("fleet_bridge.send", self.transport), ThreadPoolExecutor(2) as pool:
            results = list(pool.map(start, self.nodes))
            self.assertEqual(sum(bool(r) for r in results), 1)
            row = submit(self.root, self.nodes, "b", self.request("observations"))
            self.assertEqual(row["state"], "running")
            for state in self.rows.values():
                state["state"] = "completed"
            self.assertTrue(start("b"))
            with journal(self.root) as db:
                self.assertEqual(
                    db.execute("SELECT COUNT(*) FROM jobs").fetchone()[0], 3
                )

    def test_dropped_reply_recovers_same_id_and_keeps_foreground_reserved(self):
        request = self.request()

        def dropped(node, obj):
            self.transport(node, obj)
            raise TimeoutError("response lost")

        with patch("fleet_bridge.send", dropped), self.assertRaises(TimeoutError):
            submit(self.root, self.nodes, "a", request)
        with patch("fleet_bridge.send", self.transport):
            with self.assertRaisesRegex(ValueError, "foreground stream"):
                submit(self.root, self.nodes, "b", self.request())
            row = record(self.root, request["id"])
            self.assertEqual(reconcile(self.root, row, "a")["id"], request["id"])
            self.assertEqual(len(self.rows), 1)
            update(self.root, request["id"], "completed")
            update(self.root, request["id"], "running")
            self.assertEqual(record(self.root, request["id"])["state"], "completed")
