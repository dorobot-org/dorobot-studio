"""Compare the selected controller with its measured parent or an earlier saved revision."""

import hashlib
import json
import re


def changes(before, after, path=()):
    """Preserve JSON null, additions and removals as distinct cases."""
    if isinstance(before, dict) and isinstance(after, dict):
        for key in sorted(before.keys() | after.keys()):
            field = (*path, key)
            if key not in before:
                yield {"path": field, "after": after[key]}
            elif key not in after:
                yield {"path": field, "before": before[key]}
            else:
                yield from changes(before[key], after[key], field)
    elif (
        isinstance(before, list)
        and isinstance(after, list)
        and len(before) == len(after)
    ):
        for index, (left, right) in enumerate(zip(before, after)):
            yield from changes(left, right, (*path, str(index)))
    elif before != after or isinstance(before, bool) != isinstance(after, bool):
        yield {"path": path, "before": before, "after": after}


def compare(root, robot, selected, expected, earlier=None):
    data = selected.read_bytes()
    digest = hashlib.sha256(data).hexdigest()
    if digest != expected:
        raise ValueError(
            "Selected controller contents changed. Select the revision again."
        )
    plan = json.loads(data)
    if plan["robot"] != robot:
        raise ValueError("Selected controller belongs to a different robot")
    # Training preserves the teacher's fields and adds its own immediate parent.
    # The inherited previous_trial may refer to an earlier authoring generation.
    parent_hash = plan.get("parent_controller_sha256")
    if parent_hash is None:
        feedback = plan.get("previous_trial") or {}
        pickup = plan.get("pickup_revision") or {}
        parent_hash = feedback.get(
            "controller_hash", pickup.get("parent_controller_sha256")
        )
        if pickup.get("parent_controller_sha256", parent_hash) != parent_hash:
            raise ValueError("The saved revision names conflicting parent controllers")
    if parent_hash is not None:
        if not isinstance(parent_hash, str) or not re.fullmatch(
            r"[a-f0-9]{64}", parent_hash
        ):
            raise ValueError("Invalid parent controller digest")
        if parent_hash == digest:
            raise ValueError("The selected controller names itself as its parent")
        for candidate in sorted(root.glob(f"*-{robot}-*/controller.json")):
            try:
                parent_data = candidate.read_bytes()
            except FileNotFoundError:
                continue
            if hashlib.sha256(parent_data).hexdigest() == parent_hash:
                break
        else:
            raise ValueError(
                f"Parent {parent_hash[:12]} is unavailable in this workspace"
            )
        relation = "parent"
    else:
        if earlier is None:
            raise ValueError(
                "This is the first saved revision and has no recorded parent"
            )
        candidate = earlier
        parent_data = candidate.read_bytes()
        parent_hash = hashlib.sha256(parent_data).hexdigest()
        relation = "earlier"
    parent = json.loads(parent_data)
    if parent["robot"] != robot:
        raise ValueError("The comparison controller belongs to a different robot")

    def identity(path, doc, sha):
        return dict(
            revision=path.parent.name,
            sha256=sha,
            provider=doc.get("provider"),
            model=doc.get("policy_model"),
            task=doc.get("task"),
            robot_model=doc.get("model_hash"),
        )

    # The measurement packet and the author's explanation describe why a change was made.
    # Their hashes identify the parent above. They are not executable controller settings.
    metadata = {
        "robot",
        "task",
        "model_hash",
        "provider",
        "policy_model",
        "previous_trial",
        "pickup_revision",
        "parent_controller_sha256",
    }

    def settings(doc):
        return {k: v for k, v in doc.items() if k not in metadata}

    return dict(
        robot=robot,
        relation=relation,
        before=identity(candidate, parent, parent_hash),
        after=identity(selected, plan, digest),
        changes=list(changes(settings(parent), settings(plan))),
    )


def display(report):
    old, new = report["before"], report["after"]
    heading = (
        "Recorded parent"
        if report["relation"] == "parent"
        else "Earlier saved revision"
    )
    lines = [
        f"{heading} · {old['sha256'][:12]}",
        f"Selected revision · {new['sha256'][:12]}",
    ]

    def author(doc):
        names = {
            "octos-deepseek": "DeepSeek via Octos",
            "codex": "Codex",
            "reference": "Reference",
        }
        provider = names.get(doc["provider"], doc["provider"] or "Unrecorded")
        return provider + (f" ({doc['model']})" if doc["model"] else "")

    fields = {
        "delta_rad": ("Joint displacement", "rad"),
        "max_forward_speed_m_s": ("Forward speed limit", "m/s"),
        "standoff_m": ("Stopping distance", "m"),
        "grasp_association": ("Pickup association", ""),
        "track_grasp": ("Wrist-guided pickup", ""),
        "color_order": ("Cube order", ""),
        "joint": ("Joint", ""),
    }

    def value(v, unit):
        rendered = (
            v
            if isinstance(v, str)
            else json.dumps(v, ensure_ascii=False, sort_keys=True)
        )

        return f"{rendered} {unit}" if unit else rendered

    for change in report["changes"]:
        key = ".".join(change["path"])
        field, unit = fields.get(key, (key, ""))
        if "before" not in change:
            lines.append(f"{field} · added {value(change['after'], unit)}")
        elif "after" not in change:
            lines.append(f"{field} · removed {value(change['before'], unit)}")
        else:
            lines.append(
                f"{field} · {value(change['before'], unit)} → {value(change['after'], unit)}"
            )
    if not report["changes"]:
        lines.append("Controller settings are identical.")
    lines.append(
        f"Author · {author(old)} → {author(new)}"
        if author(old) != author(new)
        else f"Author · {author(new)}"
    )
    if old["task"] != new["task"]:
        lines.extend(
            [f"Earlier task · {old['task']}", f"Selected task · {new['task']}"]
        )
    if old["robot_model"] != new["robot_model"]:
        lines.append(f"Robot model · {old['robot_model']} → {new['robot_model']}")

    return "\n\n".join(lines)
