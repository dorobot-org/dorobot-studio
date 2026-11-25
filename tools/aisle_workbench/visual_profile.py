"""Identity of the optional Nano camera profile shared by Studio and fleet workers."""

import hashlib
from pathlib import Path

COMPACT = "compact-v1"
BUNDLE = (
    Path(__file__).resolve().parents[2] / "data/adora_nano/meshes/visual-compact-v1.zip"
)
BUNDLE_SHA256 = "d3050070deea1bbcd4edd59e32d85de845230173bb77aec901ddcd63da2fb9a2"


def available():
    return (
        BUNDLE.is_file()
        and hashlib.sha256(BUNDLE.read_bytes()).hexdigest() == BUNDLE_SHA256
    )


def validate_profile(name, plan):
    if name == "original":
        return
    if (
        name != COMPACT
        or plan.get("robot") != "Nano"
        or plan.get("kind") != "table-transfer"
    ):
        raise ValueError("Compact camera rendering requires a Nano table transfer")
    if "scene_observation" in plan:
        raise ValueError(
            "This controller is bound to a scene observation from the original cameras"
        )
    if not available():
        raise ValueError("Install the matching Nano compact-v1 visual bundle")
