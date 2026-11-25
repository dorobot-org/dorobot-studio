"""Locate the Nano policy command active at each scored loss of wheel support."""

import json
from math import isfinite


def feedback(trial, result, controller):
    total = result.get("physics_ticks")
    supported = result.get("wheel_supported_ticks")
    if total is None or supported is None:
        return {}
    if (
        type(total) is not int
        or type(supported) is not int
        or not 0 <= supported <= total
    ):
        raise ValueError("Invalid wheel-support counts")
    if total == supported and "max_forward_speed_m_s" not in controller:
        return {}
    events = []
    ranges = result.get("unsupported_wheel_ranges")
    if ranges is not None:
        end = 0
        missing = 0
        for row in ranges:
            first, last = row["first_step"], row["last_step"]
            if (
                type(first) is not int
                or type(last) is not int
                or not end < first <= last <= total
            ):
                raise ValueError("Invalid wheel-support interval")
            missing += last - first + 1
            end = last
            if len(events) < 8:
                events.append(
                    dict(first_tick=first, last_tick=last, phase=row["phase"])
                )
        if missing != total - supported:
            raise ValueError("Wheel-support intervals differ from the reported count")
    journal = trial / "actions.jsonl"
    index = 0
    if events and journal.is_file():
        with journal.open() as stream:
            for row in map(json.loads, stream):
                kind = row.get("op")
                if kind not in ("wheel", "joint"):
                    continue
                start = row["tick"]
                duration = 20 if kind == "wheel" else row["steps"]
                while (
                    index < len(events)
                    and events[index]["first_tick"] <= start + duration
                ):
                    if events[index]["first_tick"] > start:
                        action = dict(issued_tick=start, kind=kind)
                        if kind == "wheel":
                            speed, turn = row["speed"], row["turn"]
                            if not all(
                                type(v) in (int, float) and isfinite(v)
                                for v in (speed, turn)
                            ):
                                raise ValueError("Invalid forward or yaw command")
                            action.update(forward_m_s=speed, yaw_rad_s=turn)
                        events[index]["active_command"] = action
                    index += 1
                if index == len(events):
                    break
    return dict(
        wheel_support=dict(
            physics_ticks=total,
            supported_ticks=supported,
            unsupported_ticks=total - supported,
            first_intervals=events,
            max_forward_speed_m_s=controller.get("max_forward_speed_m_s", 0.13),
        )
    )
