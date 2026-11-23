# Nano cube approach with DeepSeek and Octos

In **AISLE Studio → Task**, select **Adora Nano**, enter “Drive within 45 cm of the red floor cube and stop” and choose **DeepSeek via Octos**. The prepared controller shows the selected color, stopping distance and reported model. **Scene** identifies the floating-base living room. Choose **Run → Start run** to watch the head camera and range estimate update as the wheels move. **Evidence** shows the independent final distance, contact counts and a trace of the range estimated from RGB.

The live capture below comes from the Makepad app during the red-cube trial. The recording contains all 23 head-camera observations at the simulator’s 5 Hz observation cadence. [Head-camera video](nano-rgb-approach.mp4) · [Run archive](nano-rgb-approach.zip)

![Studio during the approach with onboard RGB, estimated distance and the Stop button](nano-rgb-live.png)

## What each component controls

1. Octos invokes DeepSeek once to translate the request into the typed visual-approach controller. The Studio worker checks the capability, selected robot, color and stopping distance against the local limits. Codex can author the same controller through **Ask Codex**.
2. The Nano has a floating base, three driven wheel joints and the extended-arm URDF. AISLE runs Nexus on Metal with gravity at 9.81 m/s². The wheel servo accepts up to 5 rad/s and limits each update’s change to 1 rad/s. It advances the simulation at 100 Hz.
3. A separate visual-servo process receives one timestamp, 17 measured encoders and three synchronized onboard RGB arrays per observation. It also receives the versioned robot model and nominal camera calibration. It detects colored image regions and uses camera rays, encoder-derived forward kinematics, the planar floor assumption and the 40 mm cube size to estimate position relative to the base. Missing, ambiguous or inconsistent observations request a stop. Each new sensor packet updates the command.
4. The evaluator independently reads base and cube poses, plus wheel, floor and obstacle contacts. It checks final range within 3.5 cm of the request, final bearing within 0.12 rad, actual base travel, three stationary observations, wheel support on every rollout tick, obstacle contacts and sampled joint limits. The evidence folder retains synchronized sensor packets and the evaluator trace. The model authoring a revision can receive the previous RGB-derived range and servo state.

## Measured on this Mac

| Provider and entry point | Development layout | Requested | Measured stop | Wheel-supported ticks | Authoring time |
| --- | --- | ---: | ---: | ---: | ---: |
| DeepSeek via Octos, Makepad offscreen | 0 · red | 45 cm | 46.81 cm | 460/460 | 1.88 s |
| Codex, Makepad headless | 0 · red | 45 cm | 46.81 cm | 460/460 | 7.13 s |
| DeepSeek via Octos, worker CLI | 1 · green | 42 cm | 43.70 cm | 520/520 | 1.61 s |
| DeepSeek via Octos, worker CLI | 2 · blue | 50 cm | 51.65 cm | 520/520 | 1.54 s |
| DeepSeek via Octos, revision from red trial | 0 · red | 41 cm | 42.71 cm | 500/500 | 1.59 s |

The two red-cube traces have the same SHA-256. Both models selected the same parameters and the local visual servo executed both trials. Each retained pair of successive Run-page observations was read while physics was active. Each native journey passed 30 checks plus two restart checks. All five runs recorded zero obstacle-contact ticks. The largest measured localization error across the three layouts was 7.9 mm. Authoring times include the CLI and network and come from one call per row. [Inputs, provider metrics, outcomes and trace hashes](nano-rgb-trials.json)

For the red revision, the operator asked to stop 4 cm closer than the previously requested distance. DeepSeek received the previous controller hash, target and RGB-estimated distance. It selected the same red cube and changed 45 cm to 41 cm. In a separate check, both providers rejected a request to pick up all three cubes and carry them to the table before issuing a controller.

The development trials start with the target in view on an unobstructed floor. They exercise visual approach and braking. The three-cube qualification still needs search under occlusion, grasp and lift on the mobile task path, table localization, release, settled placement and 50 sealed layouts. The separate test below measures a single physical lift. Physical Nano calibration and deployment remain separate.

## Reproduce

Follow the [Studio runtime setup](../README.md) and select an authenticated Octos DeepSeek profile when needed. From the Studio repository root, this command runs a new development layout through the same authoring and visual-servo code.

```sh
.venv-aisle/bin/python tools/aisle-validation/run_rgb_approach.py \
  --task "Approach the blue floor cube and stop 50 cm from it." \
  --seed 2 --out /tmp/nano-blue-approach
```

Use `--provider codex` for the matched authoring comparison. To replay the published red trial from the recorded sensor packets, run the following with a fresh directory. The verifier checks the robot model digest and compares every reconstructed servo decision against the recorded command.

```sh
replay_dir=$(mktemp -d)
unzip -q docs/product/aisle-studio/validation/nano-rgb-approach.zip -d "$replay_dir"
PYTHONPATH="$DOROBOT_AISLE_ROOT/src" .venv-aisle/bin/python tools/aisle-validation/verify_rgb_replay.py "$replay_dir"
```

## Single-cube test lift

The separate command-line experiment accepts an instruction to lift one visible floor cube with the Nano’s left arm. Octos and DeepSeek select the target and capability. The local controller aligns the base using fresh onboard RGB and encoders, solves the arm pose from the final RGB estimate and closes the fingers. The arm follows its planned trajectory during closure and lift. The judge independently measures physical cube height, opposing finger contacts and wheel support. This experiment has its own entry point.

In matched development trials on layout 0, DeepSeek and Codex each selected the red cube. Its lowest measured height across five samples during the one-second hold was 9.12 cm above its starting height. Both fingers touched the cube and wheels contacted the floor on all 100 hold ticks for both runs. Measured arm joints stayed inside the URDF position limits during interpolation. The base moved through the wheel motors and the object moved through contact. [Matched lift measurements](nano-rgb-lift-trials.json) · [Head camera during the hold](nano-rgb-lift-hold.png)

DeepSeek also selected and lifted green on layout 1 and blue on layout 2. Their minimum sampled lifts were 9.24 cm and 9.22 cm. Each trial started from a fresh scene.

The probe accepts a single lift and hold. Carrying a cube to the table, placing it and repeating the sequence for three objects are open qualification steps. To reproduce from the Studio checkout, install the arm solver dependency and run the following with your Octos profile configured.

```sh
uv pip install --python .venv-aisle/bin/python scipy==1.18.1
PYTHONPATH="$DOROBOT_AISLE_ROOT/src" .venv-aisle/bin/python tools/aisle-validation/check_rgb_lift.py \
  --task "Use the Nano left gripper to lift the red cube from the floor and hold it for one second." \
  --seed 0 --out /tmp/nano-red-lift
```

Add `--provider codex` and choose a fresh output directory to compare the same task. The output includes the selected controller, provider metrics, three onboard images taken during the hold, the measured lift and the contact counts.
