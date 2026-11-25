# Pick up a sliding cube with the Nano

In **AISLE Studio → Task**, select **Adora Nano**, enter “Use onboard RGB to track the moving red floor cube during pickup and place it on the coffee table,” and choose **DeepSeek via Octos** or **Ask Codex**. The controller draft should show wrist-guided pickup. In **Scene**, select a layout and **+Y** or **−Y** under **Cube motion · world Y**. Open **Run → Start run**. In **Evidence**, inspect signed speed before first finger contact, elevated two-finger steps and final placement. **Export episode** and **Open in Replay** open the recorded camera and joint data.

The outer model selects a versioned transfer skill and cube order from the task. A separate local controller estimates the cube during descent from the Nano's three calibrated RGB cameras, joint encoders and capture times. When camera estimates agree, it prefers the left wrist view near the gripper. It updates the grasp point eight times and projects the measured motion forward by 0.5 s per waypoint and 0.8 s on the last one. Floor friction, cube positions, contact impulses and the selected slide direction are private to the simulation fixture and scorer.

## What counts as a moving grasp

At 100 physics steps per simulated second, the fixture commands up to 0.060 m/s along the selected world axis for at most 95 mm, during a bounded portion of the **first** pick. It stops sending velocity commands on first finger contact. The other velocity components are preserved. The preceding full uncontacted step must show more than 3 mm/s in the selected direction after at least 12 mm of signed travel. The first attempt must then record at least 20 steps with both fingers loaded, the cube at least 50 mm above its starting height and no other supporting body. Final placement still requires release within the coffee-table rim, all 100 settling steps in table contact, wheel support, furniture clearance and a valid motor journal. If the Nano later picks up a stationary cube on a retry, Evidence can show a successful placement and an unsuccessful moving grasp.

The [protocol](protocol.json) fixes both provider controllers, the fixture, three development layouts and four additional layouts selected before the paired evaluation. Each layout is tested in both directions using the same floor, cube and calibrated optics. The direct provider comparison measures the model's skill selection and the behavior of the shared local controller. The mass, surface and disturbance are specific to this simulation.

## Results

| Matched cases | Moving pickup and settled placement per provider | Final cube on table per provider |
| --- | ---: | ---: |
| Three development layouts × two directions | 5/6 | 6/6 |
| Four fresh layouts × two directions | 7/8 | 8/8 |
| All seven layouts × two directions | **12/14** | **14/14** |

Codex and DeepSeek through Octos independently selected the same tracking skill. The two Nexus executions produced identical action and motor journals on every paired case. Each controller maintained wheel support throughout 106,776 steps and replayed 112 first-attempt descent decisions. It localized 110 exposures, with a maximum measured planar error of 17.2 mm. All 14 first finger contacts occurred at about 13.9 mm/s after 53.2–53.7 mm of measured travel. The 12 successful first attempts had 166–215 elevated opposing-finger steps. On the negative slide on layout 2 and fresh layout 643393491, the first contact produced zero elevated opposing-finger steps. Both later stationary retries placed the cube.

The original stationary controller placed the cube on both moving fixtures on layout 0 but missed first-attempt moving pickup in both directions. With a stationary cube it reproduced the prior version's complete action journal and all 6,848 motor commands. [Per-case measurements](measurements.json) · [Raw RGB, contact and motor evidence](trials.zip)

The native DeepSeek headless journey observed −13.9 mm/s at contact, 215 opposing-finger steps and 337 exported frames per camera. The Codex offscreen journey observed +13.9 mm/s, 203 opposing-finger steps and 334 frames per camera. Both exported all three videos, checked each measured and commanded joint column in the dataset, opened Replay, and restored the result after restart. [Native interaction records](native.json) · [Live run](transfer-run.png) · [Scored grasp](transfer-evidence.png)

### Three moving cubes

DeepSeek via Octos revised the failed negative-slide baseline using its public outcome and wrist and table observations. The operator requested red, green and blue in that order. On layout 0, this revision caught all three moving cubes on the positive slide and placed them on the table. It recorded 203, 194 and 208 first-attempt elevated two-finger steps. Wheel support held throughout 32,480 steps. On the negative slide it caught and placed red and green, then stopped at step 17,814 after losing the blue pickup point in the onboard cameras. The three-cube moving sequence therefore passed **one of two directions** on this layout. The archive includes the failed trial, its parent controller identity and the exact author request. A subsequent revision used wrist-camera agreement to resolve the blue ambiguity and completed all three moving pickups in both directions on this layout. [Matched follow-up, native measurements and video](../moving-grasp-wrist-agreement/README.md)

## Reproduce a trial

Use the Studio AISLE environment and set `DOROBOT_AISLE_ROOT` and `PYTHONPATH` as in the [workbench setup](../../README.md). From the repository root, unpack the trial bundle to a new directory and replay its evidence.

```sh
unzip -q docs/product/aisle-studio/validation/moving-grasp/trials.zip -d /tmp/nano-moving-grasp
.venv-aisle/bin/python tools/aisle_workbench/moving_grasp_check.py \
  /tmp/nano-moving-grasp/octos-deepseek_slide-negative_0
.venv-aisle/bin/python tools/aisle_workbench/worker.py run --robot Nano \
  --controller /tmp/nano-moving-grasp/providers/octos-deepseek/controller.json \
  --target-motion slide-negative --seed 0 --observations-only --out /tmp/nano-moving-grasp-repeat
.venv-aisle/bin/python tools/aisle_workbench/moving_grasp_check.py /tmp/nano-moving-grasp-repeat
```

The checker reloads each first-descent RGB packet, reestimates the pickup point, checks the intercept against the issued command, reconstructs speed and lift from the private contact trace, and replays every motor target against the actual Nano URDF. It also checks the final verdict against the recorded placement and support measurements. To exercise the native pages use `tools/aisle_workbench/verify.py --transfer red --pickup-motion slide-negative --octos-deepseek` with the Studio binary, Makepad remote, backend and a fresh output directory.
