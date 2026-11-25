# Nano wrist-camera loss during a pick

Open **AISLE Studio → Task**, select **Nano**, and enter “Move all three floor cubes onto the coffee table in red, green, blue order.” For the experimental three-view placement used in the recovery demonstration, add the separate sentence **Use experimental wrist clearance.** Ordinary requests and existing controller artifacts use the established head and right-wrist release check. Choose **Ask Codex** or **DeepSeek via Octos**. On **Scene**, select **On time**, **Delay** or **Freeze** under **Left wrist RGB during pick**. On **Run**, start the simulation. **Evidence** shows whether the wrist resumed or timed out and whether each cube settled on the tabletop. Restarting Studio reopens the recorded outcome. [Timeout in Studio](timeout.png)

The local controller checks the left-wrist RGB stream at least every 20 simulated pick ticks and at each request for a synchronized policy observation. The guard uses the pick phase declared by the controller over the action pipe. Its clock and packet-delivery timestamps are in simulation time. The tested interruption starts during the first pick, while Nano is reaching for red. Green and blue receive normal wrist delivery. All 17 wheel, arm, gripper and head position targets are held while an exposure is stale. Nexus continues advancing at 10 ms per tick. No simulator object coordinates enter the policy. The independent judge measures physical contacts, support, joint limits and settled placement. The agent authors the task before the run. The local controller operates the motors. For the recorded three-cube native journeys, Codex (GPT-6.2 Astra) took 7.51 seconds to prepare its instruction and DeepSeek V4 Flash through Octos took 2.55 seconds. Each figure covers one authoring request. Physics and motor decisions then ran locally.

The previously qualified controller scored 45/50 on its own frozen runtime. The layout-0 measurements below exercise fault handling. The experimental release policy in this experiment completed 40/50 on its own fresh cohort. It remains opt-in. [50-layout commitment](qualification-manifest.json) [Previous Nano qualification](../nano-holdout-d/README.md)

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

## Fresh layout qualification and release decision

**40/50** complete three-cube transfers passed with the experimental planner (80%, 95% Wilson interval 67.0–88.8%). The preregistered gate requires 45. All 50 motor journals verified across 1,347,106 physics ticks with zero guard bypasses and zero measured joint-limit excursions. The 13,928 normal wrist observations all had matching capture and receive ticks and matching source and delivered pixel hashes. The runtime retains the established release behavior for ordinary instructions and saved controllers. The operator can explicitly select the three-view policy to reproduce the recovery experiment. This gate applies to manipulation performance. The command-hold and timeout have separate measurements. [Committed inputs](qualification-manifest.json) · [Every scored layout](qualification-report.json) · [Verified totals](qualification.json) · [Full cohort archive](qualification.zip)

In **Evidence → Batch evaluation**, Studio displayed “Qualification failed,” the 45-case requirement and all ten failed layouts. Desktop and compact Makepad instruments opened the next failure, returned to the first, retained the verified score after an unreadable archive, and restored it after restart. [Desktop Evidence page](cohort-e-evidence.png) · [Native verification records](native-evaluations.zip)

| Failed cases | Measured outcome |
| --- | --- |
| 9, 15, 16, 19, 36 | No clear release position satisfied the arm reach and observed occupancy checks. Case 16 also lost one wheel-support tick. |
| 4, 31 | All three cubes settled. Wheel support was lost for three ticks and one tick respectively. |
| 10 | Blue docking timed out with robot–table contact. |
| 20, 33 | The controller lost blue and red respectively during transport. |

### Two-view and three-view release on the same 50 layouts

Both planners completed **40/50** with normal camera delivery on the same 50 living-room layouts. The experimental run was the preregistered hidden cohort E. After its layouts were disclosed, we ran the two-view policy once per seed using the same frozen controller, robot, physics and wrist monitor. The earlier 45/50 qualification used cohort D and the same high-level controller. The runtime changed between D and E, including the wrist monitor, transfer execution and perception sources. The 45/50 qualification applies to the recorded D build. [Two-view manifest](matched-baseline-manifest.json) · [Two-view trials and motor records](matched-baseline.zip) · [Verified outcome for every pair](matched-comparison.json) · [Earlier qualification](../nano-holdout-d/README.md)

| Outcome on the same layout | Layouts | What changed |
| --- | ---: | --- |
| Both passed | 37 | All three cubes settled on the tabletop and the robot met the motion criteria. |
| Only two-view passed | 3 | On layouts 9, 19 and 36 the extra occupancy check found no reachable clear release. |
| Only three-view passed | 3 | The two-view policy stacked green on red on layout 34, and blue on green on layouts 40 and 49. Each upper cube finished about 40 mm above the lower cube, with zero direct tabletop contact in the final 100 ticks. |
| Both failed | 7 | Both runs retain their individual physical measurements and failure reasons. |

All 100 motor records verified across 2,720,952 physics ticks with zero measured guard bypasses. The 17-target motor journals were byte-identical on 21 of the 50 paired layouts. One pair also had identical high-level action logs, which record observation requests, phase changes, wheel commands and other policy operations. The physics trials ran on the Mac. The Mac and a Linux CPU independently replayed those saved motor records and produced the same comparison file (SHA-256 `690f8f8db8d802fbf5dfb2dfe52c1d10dae5d3773fb709f262d7f6197543a643`). The compact native Evidence page also opened the disclosed two-view suite and restored its development result after restart. The six changed verdicts explain the tradeoff behind the opt-in release decision. These 50 pairs compare the two frozen policy builds. The later operator-selectable build was exercised separately on layout 0 under all three delivery fixtures. [Selection runs](#verify-the-operator-selection)

## Mac capacity with the wrist monitor running

On the 128 GiB M3 Max, one native 480 × 360 viewport and nine background Nano transfers ran concurrently. Each observer requested a fresh image every 0.5 seconds of wall time. Each provider authored ten controllers. All 20 single-red transfers completed, and both foreground episodes exported 342 frames per camera. The camera monitor was active during every pick. The trials used the registered pre-selection build and the same single-red motor journal across all sessions.

| Author | Full overlap | Foreground mean | Nine observer receipt rates | Complete seconds below 25 FPS |
| --- | ---: | ---: | ---: | ---: |
| DeepSeek through Octos | 94.0 s | 27.65 FPS | 1.998–2.003 Hz | 3/93 |
| Codex | 88.3 s | 27.82 FPS | 1.990–2.004 Hz | 1/88 |

Both repeats met the preregistered mean throughput gate. [Measurements](capacity.json) · [Original draws, worker samples and motor journals](capacity.zip)

## Verify the operator selection

Both native authors bound the explicit request to `wrist_clearance: true` in the saved controller. The nine post-selection trials covered the default worker, Codex headless and DeepSeek offscreen under each delivery fixture. For the three default runs the 17-target motor journals and all wrist deliveries matched the original two-view runs byte for byte. For each author’s three opt-in runs they matched the corresponding three-view trials. The default placed all three on time and two after the delay on layout 0. The opt-in mode placed all three in both conditions. Both modes stopped on the permanent freeze. [Structured outcomes and author receipts](selection.json) · [Original selection records](selection.zip) · [Current native Evidence page](three-cube-recovery.png)

## Dataset export and worker compatibility

**Evidence → Export episode** preserves the raw camera videos, 17 encoder values and 17 commanded positions. The LeRobot field `annotation.wrist_rgb_hold` marks frames in which the controller is waiting for a fresh wrist exposure. The capture and delivery timestamps and their pixel digests are in `meta/wrist-delivery.jsonl`. The metadata names the video source and binds the log digest to the episode. The revised delay dataset contains 1,323 frames per camera, including eight hold frames. The timed-out dataset has 59 frames per camera, including ten hold frames.

An older worker that lacked the transfer capability still completed its supported delayed visual-approach task. Its independent judge recorded a final cube range of 0.468 m and full wheel support. A completed remote fault trial is checked again during import against its original lease, source fixture and structured wrist verdict. In a separate local worker-service test, Studio rejected the experimental release option on an older worker before allocating a lease. A compatible worker accepted it. Studio imported the failed grasp with the original ten stale wrist exposures and 1,180 physics ticks. [Worker receipt and local lease index](worker-selection.json) · [Imported controller, origin and motor evidence](worker-selection.zip) The current Studio archive reader also replayed the previous 50-layout Nano qualification and recovered all 50 receipts and the 45 successful trials. Comparison in Evidence requires matching wrist-timing implementations and matching sensor fault modes.

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
"$DOROBOT_AISLE_PYTHON" "$guide/selection.py" "$guide/selection.zip"
"$DOROBOT_AISLE_PYTHON" "$guide/qualification_check.py" "$guide/qualification.zip"
"$DOROBOT_AISLE_PYTHON" "$guide/capacity.py" "$guide/capacity.zip" --git-sources
~~~

To replay all 100 motor journals and compare the release policies by layout seed, use the AISLE Python environment. The verifier was tested at Studio commit `65926336ccac2645a8d512c204b984254435d518`. The command compares the replay with the published result byte for byte.

~~~sh
guide=docs/product/aisle-studio/validation/transfer-wrist-timing
(cd "$guide" && shasum -a 256 -c matched.sha256)
verifier=$(mktemp -d)
git archive 65926336ccac2645a8d512c204b984254435d518 \
  tools/aisle-validation tools/aisle_workbench tools/nano-three-cubes \
  data/adora_nano/adora_nano.urdf | tar -x -C "$verifier"
comparison="$verifier/comparison.json"
"$DOROBOT_AISLE_PYTHON" "$verifier/tools/aisle-validation/nano_suite_evidence.py" compare \
  "$guide/matched-baseline.zip" "$guide/qualification.zip" \
  --baseline-manifest 4f48725017d38217788b90704c12383e63de7bf03263fc9d867de1784d6194dc \
  --candidate-manifest 7a344127f816c49c53d12410516fd04f136ea471117cd3e9a68a8fddf126a978 > "$comparison"
cmp "$guide/matched-comparison.json" "$comparison"
~~~

On Linux, `sha256sum -c matched.sha256` performs the same checksum verification. To inspect either archive in Studio, copy it into its own folder together with the corresponding public manifest named `suite.json`. Open **Evidence → Batch evaluation**, enter the archive path, and choose **Verify evaluation**.

The measurement archive SHA-256 is `d2a7ff81e76c3b13819385b30069b1e435a696a186fcb049f1e2eaeba2e829aa`. Add `--git-sources` to check the source manifests against the measured commits. [Measured outcomes](measurements.json) · [Original trials and datasets](measurements.zip) · [RGB release distances](release-distances.json)
