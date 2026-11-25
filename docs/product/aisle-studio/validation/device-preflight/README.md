# Run a G1 controller against measured joint and IMU inputs

Open **AISLE Studio → Evidence → Device preflight** after completing a G1 standing-reach trial. Studio binds the controller, model and simulation result to that trial. It can replay the recorded Nexus sensors over Unitree DDS or subscribe to an existing telemetry publisher. After capture, the same standing controller computes its 29 joint targets at 200 Hz from encoders and IMU orientation. The preflight sends zero actuator commands. The panel shows source and receiver gaps separately and reports the largest difference between a proposed target and the measured joint. These offline measurements establish neither live control latency nor physical tracking performance. Hardware identity, calibration, monitored stopping and the physical task outcome require a separate test with a robot and its operator.

## Configure telemetry

First complete the [Studio and AISLE setup](../../README.md#run-from-a-studio-source-checkout). The DDS transport uses its own Python environment. The following versions were tested on Apple Silicon with CMake, a C compiler and uv. Run this once from a directory where you want to keep the SDK and its dependencies. [Unitree SDK installation](https://github.com/unitreerobotics/unitree_sdk2_python/tree/814556d15970dd2ecf1c9984e845ca02ab07e206) · [G1 motor indices and HG LowState subscription](https://github.com/unitreerobotics/unitree_sdk2_python/blob/814556d15970dd2ecf1c9984e845ca02ab07e206/example/g1/low_level/g1_low_level_example.py)

```sh
git clone --branch 0.10.5 --depth 1 https://github.com/eclipse-cyclonedds/cyclonedds.git cyclone-src
cmake -S cyclone-src -B cyclone-build -DCMAKE_POLICY_VERSION_MINIMUM=3.5 \
  -DCMAKE_INSTALL_PREFIX="$PWD/cyclone-install" -DBUILD_EXAMPLES=OFF \
  -DBUILD_TESTING=OFF -DENABLE_SSL=NO -DENABLE_SHM=NO
cmake --build cyclone-build --target install --parallel 4
git clone https://github.com/unitreerobotics/unitree_sdk2_python.git unitree-sdk2
git -C unitree-sdk2 checkout --detach 814556d15970dd2ecf1c9984e845ca02ab07e206
uv venv --python 3.11 g1-dds-venv
export DOROBOT_G1_DDS_PYTHON="$PWD/g1-dds-venv/bin/python"
CYCLONEDDS_HOME="$PWD/cyclone-install" CMAKE_POLICY_VERSION_MINIMUM=3.5 \
  uv pip install --python "$DOROBOT_G1_DDS_PYTHON" ./unitree-sdk2
```

Launch Studio from the shell containing that interpreter setting.

1. Select **G1** on Task. Request a standing reach under bounded impacts, choose the impact fixture on Scene and record the trial on Run. Select the successful trial in Evidence. Older recordings may require a new simulation to bind the controller implementation and retain its inputs.
2. Choose **Replay through DDS** to transmit the recording as Unitree HG LowState messages on loopback. The receiver records raw telemetry, then evaluates the controller and compares every joint target against the originating simulation. The result gives the maximum difference in radians.
3. For an existing G1 telemetry source, enter the local robot-network interface and domain in the form `en0@0` (use the actual settings for your robot). Choose **Observe device**. The receiver subscribes to `rt/lowstate` in HG pitch/roll motor layout. Allow 11 seconds of telemetry. **Stop** cancels the job. A missing publisher, changed mode or invalid measurement yields a stopped result. An external publisher can also be a simulator. Record its physical identity separately during hardware qualification.
4. Choose **Open preflight folder** for the raw timestamped encoders, velocities and IMU, shadow targets, selected controller and receipt. Reopening the trial after restarting Studio restores its latest receipt.

For a Nano trial, **Check adapter** reads the installed Feetech profile and lists the channels needed by the selected controller. Shoulder inspection needs arm actuation and measured joint positions. A visual approach also needs the mobile base, head actuation and three calibrated RGB feeds. The approach controller holds the head and arms in their observation posture and uses joint feedback to locate its cameras. A floor-to-table transfer additionally uses measured wheel odometry. The current adapter exposes three wheel command channels. The preflight receipt names each missing capability and records zero motor commands. [Nano adapter profile](../../../../../robots/adora_nano_feetech/profile.json)

## Transport measurements

Studio's worker checked the original controller and model from two successful simulated Nano trials against the installed profile. It reported the four missing visual-approach channels and the additional wheel odometry channel for the transfer. Both dry runs sent zero motor commands. [Bound receipts for both controllers](nano-channels.json)

The reference episode contains 200 warmup and 2,000 scored samples. Each path used two separate processes on this Mac. The external subscription test used a separately launched publisher of the same recorded Nexus measurements. Both produced the same saved joint-target array, with zero measured joint-limit and velocity-limit excess. All 29 motor slot names were independently compared with the pinned SDK example. The physical encoder offsets, body IMU orientation, board identity and stop remain unmeasured. Peak target-to-joint separation in both recordings was 0.504 rad. [SDK slot comparison](sdk-order.json)

| Capture path | Messages × joints | Largest source gap | Largest receive gap | Largest target difference vs. originating simulation |
| --- | ---: | ---: | ---: | ---: |
| Studio worker starts loopback publisher | 2,200 × 29 | 5 ms | 7.464 ms | 0 rad |
| External subscriber, independently launched loopback publisher | 2,200 × 29 | 5 ms | 9.889 ms | Same target array as the first capture |

[Loopback receipt](loopback.json) · [External subscription receipt](external-subscriber.json) · [Raw telemetry, shadow targets and originating episode](g1-telemetry.zip)

To reproduce the comparison, unpack the evidence into a new directory. The [tested source revision](https://github.com/dorobot-org/dorobot-studio/tree/25b90ae76a7f5c874009ac119f7214ac7929db36) pins the controller and model for this recording. From that checkout, run the verifier with the configured AISLE Python. The verifier checks stored hashes and recomputes every target and the input sample selected at every controller step.

```sh
proof_dir=$(mktemp -d)
unzip -q docs/product/aisle-studio/validation/device-preflight/g1-telemetry.zip -d "$proof_dir"
for capture in g1-device-shadow-r2 g1-device-external-r1; do
  .venv-aisle/bin/python tools/aisle_workbench/device_preflight.py \
    "$proof_dir/$capture" "$proof_dir/device-ref-standing-r1"
done
```

## Standing reach regression

The controller now shares its feedback calculation between Nexus and telemetry replay. We reran the 30° left waist turn and −0.08 rad right shoulder reach on the last 20 published impact recipes, then on eight envelope corners and twelve new interior recipes. A separate run repeated that same fresh set. Each group passed 20/20 with zero measured guard bypasses. Both feet remained loaded at all 120,000 scored physics steps. The worst settled wrist error across the three groups was 4.987 mm. [Cohort measurements and archive digests](cohorts.json)

| Impact set | Preregistration | Raw motor, wrist and contact traces |
| --- | --- | --- |
| Previously disclosed | [Matched manifest](matched-suite.json) | [20 trials](matched.zip) |
| Fresh | [Fresh manifest](fresh-suite.json) | [20 trials](fresh.zip) |
| Fresh, repeated | [Repeat manifest](repeat-suite.json) | [20 trials](repeat.zip) |

Run `g1_reach_suite.py verify <archive> <manifest-sha256>` with the AISLE Python from `tools/aisle_workbench`. The verifier opens the sealed per-case recipes and recomputes the outcome from the retained traces.

## Native Studio verification

Codex authored the G1 trial in the light 1440 × 900 offscreen app. DeepSeek through Octos authored it in the dark 1024 × 768 headless app. Each captured a stopped result for a missing external publisher, cancelled a running DDS capture, verified the publisher and subscriber process group had exited, replayed the originating Nexus recording over DDS, exported the simulation episode to Replay, restarted Studio and recovered the receipt for the selected trial. The two captures each reproduced all 63,800 motor targets exactly. [Rendered G1 preflight](g1-preflight.png) · [Native journeys and retained receipts](native.json)

The dark desktop also completed a Codex red-only transfer and a DeepSeek three-cube transfer in Nexus. Each opened the Feetech capability report, exported the episode, then restored the report with its trial after a restart. [Nano adapter view](nano-adapter.png)
