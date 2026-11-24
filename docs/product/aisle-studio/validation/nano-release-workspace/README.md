# Finding a reachable release point on the Nano coffee table

On the 50 previously concealed cohort E layouts, the experimental three-camera controller completed 40 three-cube transfers. It stopped during placement in five layouts because its left arm could reach no release point meeting the camera clearance rule. Replaying each of those five layouts with exposure recording produced identical motor journals, physical verdicts and per-cube measurements. All five motor journals verified with zero guard bypasses. These disclosed layouts are development cases. [Protocol](protocol.md) · [Original evaluation](../transfer-wrist-timing/README.md)

## What the Nano saw

At the blocked decision the Nano had three synchronized 480 × 360 RGB images, measured encoders, camera calibration and an estimate of the visible table rim. The arm planner checked the incoming closed-jaw path and the open-jaw withdrawal. A target needed at least 85 mm of observed table inset and 78 mm of clearance from each observed earlier cube.

| Original layout | Cube held | Nearest projected earlier cube after a hypothetical 8 cm move to the right |
| --- | --- | ---: |
| 9 | Blue | 97 mm |
| 15 | Green | 87 mm |
| 16 | Green | 90 mm |
| 19 | Green | 86 mm |
| 36 | Blue | 89 mm |

In all five stationary poses, the search returned zero acceptable slots. The last column is a geometric prediction from the failed decision's saved sensor inputs. A new pose must be reached in the simulation and measured again before the gripper can open. The independent judge observes loaded finger and table contact, continuous wheel support, joint limits and final cube motion.

To inspect the original captures, use the Studio AISLE Python environment from the repository root. The archive contains each action, the three RGB inputs and measured joint encoders at each marked release decision, the calibrated cameras, motor targets and the final scoring records.

```sh
DIR=docs/product/aisle-studio/validation/nano-release-workspace
python "$DIR/inspect_release.py" "$DIR/before.zip"
python tools/aisle-validation/nano_suite_evidence.py verify "$DIR/before.zip" \
  --manifest-sha256 0eac7bcc33733163c1f6e6ea86abebe7b245906a5be2634e3000f8258fb8d8cd
```
