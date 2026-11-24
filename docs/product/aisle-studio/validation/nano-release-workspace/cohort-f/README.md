# Nano places all three cubes on 45 of 50 hidden layouts

The simulated Adora Nano used its head and two wrist RGB cameras, camera calibration and joint encoders to move red, green and blue floor cubes onto the coffee table in order. When the stationary placement search ran out of reachable clear slots, the controller could turn while tracking previously placed cubes across camera images. A separate Nexus judge measured the physical grasp, release, settled table contact, joint limits, wheel support and furniture collisions.

| Preregistered condition | Result |
| --- | --- |
| Complete the entire three-cube task on at least 45 of 50 layouts | **45/50 (90%)** |
| Verify every motor journal | **50/50** |
| Measured guard bypasses | **0** |
| Synchronized occupied-table and turning packets inspected | 101 |
| 95% Wilson interval for complete-task success | 78.6–95.7% |

The 50 layout commitments and executable inputs were published in [e1e8046](https://github.com/dorobot-org/dorobot-studio/commit/e1e8046) before dispatch. All cases ran once for the score. The 101 saved packets cover occupied-table checks for green and blue and checks after a placement turn. The first red release starts on an empty table. The judge checks physical contact and settling for every cube. This cohort qualifies the continuous-tracking controller included in [PR #77](https://github.com/dorobot-org/dorobot-studio/pull/77). Results apply to the recorded Nexus model, camera timing and fixture. Cohort G uses different layouts and a different placement controller.

## The five failed layouts

| Index | Scene seed | First reported stop |
| ---: | ---: | --- |
| 15 | 2559223189 | Cube motion measured in the wrist images disagreed during the placement turn. |
| 27 | 4022072275 | The red cube could not be confirmed in the wrist camera after two grasps. |
| 29 | 259167547 | No reachable clear release point was visible on the tabletop. |
| 40 | 342072357 | Consecutive turn images lost the previously placed cube landmarks. |
| 48 | 305270804 | The robot lost red after two ordinary carries and a supported carry. |

The verdict file lists secondary failures, including the scored cube contacts. The output digest identifies the archive published by this repository. The audit checks the preregistered Git manifest, the controller packaged in the archive, and all 16 Studio runtime files against the named integration revision. [Source revisions and program hashes](provenance.json) [Verified results](verification.json) · [Frozen inputs](suite.json) · [All 50 original trial records](evidence.zip) · [Archive SHA-256](evidence.zip.sha256)

## Verify in Studio

1. Open **AISLE Studio → Evidence → Batch evaluation**. Enter the absolute path of this directory's `evidence.zip` and choose **Verify evaluation**.
2. Open the failed layouts to see the seed and measured criteria. Restart Studio to reopen the verified result.
3. To start a fresh authored trial on this layout, open Studio at integration revision `476f293`. In **Task** select Nano, request all three floor cubes in red, green, blue order with experimental wrist clearance, and choose an author. In **Scene** enter the seed and click **Set seed**, then start the local Nexus simulation in **Run**.

From the Studio checkout with its AISLE Python environment active, run the evidence audit and compare the complete verdict with the published one.

```sh
guide=docs/product/aisle-studio/validation/nano-release-workspace
python "$guide/audit_qualification.py" "$guide/cohort-f" --check
```

Native Makepad opened the published archive, displayed two different failed layouts, rejected a missing replacement and restored the verified 45/50 result after restart. [Evidence screen](evaluation.png) · [Native instrument record](native.json)

## Rerun one disclosed layout with the exact controller

The following example runs layout 27 with the archived Codex program. It uses the frozen Studio runtime and the same AISLE Python environment used for the qualification. This layout originally stopped after two unsuccessful red grasp confirmations. A completed failed trial writes a result and returns exit code 1.

```sh
archive="$PWD/docs/product/aisle-studio/validation/nano-release-workspace/cohort-f/evidence.zip"
scratch=$(mktemp -d)
git worktree add --detach "$scratch/studio" 476f293006d0ac0f07c71d80501d86b7e925a26a
unzip -p "$archive" controller.json > "$scratch/controller.json"
unzip -p "$archive" case-027/motor-targets.f64 > "$scratch/expected-motors.f64"
python "$scratch/studio/tools/aisle-validation/run_rgb_transfer.py" \
  --seed 4022072275 --controller "$scratch/controller.json" --out "$scratch/trial" \
  || test -s "$scratch/trial/result.json"
cmp "$scratch/expected-motors.f64" "$scratch/trial/motor-targets.f64"
```

The fresh trial stays in the temporary directory. Its outcome is outside the original 50-case denominator.

The separate run reproduced the same red-grasp stop after 10,794 steps and matched the original motor file byte for byte. [Disclosed rerun record](disclosed-reproduction.json)

## Wrist-camera fault checks on layout 0

The same Codex-authored program completed the task with normal wrist images in 23,724 steps. When wrist delivery was interrupted during the first pick, it held all 17 motor targets for 160 steps (1.6 s), resumed on a fresh exposure and completed the task in 26,462 steps. With a sustained disconnect it held the targets for 180 steps and stopped at tick 1180. The native Makepad journey restored the recorded timeout after a restart. These three checks are separate from the 50 nominal-timing layouts. [Motor journals, camera packets, delivery logs and native checks](native-wrist-timing.zip)
