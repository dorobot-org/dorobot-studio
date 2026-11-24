# Nano wrist-camera loss during a pick

Open **AISLE Studio → Task**, select **Nano**, and enter “Move all three floor cubes onto the coffee table in red, green, blue order.” Choose **Ask Codex** or **DeepSeek via Octos**. On **Scene**, select **On time**, **Delay** or **Freeze** under **Left wrist RGB during pick**. On **Run**, start the simulation. **Evidence** shows whether the wrist resumed or timed out and whether each cube settled on the tabletop. Restarting Studio reopens the recorded outcome. [Timeout in Studio](timeout.png)

The local controller checks the left-wrist RGB stream at least every 20 simulated pick ticks and at each request for a synchronized policy observation. The guard uses the pick phase declared by the controller over the action pipe. Its clock and packet-delivery timestamps are in simulation time. The tested interruption starts during the first pick, while Nano is reaching for red. Green and blue receive normal wrist delivery. All 17 wheel, arm, gripper and head position targets are held while an exposure is stale. Nexus continues advancing at 10 ms per tick. No simulator object coordinates enter the policy. The independent judge measures physical contacts, support, joint limits and settled placement. The agent authors the task before the run. The local controller operates the motors. For the recorded three-cube native journeys, Codex (GPT-6.2 Astra) took 7.51 seconds to prepare its instruction and DeepSeek V4 Flash through Octos took 2.55 seconds. Each figure covers one authoring request. Physics and motor decisions then ran locally.

The previously qualified controller scored 45/50 on its own frozen runtime. The layout-0 measurements below exercise fault handling. The release policy in this experiment has its own [50-layout commitment](qualification-manifest.json). [Previous Nano qualification](../nano-holdout-d/README.md)

## Layout 0

| Wrist delivery | What the sensor delivered | Command and physical outcome |
| --- | --- | --- |
| On time | Increasing capture timestamps | The original three-cube controller placed all three in 23,744 ticks. The revised release policy placed all three in 23,724. |
| Delay | Three repeated exposures, then five exposures 400 ms old | All targets held from tick 1,000 to 1,160. The original controller placed red and green. Blue came to rest on green. The revised policy placed all three independently in 26,462 ticks. |
| Freeze | Ten repetitions of the last accepted exposure | The robot held its last targets and ended the pick at tick 1,180. No cube was placed. Two simulated seconds elapsed since the last accepted exposure. |

The measured left arm continued settling during the commanded hold. The largest recorded encoder span was 0.1433 rad (8.21°). At the last recorded hold sample, the largest error from the target was 0.0092 rad (0.53°). All three-cube repeat runs maintained wheel support at every physics tick and recorded zero joint-limit excursions. These measurements describe the simulated Nano. Physical stopping distance and hardware watchdog timing require a measured device.

### Why the original blue placement failed

In the original delayed run, the head and right-wrist views yielded zero tabletop occupants. The gripping wrist could see both earlier cubes. Projecting the recorded RGB and measured encoders at tick 24,260 put green about 25 mm from the selected release location. That exposure was eight ticks after the policy's decision. The revised policy also checks the gripping wrist for prior colors and retains partially visible cubes. It selected a reachable release point about 67 mm away from the original target. The same RGB projection put green 85.9 mm from the revised point. An object clipped by the image boundary has an approximate center. The physical judge checks where each released cube actually comes to rest. Blue then remained in direct tabletop contact for the final 100 ticks with 92 mm of edge clearance. [Onboard wrist exposure](blue-release-wrist.png)

Two early three-cube instrument runs reached the blue pick before their 330-second harness limit expired. They produced no physical verdict. The revised native harness allows enough time for all three complete episodes. [Original instrument outcomes](instrument-timeout.json)

Codex and DeepSeek each reproduced all three revised three-cube outcomes through native Studio. Across the two providers and the direct worker, all three matched motor journals for each fixture. Both instruments restored the timeout after restart. A subsequent compact run switched Scene back to the enabled **On time** choice. [Rendered three-cube evidence](three-cube-recovery.png) · [Reset after restart](restore-on-time.json)

Both providers independently authored the single-red instruction. Their three command journals matched byte-for-byte across the two native backends, including the journal for the sustained freeze. The unchanged normal red run also reproduced the earlier 6,848-tick journal. The worker compatibility check rejected a transfer timing request on an older worker before lease allocation. A current worker returned a completed lease containing the failed task and timeout evidence. [Protocol](protocol.md)

## Dataset export and worker compatibility

**Evidence → Export episode** preserves the raw camera videos, 17 encoder values and 17 commanded positions. The LeRobot field `annotation.wrist_rgb_hold` marks frames in which the controller is waiting for a fresh wrist exposure. The capture and delivery timestamps and their pixel digests are in `meta/wrist-delivery.jsonl`. The metadata names the video source and binds the log digest to the episode. The revised delay dataset contains 1,323 frames per camera, including eight hold frames. The timed-out dataset has 59 frames per camera, including ten hold frames.

An older worker that lacked the transfer capability still completed its supported delayed visual-approach task. Its independent judge recorded a final cube range of 0.468 m and full wheel support. A completed remote fault trial is checked again during import against its original lease, source fixture and structured wrist verdict. The current Studio archive reader also replayed the previous 50-layout Nano qualification and recovered all 50 receipts and the 45 successful trials. Comparison in Evidence requires matching wrist-timing implementations and matching sensor fault modes.

## Reproduce the native experiments

From the Studio checkout, use the AISLE Python environment, an instrumented Studio binary and Makepad Remote. Give each run a fresh output directory.

~~~sh
"$DOROBOT_AISLE_PYTHON" tools/aisle_workbench/verify.py \
  --studio "$studio_binary" --remote "$makepad_remote" \
  --backend headless --size 1024x768 --codex \
  --wrist-faults three --out "$fresh_run"
~~~

Select `--backend offscreen --size 1440x900 --octos-deepseek` for the rendered desktop journey. Select `--wrist-faults red` for the single-cube experiment. [Studio setup and provider configuration](../../README.md)

The measurement archive contains the original failure, paired three-cube repeats, native journeys, source manifests, motor journals, wrist logs, original exposures and both exported datasets. From the same checkout, verify the measured arm positions and contact outcomes, decode all six dataset videos and recompute the RGB projection shown above.

~~~sh
guide=docs/product/aisle-studio/validation/transfer-wrist-timing
"$DOROBOT_AISLE_PYTHON" "$guide/audit.py" "$guide/measurements.zip" --datasets
"$DOROBOT_AISLE_PYTHON" "$guide/release_check.py" "$guide/measurements.zip"
~~~

The archive SHA-256 is `d2a7ff81e76c3b13819385b30069b1e435a696a186fcb049f1e2eaeba2e829aa`. Add `--git-sources` to check the source manifests against the measured commits. [Measured outcomes](measurements.json) · [Original trials and datasets](measurements.zip) · [RGB release distances](release-distances.json)
