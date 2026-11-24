# Eight Nano simulations on eight H100s

The Mac runs DoRobot Studio and asks a provider to prepare one controller per session. A Linux server runs eight independent AISLE/Nexus simulations, one per NVIDIA H100 NVL. GPU 0 streams the Nano head camera to the native Makepad viewport while recording all three onboard RGB cameras. GPUs 1–7 publish a head-camera observation every 100 physics ticks. Each Nano uses RGB and encoders to transfer the red floor cube to the coffee table in the same living room.

## Measured capacity

| Author | Eight-way overlap | Source camera | Native viewport | Seven background feeds | Nano tasks passed |
| --- | ---: | ---: | ---: | ---: | ---: |
| DeepSeek via Octos | 90.74 s | 18.18 FPS | 16.40 FPS | 0.67–0.72 Hz | 8/8 |
| Codex | 90.65 s | 17.75 FPS | 16.21 FPS | 0.66–0.70 Hz | 8/8 |

Each provider made eight real authoring requests. Within each cohort the serialized controller was identical across requests. The two providers produced different controller files and the same motor journal. These are 16 independent executions of one fixed trajectory on one layout. They establish concurrency and repeatability. The 25 FPS foreground and 1 Hz per background targets both remain open on this configuration. The remote source camera itself produced fewer than 25 unique frames per second, so improving transport alone cannot meet the foreground target. Each background publishes once per simulated second. The simulations took roughly 90–100 wall seconds to advance through 69.48 simulated seconds, limiting the source cadence itself. Each task ran for 6,948 physics ticks with continuous measured wheel support, and red settled on the coffee table. Both foreground datasets reopened after Studio restarted. The two exports each contain 347 synchronized frames per camera, action and encoder data, and calibration. FFprobe decoded all 347 frames from each of the six exported 480 × 360 videos.

![Measured foreground draw rates and per-GPU background receipt rates](cadence.png)

The two authoring batches overlapped, with up to three preparations per provider in flight. The two simulation cohorts ran sequentially. DeepSeek via Octos used deepseek-v4-flash and took 8.88 s to prepare eight programs, Codex used gpt-6.2-astra and took 19.06 s to prepare eight programs. The model choice changed authoring latency. Each provider ran one cohort. The small difference in simulation speed is within the variation these two measurements leave unresolved.

On the previously tested 128 GiB M3 Max, six local sessions met both cadence targets with both providers. The Mac used the earlier controller and local transport. The eight-H100 run used the revised controller and remote workers over SSH. The measurements describe those two configurations. [Mac measurements, configurations and failed higher loads](../foreground-preview/README.md)

[Machine-readable results](measurements.json) · [Raw draws, worker observations, model receipts, motor journals and both complete LeRobot exports](measurements.zip). Archive SHA-256 `4f67886c97c1b7afaedf8edf0112778a297231f32bbcfcbbeaf1fa4dc2fba9c5`.

## Workload and measurement

The eight controllers in each cohort came from eight real authoring requests. Each author received the exact same measured contact-loss feedback and an explicit 80 mm/s forward limit. The local controller runs throughout navigation, grasp and placement. The provider is called during preparation. These cohorts measure the Studio workflow and its infrastructure on one fixed task and scene. Testing generalization uses the separate paired layout study. [Wheel-contact revision and held-out outcomes](../nano-forward-speed/README.md)

The viewer is a 1440 × 900 native offscreen Makepad window. Each draw counts only when a new simulation tick reaches the viewport. On receipt, the Mac decodes each 480 × 360 RGB plane and records its timestamp and SHA-256. The offline audit matches those receipts to the source digest at each tick. The workers retain their final raw RGB plane. The archive includes all 14 final planes so the last receipt of each observer can also be verified from pixels. The common window starts after every background has reported a positive physics step and ends at the earliest final advancing worker sample. Rate calculations use the first and last distinct delivery inside that window. The audit also reports the count divided by the full window. It uses the Mac monotonic clock to find overlap and the native draw clock for frame-to-frame intervals. The remote capture clock is used only for durations between remote captures. [Protocol declared before dispatch](protocol.md)

## Reproduce

Start with the native Studio binary, Makepad remote instrument, a configured Codex or Octos author on the Mac, and an installed Linux worker checkout with the Nano model, AISLE, Nexus 0.2.1, NVIDIA Vulkan ICD and the Mesa device-selection layer. The node registry contains private SSH paths and remains on the local machine. The setup creates eight fresh worker spools under an existing remote directory. [Install Studio](../../README.md) · [Set up a Linux worker and SSH transport](../remote-nano/README.md)

~~~sh
guide=docs/product/aisle-studio/validation/eight-h100
python3 "$guide/configure.py" --ssh "$worker_host" --ssh-config "$worker_ssh_config" \
  --checkout "$remote_studio" --aisle-root "$remote_aisle" \
  --interpreter "$remote_nexus_python" --remote-parent "$remote_spool_parent" \
  --out "$nodes_file"
~~~

For each provider, set `cohort` to a fresh output directory, `provider` to `codex` or `octos-deepseek`, and `failed_trial` to the original scored Nano trial with the two wheel-support gaps. Use the Studio Python environment. Finish one measurement before starting the next foreground stream.

~~~sh
"$DOROBOT_AISLE_PYTHON" "$guide/prepare.py" --provider "$provider" \
  --feedback "$failed_trial" --out "$cohort/authors"
"$DOROBOT_AISLE_PYTHON" "$guide/run.py" --nodes "$nodes_file" \
  --studio "$studio_binary" --remote "$makepad_remote" \
  --authors "$cohort/authors" --out "$cohort/measurement"
~~~

The recorded H100 cohorts used a one-second simulation period. To run a new cohort using wall time, append `--background-clock wall --background-period-s 0.5` to the runner command above. Update the Linux worker checkout first. The runner checks each worker's advertised clock and periods before submitting. Worker publication uses that worker's monotonic clock. The Mac timestamps received images and viewport draws on its own clock. See the [local wall-time measurements](../wall-clock-rgb/README.md).

Run the public archive audit with Python's standard library.

~~~sh
python3 "$guide/audit.py" "$guide/measurements.zip"
~~~

To decode every image and check its presentation time against the episode's Parquet rows, use the Studio Python environment with PyArrow and FFprobe. This also checks each row's 17 action and encoder values and each camera's calibration and dimensions.

~~~sh
"$DOROBOT_AISLE_PYTHON" "$guide/audit.py" "$guide/measurements.zip" --datasets
~~~

The runner checks each worker for an available single-slot spool before starting. Interrupting a cohort stops its native processes and cancels the job IDs it submitted. A separate interruption check observed eight running Nano leases on the Mac and verified that all eight reached the cancelled state. The H100 endpoint timed out before the remote interruption check could dispatch a job. [Local interruption receipts](interrupt-mac.json)
