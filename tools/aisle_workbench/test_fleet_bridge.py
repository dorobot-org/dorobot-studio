"""Studio-level admission across nodes and recovery after an uncertain submission."""

import base64
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
from types import SimpleNamespace
import tempfile
import json
import os
import sys
import unittest
from unittest.mock import patch
from uuid import uuid4

from fleet_bridge import (
    foreground_packets,
    operate,
    journal,
    reconcile,
    record,
    submit,
    update,
    watch_foreground,
    verify_wrist_fixture,
)


class WorkspaceAdmissionTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.nodes = {"a": "a", "b": "b"}
        self.rows = {}

    def test_changed_selected_controller_is_rejected_before_lease(self):
        path = self.root / "controller.json"
        path.write_text('{"robot":"Nano","model_hash":"test"}')
        args = SimpleNamespace(
            out=self.root / "result",
            node="a",
            job_id=None,
            fleet_action="stream",
            controller=path,
            controller_sha256="0" * 64,
            seed=0,
            fixture="standard",
            room_revision=None,
        )
        with (
            patch("fleet_bridge.nodes", return_value=self.nodes),
            patch("fleet_bridge.submit") as send,
        ):
            with self.assertRaisesRegex(ValueError, "contents changed"):
                operate(args)
            send.assert_not_called()

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

    def test_import_checks_actual_wrist_fixture_against_the_lease(self):
        request = dict(
            controller=base64.b64encode(b'{"kind":"table-transfer"}').decode(),
            rgb_timing="intermittent",
        )
        (self.root / "source.json").write_text('{"wrist_rgb_timing":"intermittent"}')
        out = self.root / "result.json"
        out.write_text('{"wrist_timing":{"fixture":"normal"}}')
        with self.assertRaisesRegex(ValueError, "result.json does not confirm"):
            verify_wrist_fixture(self.root, request, "completed")
        out.write_text('{"wrist_timing":"intermittent"}')
        with self.assertRaisesRegex(ValueError, "result.json does not confirm"):
            verify_wrist_fixture(self.root, request, "completed")
        out.write_text('{"wrist_timing":{"fixture":"intermittent"}}')
        verify_wrist_fixture(self.root, request, "completed")
        (self.root / "source.json").unlink()
        with self.assertRaisesRegex(ValueError, "source.json does not confirm"):
            verify_wrist_fixture(self.root, request, "completed")
        verify_wrist_fixture(self.root, request, "cancelled")
        plan = {"kind": "table-transfer", "wrist_clearance": True}
        request.update(
            controller=base64.b64encode(json.dumps(plan).encode()).decode(),
            rgb_timing="normal",
        )
        (self.root / "source.json").write_text(
            '{"wrist_rgb_timing":"normal","wrist_clearance":true}'
        )
        out.write_text('{"wrist_timing":{"fixture":"normal"}}')
        with self.assertRaisesRegex(ValueError, "experimental wrist clearance"):
            verify_wrist_fixture(self.root, request, "completed")
        out.write_text('{"wrist_timing":{"fixture":"normal"},"wrist_clearance":true}')
        verify_wrist_fixture(self.root, request, "completed")

    def test_stalled_partial_rgb_plane_times_out(self):
        reader, writer = os.pipe()
        with os.fdopen(reader, "rb") as incoming, os.fdopen(writer, "wb") as outgoing:
            outgoing.write(
                json.dumps({"bytes": 4 + 480 * 320 * 3}).encode() + b"\npartial"
            )
            outgoing.flush()
            with self.assertRaisesRegex(TimeoutError, "stalled"):
                next(foreground_packets(incoming, silence_s=0.05))

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
