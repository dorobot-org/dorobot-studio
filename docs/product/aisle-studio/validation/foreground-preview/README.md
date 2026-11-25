# Nano live RGB in AISLE Studio

Prepare a Nano floor-to-table transfer on **Task** and start it on **Run** to see the simulated head camera. To watch one session in a fleet, open **Run → Workers**, connect to a worker and select **Stream run**. For the other sessions, choose a background RGB period and select **Background run**. The default one-second period requests one frame per second of simulated time. Select **Wall time** to schedule observations using elapsed time on the worker. The measured delivery rate depends on how quickly the simulation advances.

The selected foreground produces fresh 480×360 head-camera images on a 30 Hz wall-clock schedule when new physics ticks are available. A slow viewer can skip old images. The recorded episode still contains aligned head, left-wrist and right-wrist RGB at five frames per simulated second, joint states and commands. In Evidence, choose **Export episode** and **Open in Replay**. [Worker configuration](../worker-leases/README.md) · [Native foreground viewport](foreground.png) The camera heading in Run shows the measured native draw rate and flags a stalled live stream. [Read the display and reproduce the renderer comparisons](../viewport-cadence/README.md)

## Single-host capacity

Each session ran its own red-cube floor-to-table transfer on a 128 GiB Apple M3 Max with a development build of Studio. Each author produced a constrained task program selecting the red cube. The local AISLE controller performed navigation, grasp and placement from onboard RGB and encoders throughout the run. The foreground also recorded and exported three RGB cameras. Each background polled for fresh RGB and downloaded its worker evidence. The table counts only the interval when all sessions were advancing. The acceptance gate is a mean of at least 25 fresh viewport draws/second and at least one delivered image/second for **each** background over at least 30 seconds. The offscreen viewport was 1440×900. The four-session Codex headless instrument used 1024×768. The second-to-last column counts complete one-second intervals with fewer than 25 fresh draws.

| Sessions | Author and native backend | Overlap used (s) | Foreground FPS | Background FPS range | p95 / longest draw gap (ms) | Seconds below 25 FPS | Gate |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | --- |
| 4 | DeepSeek via Octos, offscreen | 60.6 | 28.82 | 1.09–1.10 | 54 / 218 | 1/60 | Pass |
| 4 | Codex, headless | 59.5 | 28.99 | 1.10–1.12 | 54 / 218 | 0/59 | Pass |
| 6 | DeepSeek via Octos, offscreen | 63.8 | 28.44 | 1.01–1.04 | 57 / 207 | 0/63 | Pass |
| 6 | Codex, offscreen | 63.7 | 28.66 | 1.01–1.03 | 57 / 182 | 1/63 | Pass |
| 7 | DeepSeek via Octos, offscreen | 69.0 | 28.76 | 0.94–0.96 | 58 / 204 | 1/69 | Miss |
| 8 | DeepSeek via Octos, offscreen | 76.1 | 28.39 | 0.84–0.88 | 58 / 215 | 2/76 | Miss |

Six sessions met the full gate with each author on this machine. Seven and eight missed the background target. The margins at six were small enough that other workloads can change the outcome. For measurements with wall-time scheduling and the same delivery target, see the [background RGB clock comparison](../wall-clock-rgb/README.md).

Frame rates count distinct Nexus exposure ticks drawn by the native Makepad video widget. They measure application draws. Capture-to-draw latency and display presentation require separate clocks or instrumentation. Offscreen mode also captures pixels. Physical display scanout, internet transport, remote-host capacity, and other task or camera profiles require separate measurements. Codex and DeepSeek authored controllers before each session executed its local sensor-driven policy. These timings measure the running simulations and Studio. [Timing records and outcomes](results.json)

Every transfer in the table completed with wheel support at all 6,848 physics ticks and the same recorded actuator targets. Each foreground recording reopened in Replay with 342 frames on each of the three cameras. The runs retained delayed draws, individual observation delivery times, hashes and outcomes. [Raw draw events, source exposures, agent measurements, journals and manifests](measurements.zip)

One foreground through the ordinary local Run page drew 28.47 FPS. The loopback OpenSSH reader received 2,221 of 2,222 exposure ticks at 29.45 FPS. The source timing log contains a repeated publication on the terminal physics tick. The count above uses distinct ticks. The reader recorded 2,220 distinct pixel hashes. A fresh exposure can produce identical pixels while the robot is still. The current worker advertises the persistent binary stream during connection. Older workers continue to serve periodic RGB snapshots. The earlier capacity benchmark measured that polling path. [Previous polling measurements](../worker-observations/local-capacity/README.md)

## Share one camera exposure at a physics tick

During a live Nano transfer, the head camera feeds the operator preview, recording, and policy. The wrist cameras can also be requested by the policy, recorder and grasp checks at the same tick. The live worker now renders each requested camera once per physics tick and shares those pixels. Physics advancing invalidates every cached view. The wrist timing check still controls which exposure can reach the policy. If another preview is due when the three-camera recording finishes, Studio can publish that tick’s head capture.

Two successive native journeys on the M3 Max used one foreground worker, the same DeepSeek-authored 80 mm/s red transfer, layout 0, offscreen Makepad at 1440 × 900, and the original 480 × 360 camera settings. Both placed red with 12,608 consecutive supported ticks. Across the pair, all 1,890 recorded camera PNGs, the entire motor journal, policy decisions, wrist delivery records, 17-channel action and encoder rows, and all three exported videos matched byte for byte. Both exports reopened after restarting Studio.

| Measurement | Prior worker | Shared exposures |
| --- | ---: | ---: |
| Source camera publications per second | 29.62 | 29.92 |
| Fresh frames drawn per second | 28.51 | 29.43 |
| Full one-second intervals below 25 FPS | 2 / 140 | 0 / 135 |

The revised worker served 6,072 camera reads with 5,775 renders. It reused 111 head, 128 left-wrist and 58 right-wrist exposures. These rates describe one paired foreground run on Metal. [Native draws, exposure timestamps, recorded-file digests and motor journals](exposure-reuse-metal.zip) · [Recalculate the comparison or capture a new pair](compare_exposures.py).

A separate pair held the delivered left-wrist image at tick 1,080 while physics and the other cameras continued. Both live workers rejected ten stale deliveries and stopped the pickup at tick 1,280. Their motor journals, decisions, sensor delivery logs and all 192 saved images matched. [Stopped trials and original capture and delivery timestamps](exposure-reuse-disconnect.json)

Run `python3 compare_exposures.py exposure-reuse-metal.zip` from this directory. To compare fresh runs, pass a new archive path followed by `--pack BASELINE_NATIVE_DIR CANDIDATE_NATIVE_DIR`. Each directory must contain a completed and restarted `--fleet --fleet-node ... --fleet-controller ...` journey. The checker reads each saved camera file from the original workspaces when packing a pair.

## Ten sessions with shared camera exposures

On the 128 GiB M3 Max, each author prepared ten independent red-cube transfers. One worker streamed its 480 × 360 head camera to the 1440 × 900 offscreen Makepad viewport. The other nine delivered a fresh onboard RGB image on a 0.5-second wall-clock schedule. The authors ran in separate batches. The two batches used the same Studio binary and runtime source files.

| Controller author | Time with all ten advancing | Native foreground | Slowest background | Full seconds below 25 FPS | Transfers passed |
| --- | ---: | ---: | ---: | ---: | ---: |
| DeepSeek V4 Flash via Octos | 93.27 s | 28.59 FPS | 1.990 Hz | 1 / 93 | 10 / 10 |
| Codex GPT-6.2 Astra | 92.24 s | 28.47 FPS | 1.993 Hz | 0 / 92 | 10 / 10 |

All 20 transfers maintained wheel support for 6,848 physics ticks. Both foreground datasets reopened after a Studio restart with 342 frames per camera. This is a capacity test of one stationary red transfer per session on the same layout. Provider calls prepared the program before simulation. [Per-receiver rates and exact source revisions](exposure-reuse-ten.json) · [Native draw times, author receipts, worker observations and motor journals](exposure-reuse-ten.zip)

Run `python3 ../transfer-wrist-timing/capacity.py exposure-reuse-ten.zip` here with NumPy installed to recalculate the table. Add `--git-sources` in a full checkout to compare the recorded source hashes with Git.

## One H100 foreground worker

A single registered Linux worker on an NVIDIA H100 NVL ran the frozen DeepSeek red-cube controller with the default room and recorded all three cameras. The worker published **6,948** distinct head-camera frames at **18.18 FPS**, and the Mac's native Makepad viewport displayed **6,834** at **17.89 FPS**. All 381 complete one-second viewport intervals fell below 25 FPS. At the source's measured rate, even displaying every published frame would fall short of the target. This run used one GPU for the Nano with other users working on the host.

The cube settled 104 mm inside the table edge. Wheel support held for all 6,948 ticks and 316 same-tick camera reads reused an exposure. Studio imported the original worker archive, exported 347 frames per camera and reopened the result after restart. The offline check compares the actual capture ticks, native draw timestamps, controller and motor hashes, recording and reopened verdict. [Measurements](h100-single.json) · [Native session and worker records](h100-single.zip)

The original viewer parsed the cube's X coordinate one binary64 step away from the worker's value. A regression test reproduced the discrepancy on reopening. With correctly rounded JSON parsing enabled, native headless Studio reopened the same H100 result in Evidence and reproduced every recorded value exactly. The archive preserves the original discrepancy and the corrected native result.

```sh
python3 docs/product/aisle-studio/validation/foreground-preview/compare_exposures.py \
  --single docs/product/aisle-studio/validation/foreground-preview/h100-single.zip
```

## Reproduce the measurements

From the Studio checkout, follow the build, AISLE Python environment and author credential instructions in the [workbench guide](../../README.md). Use a new output directory each time. These commands open the actual Studio binary, send Makepad input events, start independent worker jobs, export and reopen the foreground episode, and restart Studio.

```sh
cargo build --locked -p dorobot-flex --bin dorobot-flex
.venv-aisle/bin/python tools/aisle_workbench/verify.py \
  --studio target/debug/dorobot-flex --backend offscreen --size 1440x900 \
  --fleet --foreground-cadence 4 --octos-deepseek --out /tmp/aisle-four-offscreen
.venv-aisle/bin/python tools/aisle_workbench/verify.py \
  --studio target/debug/dorobot-flex --backend headless --size 1024x768 \
  --fleet --foreground-cadence 4 --codex --out /tmp/aisle-four-headless
python3 docs/product/aisle-studio/validation/foreground-preview/audit.py
```

Change the session count to test another load. Each session consumes one real author request. The audit command requires only Python’s standard library and the archived measurements. It checks the reported frame rates, common observation window, per-session task and motor verdicts, exported frame counts and SSH exposure ticks.

To regenerate the source files for the ordinary Run page and loopback OpenSSH transport, run the commands below in the same checkout. The SSH check starts a temporary key-only server bound to loopback and restricts it to the selected worker’s RGB stream. It needs local OpenSSH client and server executables. Its keys and SSH configurations live in a temporary directory.

```sh
proof=docs/product/aisle-studio/validation/foreground-preview
.venv-aisle/bin/python tools/aisle_workbench/verify.py \
  --studio target/debug/dorobot-flex --backend offscreen --size 1440x900 \
  --transfer red --octos-deepseek --out /tmp/aisle-direct-red
.venv-aisle/bin/python "$proof/loopback_ssh.py" /tmp/aisle-ssh-red \
  /tmp/aisle-four-offscreen/state/aisle/*-Nano-prepare/controller.json
python3 "$proof/archive.py" --case /tmp/aisle-four-offscreen --case /tmp/aisle-four-headless \
  --direct /tmp/aisle-direct-red --ssh /tmp/aisle-ssh-red --out /tmp/aisle-new-evidence
python3 "$proof/audit.py" /tmp/aisle-new-evidence
```

Add a `--case` directory for each additional load. The archive builder uses FFprobe on all three exported videos and verifies worker archive and actuator hashes. It retains the raw stream and draw timestamps so the audit can recompute averages, percentile gaps and the one-second intervals.
