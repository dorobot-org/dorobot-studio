# Resolving the blue pickup on the negative slide

The three-cube Nano controller previously stopped at tick 17,814 on layout 0. Red and green had settled on the table. To diagnose blue, the worker repeated the same negative slide and saved the final onboard exposure, encoders and timestamp. Its motor journal reproduced the original failure exactly. Replaying that packet gave two plausible blue positions in the head image and one in the left wrist image. One head estimate was 4.12 mm from the wrist estimate. The other was approximately 243 mm away. The original controller stopped as soon as one view contained multiple plausible candidates. [Previous qualification and failed run](../moving-grasp/README.md)

During a wrist-guided descent, a unique wrist candidate can now resolve another camera's alternatives. Exactly one projected candidate in that view must lie within 35 mm of the wrist estimate. A missing or ambiguous wrist view, or ambiguous proximity to the wrist candidate, still produces a stop on that exposure. The change uses the onboard RGB cameras and encoders. Existing approach and stationary pickup behavior is preserved. The trial calibration records the association method for replay.

A tracked pickup that stops also saves the last received packet as `last-pickup-observation.npz`. Its receipt binds the snapshot hash, color, capture timestamp and stop tick. The moving-grasp checker reloads the snapshot, validates its sensor channels and replays the recorded association method.

## Matched results on living-room layout 0

| Three moving cubes | Previously settled | Revised moving grasps and placements | Supported steps | First-attempt elevated opposing-finger steps (red, green, blue) |
| --- | ---: | ---: | ---: | --- |
| Negative slide | 2/3 | **3/3** | 23,596/23,596 | 215, 205, 210 |
| Positive slide | 3/3 | **3/3** | 32,480/32,480 | 203, 194, 208 |

The operator's controller artifact was identical in each matched pair. On the positive slide the complete motor and action journals also matched the previous version. Two single-red negative-slide controls reproduced their earlier motor journals, including the missed first-attempt lift on seed 643393491 and the success on seed 0. These comparisons cover the disclosed layouts. [Per-cube results, RGB candidates and provenance](measurements.json) · [Sensor packets, contact traces and motor journals](trials.zip)

DeepSeek via Octos also authored the request again in native offscreen Studio. All three cubes were moving at first finger contact and settled on the table for 100/100 steps. Studio exported 1,179 synchronized frames per camera and reopened the result after restart. In the video the views are head, left wrist and right wrist from left to right. It shows all 1,179 exposures at four times simulated speed. The recorded dataset samples at five frames per simulated second. [Watch the native camera replay](nano-three-cubes.mp4) · [Evidence screen](evidence.png) · [Native journey and restart](native.json)

To replay from the Studio checkout with the AISLE Python environment, extract the archive and run the checker on any included trial.

```sh
unzip -q docs/product/aisle-studio/validation/moving-grasp-wrist-agreement/trials.zip -d /tmp/nano-wrist-agreement
.venv-aisle/bin/python tools/aisle_workbench/moving_grasp_check.py /tmp/nano-wrist-agreement/repeat-of-stop
.venv-aisle/bin/python tools/aisle_workbench/moving_grasp_check.py /tmp/nano-wrist-agreement/native-three-negative
```
