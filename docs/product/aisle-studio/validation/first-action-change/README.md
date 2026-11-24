# Replay the first changed Nano action

On living-room layout 0, two Nano policies received the same instruction and the same input at the blue pickup. The earlier policy placed red and green, then stopped with two plausible blue targets in its head camera. The revised policy used its left wrist camera to select the target and placed all three cubes. The base stayed supported throughout both runs. [Camera inputs at the decision](decision-cameras.png) · [Why the association changed](../moving-grasp-wrist-agreement/README.md)

## Open the recorded comparison

The demo workspace includes both LeRobot episodes, all three camera videos for each episode, the action and motor journals, and both original decision packets. From a Studio checkout with the AISLE environment set up, open it in an isolated session. [Studio setup](../../README.md) · [Demo workspace](demo-workspace.zip)

```sh
demo_dir="$(mktemp -d)"
unzip -q docs/product/aisle-studio/validation/first-action-change/demo-workspace.zip -d "$demo_dir"
DOROBOT_STUDIO_STATE_DIR="$demo_dir" cargo run --locked -p dorobot-flex --bin dorobot-flex -- --screen aisle-evidence
```

1. On **Evidence → Trial**, select the newer Nano run. The comparison identifies the first changed action as **1,148**. The previous trace ended after sensing at physics tick **17,814**. The current trace recorded a blue grasp observation at the same tick. [Evidence screen](paired-evidence.png)
2. Choose **Inspect decision RGB** to see the head, left-wrist and right-wrist images that each policy received at **178.14 simulated seconds**. Studio checks the packet timestamp, camera shapes, channels and encoder vector. It checks the stopped run’s saved packet against its receipt and associates the continuing run’s packet with the recorded grasp action. The two sets of image bytes, all 17 encoders and the exposure timestamp matched exactly.
3. Choose **Trial**, then **Replay previous** or **Replay current**. Both open paused on **frame 889**, captured at physics tick **17,800**. The recording sampled every 20 ticks, so this exposure precedes the decision by 140 ms. The exported video begins at its first recorded exposure, giving this frame a video timestamp of **177.8 seconds**. Press Play in the current episode to follow the revised grasp. A newly recorded trial first offers **Export previous** or **Export current** in the same location. Exporting the previous run keeps the current trial selected. [Previous replay](replay-previous.png) · [Current replay](replay-current.png)

| Measured outcome | Previous policy | Revised policy |
| --- | ---: | ---: |
| Settled cubes | 2/3 | 3/3 |
| Supported / executed physics ticks | 17,814 / 17,814 | 23,596 / 23,596 |
| Recorded frames per onboard camera | 890 | 1,179 |
| Maximum measured joint-limit excursion | 0 rad | 0 rad |

The archived motor command values were identical from the initial targets through the entire earlier run. The experiment reused the same prepared task, URDF, evaluator, camera calibration, negative slide and layout seed. The calibration receipt records each revision’s association method for offline reconstruction. The Nexus judge applies the same physical scoring in both runs. Run `python3 docs/product/aisle-studio/validation/first-action-change/audit.py` to verify the archive checksum and independently compare the recorded actions and motor commands. These are two executions on one disclosed layout. [Controller](controller.json) · [Native headless and offscreen results, restart receipts and source hashes](native.json)

## Record and verify a fresh pair

Use a full Git checkout containing historical policy revision `b2de5f2c7a5ef8c3bf03f5a6976ba9db038329ae`. Configure the same Nexus and AISLE runtime used by Studio. From the repository root, run the following with fresh output directories. The script runs both real simulations on the current evaluator, using a temporary copy of the earlier policy for the first episode. The published workspace already contains the exported episodes. Re-recording also produces the full per-exposure PNG sequences for a new export.

```sh
proof=docs/product/aisle-studio/validation/first-action-change
.venv-aisle/bin/python "$proof/record_pair.py" /tmp/aisle-decision-recordings
cargo build --locked -p dorobot-flex --bin dorobot-flex
.venv-aisle/bin/python tools/aisle_workbench/verify.py \
  --studio target/debug/dorobot-flex --backend offscreen --size 1440x900 \
  --compare-runs /tmp/aisle-decision-recordings/0001-Nano-run /tmp/aisle-decision-recordings/0002-Nano-run \
  --out /tmp/aisle-decision-offscreen
```

For the compact instrument, use `--backend headless --dark --size 1024x768` and a new output directory. Both instruments independently read the original traces and recording manifests, compare each displayed RGB plane with the saved sensor packet, export both trials and inspect the native Replay frame after restart. When the action journals match, Evidence shows the matching action count. A missing camera packet or episode leaves only the available evidence accessible.
