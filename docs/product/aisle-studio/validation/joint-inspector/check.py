"""Audit native joint labels against the shipped URDFs and inspect the captured viewport pixels."""

import hashlib
import json
from pathlib import Path
import xml.etree.ElementTree as ET

import numpy as np
from PIL import Image


HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[4]
MODELS = {"Nano": "adora_nano/adora_nano.urdf", "G1": "g1/g1.urdf"}


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check_model(robot, details):
    model = ROOT / "data" / MODELS[robot]
    joints = [j for j in ET.parse(model).findall("joint") if j.get("type") != "fixed"]
    assert list(details) == [j.attrib["name"] for j in joints]
    for joint in joints:
        name, kind = joint.attrib["name"], joint.attrib["type"]
        lines = details[name].splitlines()
        assert lines[0] == kind
        expected = {
            "Parent": joint.find("parent").attrib["link"],
            "Child": joint.find("child").attrib["link"],
        }
        fields = dict(line.split(" · ", 1) for line in lines[1:] if line)
        if kind == "continuous":
            expected["Position"] = "continuous rotation"
        lim = joint.find("limit").attrib
        for key, label, unit in (
            ("lower", "Lower", "rad"),
            ("upper", "Upper", "rad"),
            ("velocity", "Speed limit", "rad/s"),
            ("effort", "Effort limit", "N·m"),
        ):
            if key in lim:
                value, displayed_unit = fields.pop(label).split()
                assert displayed_unit == unit and float(value) == float(lim[key]), name
        assert fields == expected, name
    return len(joints)


def blue_pixels(path):
    with Image.open(path) as img:
        assert img.size == (1440, 900)
        # Only the native robot viewport. Excludes the sidebar, controls and status line.
        pixels = np.asarray(img.convert("RGB").crop((236, 295, 1096, 824)), dtype=int)
    red, green, blue = np.moveaxis(pixels, 2, 0)
    return (blue - red >= 30) & (green - red >= 12) & (blue - green >= 12)


def main():
    runs = json.loads((HERE / "native.json").read_text())
    assert {(r["provider"], r["backend"], r["requested_size"]) for r in runs} == {
        ("octos-deepseek", "offscreen", "1440x900"),
        ("codex", "headless", "1024x768"),
    }
    first, second = runs
    for k in (
        "joints",
        "models",
        "sources",
        "native_binary_sha256",
        "studio_source_commit",
    ):
        assert first[k] == second[k]
    for run in runs:
        start, restart = run["sessions"]
        assert [start["stage"], restart["stage"]] == ["journey", "recovery"]
        assert start["pid"] != restart["pid"]
        assert start["observed_unix_ns"] <= restart["started_unix_ns"]
        for session in (start, restart):
            assert [int(v) for v in run["requested_size"].split("x")] in session[
                "windows"
            ]
        replay = run["replay"]
        assert replay["selected"] == replay["restored"] == replay["dataset_trial"]
        assert replay["stopped"] == replay["recovered_stop"] != replay["restored"]
        assert replay["frames"] == 25 and replay["operator_stop"] is True
        assert len(replay["controller_sha256"]) == 64
        for robot, path in MODELS.items():
            assert sha(ROOT / "data" / path) == run["models"][robot]
            count = check_model(robot, run["joints"][robot])
            print(f"{run['provider']} {robot}: {count} movable joints match the URDF")
        for filename, digest in run["screenshots"].items():
            assert sha(HERE / filename) == digest
    for robot in MODELS:
        selected = blue_pixels(HERE / f"{robot}-joint-limits.png")
        empty = blue_pixels(HERE / f"{robot}-joint-empty.png")
        changed = int(np.count_nonzero(selected & ~empty))
        assert changed >= 25, f"{robot}: selection tint missing from native viewport"
        print(f"{robot}: {changed} newly blue pixels after selection")


if __name__ == "__main__":
    main()
