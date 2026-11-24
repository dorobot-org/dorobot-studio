# Nano places all three cubes on 45 of 50 hidden layouts

The simulated Adora Nano used its head and two wrist RGB cameras, camera calibration and joint encoders to move red, green and blue floor cubes onto the coffee table in order. When the stationary placement search ran out of reachable clear slots, the controller could turn while tracking previously placed cubes across camera images. A separate Nexus judge measured the physical grasp, release, settled table contact, joint limits, wheel support and furniture collisions.

| Preregistered condition | Result |
| --- | --- |
| Complete the entire three-cube task on at least 45 of 50 layouts | **45/50 (90%)** |
| Verify every motor journal | **50/50** |
| Measured guard bypasses | **0** |
| Synchronized release packets inspected | 101 |
| 95% Wilson interval for complete-task success | 78.6–95.7% |

The 50 layout commitments and executable inputs were published in [e1e8046](https://github.com/dorobot-org/dorobot-studio/commit/e1e8046) before dispatch. All cases ran once. This cohort qualifies the continuous-tracking controller included in [PR #77](https://github.com/dorobot-org/dorobot-studio/pull/77). Results apply to the recorded Nexus model, camera timing and fixture. Cohort G uses different layouts and a different placement controller.

## The five failed layouts

| Index | Scene seed | First reported stop |
| ---: | ---: | --- |
| 15 | 2559223189 | Cube motion measured in the wrist images disagreed during the placement turn. |
| 27 | 4022072275 | The red cube could not be confirmed in the wrist camera after two grasps. |
| 29 | 259167547 | No reachable clear release point was visible on the tabletop. |
| 40 | 342072357 | Consecutive turn images lost the previously placed cube landmarks. |
| 48 | 305270804 | The robot lost red after two ordinary carries and a supported carry. |

The verdict file lists every secondary failure, including the scored cube contacts. [Verified results](verification.json) · [Frozen inputs](suite.json) · [All 50 original trial records](evidence.zip) · [Archive SHA-256](evidence.zip.sha256)

## Verify in Studio

1. Open **AISLE Studio → Evidence → Batch evaluation**. Enter the absolute path of this directory's `evidence.zip` and choose **Verify evaluation**.
2. Open the failed layouts to see the seed and measured criteria. Restart Studio to reopen the verified result.
3. To rerun a case on this controller, open Studio at integration revision `476f293`. In **Task** select Nano, request all three floor cubes in red, green, blue order with experimental wrist clearance, and choose an author. In **Scene** enter the seed and click **Set seed**, then start the local Nexus simulation in **Run**.

From the Studio checkout with its AISLE Python environment active, run the evidence audit and compare the complete verdict with the published one.

```sh
guide=docs/product/aisle-studio/validation/nano-release-workspace
python "$guide/audit_qualification.py" "$guide/cohort-f" > /tmp/aisle-cohort-f.json
diff -u "$guide/cohort-f/verification.json" /tmp/aisle-cohort-f.json
```

Native Makepad opened the published archive, displayed two different failed layouts, rejected a missing replacement and restored the verified 45/50 result after restart. [Evidence screen](evaluation.png) · [Native instrument record](native.json)
