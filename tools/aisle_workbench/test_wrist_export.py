import json
from pathlib import Path
import tempfile
import unittest

from replay import wrist_holds


class WristExportTests(unittest.TestCase):
    def test_maps_capture_and_receive_times_to_the_recording_clock(self):
        samples = [{"step": step} for step in (20, 40, 60, 80, 100)]
        deliveries = [
            dict(tick=28, capture_tick=28, fault=None),
            dict(tick=38, capture_tick=28, fault="repeated"),
            dict(tick=58, capture_tick=28, fault="repeated"),
            dict(tick=73, capture_tick=73, fault=None),
        ]
        with tempfile.TemporaryDirectory() as name:
            path = Path(name)
            raw = "".join(json.dumps(r) + "\n" for r in deliveries).encode()
            (path / "wrist-delivery.jsonl").write_bytes(raw)
            timing = dict(received=4, rejected={"repeated": 2})
            self.assertEqual(wrist_holds(path, timing, samples), (raw, [0, 1, 1, 0, 0]))
            with self.assertRaisesRegex(ValueError, "differs"):
                wrist_holds(
                    path, dict(received=5, rejected=timing["rejected"]), samples
                )

    def test_early_stop_before_first_pick_has_no_wrist_hold(self):
        with tempfile.TemporaryDirectory() as name:
            timing = dict(received=0, rejected={})
            self.assertEqual(
                wrist_holds(Path(name), timing, [{"step": 20}]), (b"", [0])
            )


if __name__ == "__main__":
    unittest.main()
