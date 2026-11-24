# Nano live RGB in AISLE Studio

Prepare a Nano floor-to-table transfer on **Task** and start it on **Run** to see the simulated head camera. To watch one session in a fleet, open **Run → Workers**, connect to a worker and select **Stream run**. For the other sessions, choose a background RGB period and select **Background run**. A one-second period requests one frame per second of simulated time. The measured delivery rate depends on how quickly the simulation advances.

The selected foreground produces fresh 480×360 head-camera images on a 30 Hz wall-clock schedule when new physics ticks are available. A slow viewer can skip old images. The recorded episode still contains aligned head, left-wrist and right-wrist RGB at five frames per simulated second, joint states and commands. In Evidence, choose **Export episode** and **Open in Replay**. [Worker configuration](../worker-leases/README.md) · [Native foreground viewport](foreground.png)

## Single-host capacity

Each session ran its own red-cube floor-to-table transfer on a 128 GiB Apple M3 Max. Each requested an actual controller from the named author. The foreground also recorded and exported three RGB cameras. Each background polled for fresh RGB and downloaded its worker evidence. The table counts only the interval when all sessions were advancing. The acceptance gate is a mean of at least 25 fresh viewport draws/second and at least one delivered image/second for **each** background over at least 30 seconds. The offscreen viewport was 1440×900. The Codex headless instrument used 1024×768. The second-to-last column counts complete one-second intervals with fewer than 25 fresh draws.

| Sessions | Author and native backend | Overlap used (s) | Foreground FPS | Background FPS range | p95 / longest draw gap (ms) | Seconds below 25 FPS | Gate |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | --- |
| 4 | DeepSeek via Octos, offscreen | 60.6 | 28.82 | 1.09–1.10 | 54 / 218 | 1/60 | Pass |
| 4 | Codex, headless | 59.5 | 28.99 | 1.10–1.12 | 54 / 218 | 0/59 | Pass |

Frame rates count distinct Nexus exposure ticks drawn by the native Makepad video widget. They measure application draws. Offscreen mode also captures pixels. Physical display scanout, internet transport, remote-host capacity, and other task or camera profiles require separate measurements. Codex and DeepSeek authored controllers before each session executed its local sensor-driven policy. These timings measure the running simulations and Studio. [Timing records and outcomes](results.json)

Every transfer in the table completed with wheel support at all 6,848 physics ticks and the same recorded actuator targets. Each foreground recording reopened in Replay with 342 frames on each of the three cameras. The runs retained delayed draws, individual observation delivery times, hashes and outcomes. [Raw draw events, source exposures, agent measurements, journals and manifests](measurements.zip)

One foreground through the ordinary local Run page drew 28.47 FPS. A separate loopback OpenSSH stream received all 2,318 published exposures at 29.04 FPS. The current worker advertises the persistent binary stream during connection. Older workers continue to serve periodic RGB snapshots. The earlier capacity benchmark measured that polling path. [Previous polling measurements](../worker-observations/local-capacity/README.md)

## Reproduce the measurements

From the Studio checkout, follow the build, AISLE Python environment and author credential instructions in the [workbench guide](../../README.md). Use a new output directory each time. These commands open the actual Studio binary, send Makepad input events, start independent worker jobs, export and reopen the foreground episode, and restart Studio.

```sh
cargo build --locked --release -p dorobot-flex --bin dorobot-flex
.venv-aisle/bin/python tools/aisle_workbench/verify.py \
  --studio target/release/dorobot-flex --backend offscreen --size 1440x900 \
  --fleet --foreground-cadence 4 --octos-deepseek --out /tmp/aisle-four-offscreen
.venv-aisle/bin/python tools/aisle_workbench/verify.py \
  --studio target/release/dorobot-flex --backend headless --size 1024x768 \
  --fleet --foreground-cadence 4 --codex --out /tmp/aisle-four-headless
python3 docs/product/aisle-studio/validation/foreground-preview/audit.py
```

Change the session count to test another load. Each session consumes one real author request. The audit command requires only Python’s standard library and the archived measurements. It checks the reported frame rates, common observation window, per-session task and motor verdicts, exported frame counts and SSH exposure ticks.
