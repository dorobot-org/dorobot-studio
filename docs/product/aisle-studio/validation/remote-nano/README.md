# Run Nano on a Linux H100 worker from Studio

The Mac runs the native Makepad workbench and requests a controller from DeepSeek via Octos. The registered Linux worker receives the prepared controller over SSH, runs the Nano URDF through AISLE and Nexus, and returns RGB and scored artifacts. On Linux this configuration uses Nexus WebGPU through NVIDIA Vulkan. On macOS the same scene uses Metal.

## What the measured run established

| Check | Observed result |
| --- | --- |
| Remote foreground | The Nano settled red 115 mm inside the coffee-table rim. The motor journal passed and all 340 recorded frames reopened in Replay after Studio restarted. |
| Physical verdict | **Failed.** The evaluator recorded loaded wheel support in 6,806 of 6,808 ticks. It recorded one unsupported tick during localization (305) and one during carry (4,067). The cube had table contact on the last 100 ticks and the measured joint-limit excursion was zero. |
| Concurrent remote background | The operator retrieved the head-camera sample at tick 600. Its digest matched the downloaded archive. Cancelling that lease preserved the foreground simulation. |
| Visible foreground cadence | After returning from the background view, the Mac drew 5,960 distinct remote camera ticks in 340.03 wall seconds, **17.52 FPS**. The initial shorter interval measured 16.20 FPS. These figures cover one selected camera view on this Mac–Linux link. |
| Device use | During the remote lease overlap, NVIDIA reported both simulation processes active on GPU 0. The GPU reported 11,264 MiB used and 74% utilization in that sample. The driver also registered small contexts on other devices. Eight-card throughput needs its own benchmark. |

The native verifier initially expected continuous wheel support and stopped before importing the failed result. The operator reconnected to the same completed remote lease, fetched the scored trial and exported the original three-camera episode. The second native restart recovered the same trial and dataset. The recovery reused both original remote job IDs. [Measured verdict](imported-evidence.png) · [Camera sample](background-snapshot.png) · [Native Replay](imported-replay.png) · [Measurements and artifact digests](measurements.json) · [Exported dataset, contact results and motor journals](episodes.zip) · [Raw viewport ticks](draws.jsonl.gz) · [GPU samples](gpu-timing.jsonl.gz)

## Configure a worker

Install the Studio checkout, Nano meshes and the matching AISLE runtime on the worker. Use the Python environment from the Studio setup, including OpenCV and SciPy. The vision controller imports OpenCV and its arm kinematics use SciPy. Initialize a two-slot spool and register it on the Mac using an SSH alias. The operator's model credentials belong on the machine that issues the model request. [Runtime installation](../../README.md) · [Worker registration and authentication](../worker-leases/README.md)

In **Run → Workers**, enter the registered alias and connect. Inspect the returned model hash, backend and available slots. Submit a foreground job and look for advancing camera ticks. Submit a background job, select it, fetch a camera observation and cancel that job. Select the foreground job again and fetch its completed episode into **Evidence**. Export it and open **Replay**.

To repeat the complete UI check, configure a reachable worker in `DOROBOT_AISLE_WORKERS` and the local provider as above. The verifier controls Studio through Makepad, restarts the app and checks the reloaded episode. Choose an output directory that does not already exist.

```sh
.venv-aisle/bin/python tools/aisle_workbench/verify.py \
  --studio target/debug/dorobot-flex --backend offscreen --size 1440x900 \
  --fleet --fleet-node h100 --octos-deepseek --out /tmp/aisle-remote-nano
.venv-aisle/bin/python docs/product/aisle-studio/validation/remote-nano/summarize.py \
  /tmp/aisle-remote-nano
```

The worker used a Python 3.12 environment with Nexus 0.2.1, an NVIDIA Vulkan ICD and OpenCV 5.1.0.95 with SciPy 1.18.1. For a multi-GPU host, choose the intended Vulkan device and verify placement with the driver during the run. On this host the Mesa device-select layer accepted a PCI-qualified `DRI_PRIME` setting. The remote account and spool are operator-owned. The app streams the foreground and polls the selected background observation over the registered SSH connection.


## Exercise both physical hosts

While the Linux foreground is running and selected in the native verifier, the following command uses the same authored controller for a second Nano on the Mac. It starts a separate local worker with one slot, samples progress on both machines against the Mac's monotonic clock and verifies the downloaded local motor journal. It only cancels the local job it created if that test is interrupted.

```sh
.venv-aisle/bin/python docs/product/aisle-studio/validation/remote-nano/overlap.py \
  /tmp/aisle-remote-nano /tmp/aisle-mac-observer
```

The Mac and the remote foreground both advanced for at least **54.89 wall seconds** with the same authored controller. The Mac placed red in 6,848 continuously supported physics steps. [Paired progress samples and controller identity](cross-host.json)
