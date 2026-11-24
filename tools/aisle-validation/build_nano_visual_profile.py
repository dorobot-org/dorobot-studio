"""Build the optional Nano compact-v1 camera geometry with fast-simplification==0.2.0."""

import argparse
import hashlib
import json
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile, ZipInfo

import fast_simplification
import numpy as np

from nano_visuals import STL_TRIANGLE, visual_specs


def build(model, dest):
    entries = {}
    rows = {}
    for spec in visual_specs(model):
        path = Path(spec["path"])
        original = path.read_bytes()
        faces = np.frombuffer(original, offset=84, dtype=STL_TRIANGLE)["v"]
        row = dict(
            source_sha256=hashlib.sha256(original).hexdigest(), triangles=len(faces)
        )
        if len(faces) >= 6000 and not any(
            part in spec["link"] for part in ("finger", "cam")
        ):
            vertices, indices = np.unique(
                faces.reshape(-1, 3), axis=0, return_inverse=True
            )
            vertices, indices = fast_simplification.simplify(
                vertices.astype("float64"),
                indices.reshape(-1, 3).astype("int32"),
                target_reduction=0.80,
            )
            if not len(indices):
                raise ValueError(f"Empty reduced geometry for {path.name}")
            records = np.zeros(len(indices), dtype=STL_TRIANGLE)
            records["v"] = vertices[indices]
            data = bytes(80) + len(records).to_bytes(4, "little") + records.tobytes()
            entries[path.name] = data
            row.update(
                render_sha256=hashlib.sha256(data).hexdigest(), triangles=len(records)
            )
        rows[path.name] = row
    manifest = dict(
        profile="compact-v1",
        model_sha256=hashlib.sha256(model.read_bytes()).hexdigest(),
        meshes=rows,
    )
    entries["manifest.json"] = (
        json.dumps(manifest, sort_keys=True, indent=2) + "\n"
    ).encode()
    with ZipFile(dest, "w") as archive:
        for name, data in sorted(entries.items()):
            entry = ZipInfo(name)
            entry.create_system = 3
            entry.external_attr = 0o100644 << 16
            archive.writestr(entry, data, compress_type=ZIP_DEFLATED, compresslevel=9)


if __name__ == "__main__":
    model = Path(__file__).resolve().parents[2] / "data/adora_nano/adora_nano.urdf"
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--out", type=Path, default=model.parent / "meshes/visual-compact-v1.zip"
    )
    build(model, parser.parse_args().out)
