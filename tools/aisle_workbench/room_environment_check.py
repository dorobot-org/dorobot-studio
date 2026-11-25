"""Drive independent room edits through the native Scene controls."""

import hashlib
from pathlib import Path


def edit_environment(start, report, remote, fill, click, done, grab):
    x, y, width, height = report()["controls"]["aisle_scene.survey_card"]["rect"]
    remote(
        "mouse",
        "k=scroll",
        "w=0",
        f"x={x + width / 2}",
        f"y={y + height / 2}",
        "dy=620",
        "wait=1",
    )
    fill("room_friction", "2.5")
    click("aisle_scene.save_environment")
    failed = done(
        lambda w: not w["busy"] and "friction coefficient" in w["status"],
        "Out-of-range floor friction rejected",
    )
    assert failed["room_path"] == start["room_path"]

    def change(parent, field, entry, key, expected, label):
        fill(field, entry)
        click("aisle_scene.save_environment")
        row = done(
            lambda w: not w["busy"] and w["room_path"] != parent["room_path"], label
        )
        before, after = parent["room_info"], row["room_info"]
        assert (
            after["parent_sha256"]
            == hashlib.sha256(Path(parent["room_path"]).read_bytes()).hexdigest()
        )
        assert after["settings"] == {**before["settings"], key: expected}
        for same in ("obstacles", "reference_sha256", "image_to_floor", "model_sha256"):
            assert after[same] == before[same]
        return row

    original = start["room_info"]["settings"].get("floor_friction", 0.7)
    fill("room_friction", str(original))
    lit = change(
        start,
        "room_ambient",
        "0.40 0.42 0.44",
        "ambient_rgb",
        [0.40, 0.42, 0.44],
        "Ambient RGB saved with the same floor and calibration",
    )
    result = change(
        lit,
        "room_friction",
        "0.80",
        "floor_friction",
        0.80,
        "Floor friction saved with the same light and geometry",
    )
    grab("survey-environment")
    return result
