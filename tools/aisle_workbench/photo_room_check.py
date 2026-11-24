"""Exercise photo drafting, review, application and Nexus readback in the native Scene page."""

import hashlib
from pathlib import Path
import sys

import numpy as np


def draft(args, checks, click, remote, done, grab):
    click("aisle_scene.room_mode")
    done(lambda w: w["room_editor"], "Room editor opened")
    click("aisle_scene.survey_path")
    remote(
        "key",
        "w=0",
        "c=KeyA",
        "logo=1" if sys.platform == "darwin" else "ctrl=1",
        "wait=1",
    )
    remote("text", "w=0", "t=" + str(args.photo_input.resolve()), "wait=1")
    button = (
        "aisle_scene.photo_octos"
        if args.provider == "octos-deepseek"
        else "aisle_scene.photo_codex"
    )
    click(button)
    result = done(
        lambda w: not w["busy"] and w["room_draft_path"],
        "Photographic geometry drafted for review",
        180,
    )
    assert result["room_path"] is None
    receipt = result["room_draft"]["photo_draft"]
    assert (
        receipt["provider"] == args.provider
        and receipt["input_sha256"]
        == hashlib.sha256(args.photo_input.read_bytes()).hexdigest()
    )
    assert len(result["room_draft"]["obstacles"]) == 1
    obj = result["room_draft"]["obstacles"][0]
    # Independently surveyed dimensions and floor center for the photographed reference ottoman.
    np.testing.assert_allclose(obj["position"][:2], [1.28, -0.45], rtol=0, atol=0.025)
    np.testing.assert_allclose(obj["size"], [0.4, 0.31, 0.33], rtol=0, atol=0.025)
    checks[-1].update(object=obj, provenance=receipt)
    grab("photo-draft")
    click("aisle_scene.apply_draft")
    applied = done(
        lambda w: not w["busy"] and w["room_path"] and not w["room_draft_path"],
        "Reviewed room selected for simulation",
    )
    assert (
        applied["room_info"] == result["room_draft"]
        and applied["room_path"] == result["room_draft_path"]
    )
    grab("photo-applied")
    click("aisle_scene.robot_mode")
    return obj, hashlib.sha256(Path(applied["room_path"]).read_bytes()).hexdigest()


def verify_run(result, obj, revision):
    assert result["room_revision_sha256"] == revision and result["room_obstacles"] == [
        obj["name"]
    ]
    body = result["measured_room_bodies"][obj["name"]]
    np.testing.assert_allclose(body["center_m"], obj["position"], rtol=0, atol=1e-5)
    return body
