"""Exercise Task's review controls and verify the text shown for the selected controller."""

import json


def inspect(
    args, click, wait, grab, name, relation, before_hash, after_hash, expected_line
):
    click("aisle_task.review_controller")

    def opened(ui):
        state = ui["state"]["aisle"]
        if not state["busy"] and state["status"].startswith("Failed"):
            raise RuntimeError(state["status"])
        report = state.get("controller_diff")
        label = ui["controls"]["aisle_task.controller_changes"]
        return (
            report is not None
            and report["after"]["sha256"] == after_hash
            and ui["controls"]["aisle_task.diff_scroll"]["visible"]
            and label["text"] == report["display"]
        )

    ui = wait(opened, "Controller comparison opened", 30)
    state = ui["state"]["aisle"]
    review = state["controller_diff"]
    assert review["before"]["sha256"] == before_hash and review["relation"] == relation
    assert expected_line in ui["controls"]["aisle_task.controller_changes"]["text"]
    grab(name)
    click("aisle_task.review_controller")
    closed = wait(
        lambda r: r["state"]["aisle"]["controller_diff"] is None
        and not r["controls"]["aisle_task.diff_scroll"]["visible"],
        "Controller comparison closed",
    )
    assert closed["state"]["aisle"]["controller"] == state["controller"]
    (args.out / (name + ".json")).write_text(json.dumps(review, indent=2) + "\n")
