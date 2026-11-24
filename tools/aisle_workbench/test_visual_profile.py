import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from zipfile import ZipFile

from fleet_bridge import verify_visual_profile
from visual_profile import BUNDLE, BUNDLE_SHA256, COMPACT, available, validate_profile


class VisualProfileTests(unittest.TestCase):
    def test_bundle_covers_real_urdf_and_preserves_fingers_and_cameras(self):
        import xml.etree.ElementTree as ET

        self.assertTrue(available())
        model = BUNDLE.parents[1] / "adora_nano.urdf"
        with ZipFile(BUNDLE) as archive:
            manifest = json.loads(archive.read("manifest.json"))
            self.assertEqual(
                hashlib.sha256(model.read_bytes()).hexdigest(), manifest["model_sha256"]
            )
            links = ET.parse(model).findall("link")
            triangles = 0
            meshes = set()
            for link in links:
                for mesh in link.findall("visual/geometry/mesh"):
                    path = model.parent / mesh.get("filename")
                    meshes.add(path.name)
                    row = manifest["meshes"][path.name]
                    original = path.read_bytes()
                    self.assertEqual(
                        hashlib.sha256(original).hexdigest(), row["source_sha256"]
                    )
                    count = int.from_bytes(original[80:84], "little")
                    protected = count < 6000 or any(
                        s in link.get("name") for s in ("finger", "cam")
                    )
                    self.assertEqual("render_sha256" not in row, protected)
                    data = original if protected else archive.read(path.name)
                    self.assertEqual(
                        hashlib.sha256(data).hexdigest(),
                        row.get("render_sha256", row["source_sha256"]),
                    )
                    self.assertGreater(row["triangles"], 0)
                    self.assertEqual(len(data), 84 + 50 * row["triangles"])
                    triangles += row["triangles"]
            self.assertEqual(set(manifest["meshes"]), meshes)
            self.assertEqual(triangles, 329485)

    def test_only_unbound_nano_transfers_accept_compact(self):
        plan = dict(robot="Nano", kind="table-transfer")
        validate_profile(COMPACT, plan)
        for changes in (
            dict(robot="G1"),
            dict(kind="joint-inspection"),
            dict(scene_observation={}),
        ):
            with self.assertRaises(ValueError):
                validate_profile(COMPACT, plan | changes)
            validate_profile("original", plan | changes)

    def test_imported_provenance_matches_submission(self):
        with tempfile.TemporaryDirectory() as temp:
            folder = Path(temp)
            request = dict(visual_profile=COMPACT)
            source = folder / "source.json"
            result = folder / "result.json"
            source.write_text(
                json.dumps(request | dict(visual_bundle_sha256=BUNDLE_SHA256))
            )
            result.write_text(json.dumps(request))
            verify_visual_profile(folder, request, "completed")
            for file, value in ((source, request), (result, {})):
                original = file.read_bytes()
                file.write_text(json.dumps(value))
                with self.assertRaises(ValueError):
                    verify_visual_profile(folder, request, "completed")
                file.write_bytes(original)
