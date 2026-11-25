"""Inspect each real movable joint through the native Scene controls."""

import re
import sys
import xml.etree.ElementTree as ET

from worker import MODELS, ROOT


def check(robot, click, remote, wait, grab):
    tree = ET.parse(ROOT / "data" / MODELS[robot][0])
    joints = [joint for joint in tree.findall("joint") if joint.get("type") != "fixed"]
    scene = "aisle_scene."

    def shown(data, name):
        return data["controls"][scene + "joint_name"]["text"] == name and data["state"][
            "aisle_joint_highlights"
        ] == [[name, False]]

    def filter_by(text):
        click(scene + "joint_query")
        remote(
            "key",
            "w=0",
            "c=KeyA",
            "logo=1" if sys.platform == "darwin" else "ctrl=1",
            "wait=1",
        )
        remote("text", "w=0", "t=" + text, "wait=1")

    click(scene + "joint_mode")
    records = {}
    for index, joint in enumerate(joints):
        name = joint.attrib["name"]
        if index:
            click(scene + "joint_next")
        report = wait(lambda data: shown(data, name), "URDF marker for " + name)
        controls = report["controls"]
        assert (
            controls[scene + "joint_count"]["text"]
            == f"Joint {index + 1} of {len(joints)}"
        )
        assert controls[scene + "joint_previous"]["enabled"] == (index > 0)
        assert controls[scene + "joint_next"]["enabled"] == (index + 1 < len(joints))
        details = controls[scene + "joint_detail"]["text"].splitlines()
        kind = joint.attrib["type"]
        assert details[0] == kind
        assert "Parent · " + joint.find("parent").attrib["link"] in details
        assert "Child · " + joint.find("child").attrib["link"] in details
        if kind == "continuous":
            assert "Position · continuous rotation" in details
        lim = joint.find("limit")
        labels = {
            "lower": "Lower",
            "upper": "Upper",
            "velocity": "Speed limit",
            "effort": "Effort limit",
        }
        for key, label in labels.items():
            if key not in lim.attrib:
                continue
            unit = {"velocity": "rad/s", "effort": "N·m"}.get(key, "rad")
            match = next(
                re.fullmatch(re.escape(label) + r" · (\S+) " + re.escape(unit), line)
                for line in details
                if line.startswith(label + " · ")
            )
            assert match and float(match[1]) == float(lim.attrib[key])
        records[name] = "\n".join(details)
    filter_by("ABSENT JOINT")
    wait(
        lambda j: j["controls"][scene + "joint_count"]["text"].startswith("No matching")
        and j["state"]["aisle_joint_highlights"] == []
        and j["controls"][scene + "joint_detail"]["text"] == "",
        "Empty search cleared the marker",
    )
    grab(robot + "-joint-empty")
    query, selected = (
        ("GRIPPER", "right_j6_gripper")
        if robot == "Nano"
        else ("ANKLE ROLL", "right_ankle_roll_joint")
    )
    filter_by(query)
    wait(
        lambda j: j["controls"][scene + "joint_count"]["text"].startswith(
            "Match 1 of 2"
        ),
        "Filtered pair",
    )
    click(scene + "joint_next")
    wait(lambda j: shown(j, selected), "Filtered joint and model marker")
    grab(robot + "-joint-limits")
    click(scene + "joint_previous")
    wait(
        lambda j: j["controls"][scene + "joint_count"]["text"].startswith(
            "Match 1 of 2"
        ),
        "Previous filtered joint",
    )
    click(scene + "robot_mode")
    wait(
        lambda j: not j["controls"][scene + "joint_card"]["visible"]
        and j["state"]["aisle_joint_highlights"] == [],
        "Returned to fixture",
    )
    return dict(
        check="All movable joint names, URDF limits and model markers",
        robot=robot,
        joints=records,
    )
