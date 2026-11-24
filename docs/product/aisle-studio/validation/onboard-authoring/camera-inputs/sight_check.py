"""Ask Codex the same image-only question for two previously captured Nano exposures."""

import argparse
import hashlib
import json
from pathlib import Path
import shutil
import sys
from tempfile import TemporaryDirectory

import numpy as np
from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parents[6] / "tools/aisle_workbench"))
from worker import request_controller

QUESTION = (
    "The three attached RGB images are the head camera, left wrist and right wrist, in that order. "
    "In the HEAD image, which floor cube has the smallest horizontal pixel coordinate "
    "(furthest left on the image)? Answer with its color. Use only the attached images."
)
SCHEMA = dict(
    type="object",
    additionalProperties=False,
    required=["color"],
    properties=dict(color=dict(type="string", enum=["red", "green", "blue"])),
)


def pixel_centres(rgb):
    values = rgb.astype(np.int16)
    centres = {}
    for i, name in enumerate(("red", "green", "blue")):
        channel = values[:, :, i]
        other = np.delete(values, i, axis=2).max(axis=2)
        y, x = np.where((channel > 85) & (channel > other * 1.5 + 20))
        # These two captured head images have one isolated small cube per color.
        if len(x) < 10 or np.ptp(x) > 30 or np.ptp(y) > 30:
            raise ValueError(f"Cannot independently isolate a single {name} cube")
        centres[name] = round(float(x.mean()), 3)
    return centres


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("preparations", nargs=2, type=Path)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=False)
    rows = []
    with TemporaryDirectory() as tmp:
        work = Path(tmp)
        for i, prepared in enumerate(args.preparations):
            images = []
            for name in ("head", "left_wrist", "right_wrist"):
                path = work / f"author-{name}.png"
                shutil.copyfile(prepared / path.name, path)
                images.append(
                    dict(
                        camera=name,
                        file=path.name,
                        sha256=hashlib.sha256(path.read_bytes()).hexdigest(),
                    )
                )
            with Image.open(work / images[0]["file"]) as im:
                centres = pixel_centres(np.asarray(im.convert("RGB")))
            expected = min(centres, key=centres.get)
            reply, model = request_controller(
                "codex", QUESTION, SCHEMA, work, camera_input={"images": images}
            )
            assert reply["color"] == expected
            rows.append(
                dict(
                    exposure=i,
                    model=model,
                    images=images,
                    head_centres_x_px=centres,
                    expected=expected,
                    reply=reply,
                )
            )
            shutil.copyfile(
                work / "controller-request.json", args.out / f"request-{i}.json"
            )
    assert rows[0]["expected"] != rows[1]["expected"]
    (args.out / "results.json").write_text(json.dumps(rows, indent=2) + "\n")


if __name__ == "__main__":
    main()
