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

## Reproduce a disclosed layout in Studio

In **Task**, select **Nano** and enter “Move all three floor cubes onto the coffee table in red, green, blue order. Use experimental wrist clearance.” Choose **Ask Codex** or **DeepSeek via Octos**. In **Scene**, enter the numeric layout seed and click **Set seed**. The label shows the active layout. Seeds from 0 to 4294967295 are supported. Invalid input preserves the previous layout. **Run → Start simulation** executes that layout. **Evidence** shows the independent result and can export the episode. The selected seed survives a Studio restart. [Scene with seed 3856023877 selected](custom-layout.png)

The agent authors the task and chooses the requested placement mode. The local controller watches the table, plans the arm movements and sends each bounded motor command. A release decision uses the onboard images, camera calibration and robot encoders. The judge records contacts and settled cube positions separately.

## Development experiments

| Method | Complete transfers on the five original refusals | Outcome |
| --- | ---: | --- |
| Original stationary placement | 0/5 | No slot passed both reachability and camera clearance. |
| Straight 8 cm wheel sidestep | 0/5 | The measured table heading changed too far during sideways wheel motion. All five trials stopped. All four control layouts passed. |
| Slower sidestep with steering feedback | 3/5 | Recovered layouts 9, 15 and 19. On 16 and 36, no target passed the final camera and arm checks. |
| Turn in place with landmark matches between the first and last images | 2/5 | Recovered layouts 9 and 36. On 15 and 19, the visible wrist camera changed. On 16, all three cubes settled, but the robot had already lost loaded wheel contact for one simulation tick. |

The archived development attempts include the full motor and camera evidence and a source patch for each alternative against commit `93d67f4`. The selected turn uses the robot’s existing steering control. It registers complete silhouettes between consecutive camera exposures so that an earlier cube can pass from one wrist’s view to the other. It retains earlier detections when they become occluded. An interrupted observation chain stops the turn. [All three exploratory attempts and control layouts](development-attempts.zip)
