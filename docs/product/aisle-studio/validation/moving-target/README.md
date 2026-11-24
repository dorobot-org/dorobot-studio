# Approach a moving floor cube with Nano

In **AISLE Studio → Task**, select **Adora Nano** and enter “Approach the red floor cube and stop at 0.45 metres using the onboard cameras.” Author it with **DeepSeek via Octos** or **Codex**. In **Scene**, select development layout **0** and choose **+Y** or **−Y** under **Cube motion · world Y**. Keep **On time** selected for the first trial. Open **Run → Start run** to watch the head camera. Open **Evidence** for the final standoff, signed cube displacement, moving-exposure count and maximum localization error. **Still** supplies the stationary comparison. [Scene](scene.png) · [Live onboard view](run.png) · [Scored approach](evidence.png)

## What moves and what is measured

Nexus gives the loose target cube five lateral velocity pulses of 0.60 m/s at physics ticks 76, 96, 116, 136 and 156. Each pulse occurs four 10 ms physics steps before a camera exposure. The cube slides on the living-room floor while the Nano drives and turns. The fixture sends no pose writes to the robot or cube. The controller runs in a separate process. Each packet contains the head, left wrist and right wrist RGB images, measured joint encoders and a capture timestamp. The evaluator records the cube positions and contact information separately.

The evaluator classifies an exposure as moving when measured planar speed over its preceding physics step exceeds 0.02 m/s. It requires at least 100 mm of signed travel, all five pulses, at least three moving exposures, a fresh localization in every moving exposure, and a maximum planar error of 35 mm. The approach must also settle over three observations within 35 mm of the requested distance, maintain wheel support and avoid obstacles. The cube comes to rest before final alignment. Grasping a moving cube is a separate task.

## Results on the same layout

Codex and DeepSeek each authored the 450 mm approach once. Each provider's exact program drove four trials with the same layout and calibration. Both providers produced the measurements below. There were zero recorded robot–obstacle contact steps and continuous wheel support in every trial.

| Target and camera condition | Cube travel along Y | Localized while moving | Worst moving error | Final distance | Verdict per provider |
| --- | ---: | ---: | ---: | ---: | --- |
| Still, on time | 0 mm | No moving exposures | — | 468.1 mm | Passed |
| +Y, on time | +130.4 mm | 5/5 | 3.47 mm | 467.9 mm | Passed |
| +Y, delayed | +130.4 mm | 0/5 | No fresh estimates | 466.0 mm | Failed |
| −Y, on time | −129.7 mm | 5/5 | 2.81 mm | 466.3 mm | Passed |

With **Delay**, repeated and late packets covered all five moving exposures. The robot subsequently recovered on current RGB and reached the requested distance tolerance. Evidence retained the moving-perception failure. Each provider replayed all 119 decisions from its four sets of saved camera packets, including all eight withheld observations in the delayed trial. These are matched development runs on one layout. [Delayed trial in Evidence](delay.png) · [Headless and offscreen native checks, build and source fingerprints](native.json) · [Both providers' eight raw episodes](episodes.zip)

## Reproduce the evidence

Apply the [Studio setup](../../README.md) and run the following from the repository root. The archive contains an independent directory for every combination of provider and trial. The replay checks the URDF digest and rebuilds each command from the recorded images, encoder state and timestamps.

```sh
trial_dir=$(mktemp -d)
unzip -q docs/product/aisle-studio/validation/moving-target/episodes.zip -d "$trial_dir"
for episode in "$trial_dir"/*/*; do
  "$DOROBOT_AISLE_PYTHON" tools/aisle-validation/verify_rgb_replay.py "$episode"
done
```

To repeat the native sequence, add `--moving-target --octos-deepseek` or `--moving-target --codex` to the [Makepad verifier invocation](../../README.md). Each journey opens Scene, Run and Evidence, executes the four conditions and restores the final scored episode after restart. The 1024 × 768 headless journey also scrolls to the model limits and back to the fixture controls.
