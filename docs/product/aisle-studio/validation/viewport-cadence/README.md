# Live camera rate on the Run page

Start a local simulation on **Run**, or choose **Workers → Stream run** to watch a worker. The frame rate beside the camera counts distinct RGB exposures drawn by Makepad. The job inspector separately shows physics steps. A background snapshot has its own label.

The display uses draw intervals from the preceding two seconds, measured at the most recent draw. It needs at least half a second of observations to show a number. A two-second interruption switches the label to **Stream stalled**. Fresh frames restart the measurement. Changing the camera source clears its previous measurements. After the run, the last image remains available for inspection.

![The Nano head camera and its live frame rate in the native Run page](run-page.png)

## Native checks

| Foreground | Sample from visible camera heading | Task and recovery |
| --- | ---: | --- |
| M3 Max, 1440 × 900 offscreen, Codex-authored Nano | 30.3 FPS | The red cube reached the table. Studio switched to the background job’s snapshot, then recovered the original 342-frame foreground episode after restart. |
| One H100 NVL, 1024 × 768 headless Studio, DeepSeek controller authored through Octos | 16.1 FPS | The red cube reached the table. Both the evidence and the 347-frame export reopened after restart. |

The harness captured each camera title with the timestamp of the exact draw used to update it. The audit independently reconstructs the displayed rate from the draw intervals ending at that timestamp. Both checks used the same macOS Studio binary. In the H100 check, Linux ran the Nexus worker and Studio received its frames over SSH. When we interrupted that session’s receiver, the title showed **Stream stalled** at 2.083 seconds, followed by **Waiting for frames** and a fresh FPS reading on the same lease. Studio refreshes the selected Run panel on its playback timer. Each local trial and foreground watcher has its own directory. [Label and draw timestamps](native-draws.json) · [Interruption samples](worker-stall.json) · [Build and source hashes](provenance.json)

The direct local path ran the floating-base G1 standing reach with draw tracing disabled. Its external operator camera showed about 6 FPS and returned to its static heading on completion. Both feet stayed loaded for 2,000 steps. The episode contains 101 frames, and Evidence restored the trial after restart. [G1 camera labels, impact and recovery](g1-local.json)

## One stream across two physical hosts

Each worker ran Nexus 0.2.1. The foreground streamed to native Studio on the M3 Max. The other worker delivered head-camera RGB on a 0.5-second wall-clock schedule. Both red-cube transfers completed in each direction.

| Foreground worker | Background worker | Shared measurement interval | Native FPS | Background RGB received |
| --- | --- | ---: | ---: | ---: |
| H100 NVL | M3 Max | 103.998 s | 17.35 | 2.00 Hz |
| M3 Max | H100 NVL | First 30 complete seconds | 29.40 | 2.00 Hz |

With the Mac as foreground, 30/30 one-second bins delivered at least 25 distinct draws and at least one decoded background RGB image. The window began at the first received observer frame. With the H100 as foreground, the longer interval spans 1,805 actual viewport draws and 208 decoded background observations. Each observer image was checked against its worker’s recorded pixel digest. [H100 foreground measurements](mixed-hosts.json) · [H100 foreground trace](mixed-host-trace.zip) · [Mac foreground per-second counts](mac-foreground.json) · [Mac foreground trace and preregistered window](mac-foreground-trace.zip)

## Renderer measurements

On one H100 NVL we replayed the same 320 motor commands with no policy decisions, using layout 0 of the Nano living room. After 64 warm-up steps we timed 256 sequential steps with one head exposure per step. At eight equally spaced ticks we retained all three RGB images and the robot configuration. Each configuration below ran once in a fresh engine. The original settings also ran a second time. The complete production transfer additionally captures wrist exposures for decisions and recordings, publishes previews, and sends them to Studio. The timings below measure the worker portion of the short replay.

| Nexus and shadow settings | Mean measured step | Median head render and readback | Pixels changed across 24 images | Configuration readings identical |
| --- | ---: | ---: | ---: | --- |
| 0.2.1, original 4096 | 47.07 ms | 32.17 ms | Reference | Reference |
| 0.2.1, original 4096 repeated | 46.54 ms | 32.25 ms | 0 | Yes |
| 0.2.1, omit floor from shadow casters | 40.64 ms | 26.55 ms | 221 (0.00533%) | Yes |
| 0.2.1, 2048 shadows | 36.46 ms | 22.81 ms | 99,265 (2.39354%) | Yes |
| 0.2.1, 1024 shadows | 34.18 ms | 20.52 ms | 177,082 (4.26992%) | Yes |
| 0.3.0, original 4096 | 46.44 ms | 32.04 ms | 256,389 (6.18222%) | No |

The denominator is 4,147,200 RGB pixels. A pixel counts as changed if any color channel differs. Even the floor-only change affected all 24 sampled images. The isolated 0.3.0 wheel changed the trajectory with the same motor commands. Studio continues to use the qualified 0.2.1 configuration. The measured production H100 viewport remains below the 25 FPS goal. [Full eight-H100 and Mac capacity measurements](../eight-h100/lossless-rgb/README.md)

We also tried rendering the camera immediately after the physics step, before reading the scoring contacts. That ordering produced the same 24 RGB arrays and configuration samples. Two runs took 47.18 and 47.17 ms per step, compared with 46.20 ms with the original ordering between them. The simulation continues to score contacts before rendering. Use `camera-first` with the same measurement program to repeat this experiment. [Stage timings and sample digests](stage-order.json.gz)

Run the following from this directory in a Python environment with NumPy. The audit verifies the motor journal against the published full transfer and recalculates comparisons from the original pixels, configuration samples and per-tick nanosecond records.

```sh
python audit.py --check
```

[Raw samples](render-samples.zip) · [Machine-readable comparisons](render-comparison.json) · [Measurement program and arguments](render_probe.py)

### Rerun the H100 measurement

Run these commands from this directory using the H100 worker’s Python with AISLE, the Nano assets, and dimforge-nexus3d 0.2.1 available. Each invocation creates a new output directory and initializes a new Nexus engine on the selected GPU.

```sh
repo=$(git rev-parse --show-toplevel)
work=$(mktemp -d)
unzip -p ../eight-h100/lossless-rgb/measurements.zip codex/gpu1/motor-targets.f64 > "$work/motor-targets.f64"
for setting in reference floor 2048 1024; do
  python render_probe.py "$repo" "$work/motor-targets.f64" "$work/$setting" "$setting"
done
python render_probe.py "$repo" "$work/motor-targets.f64" "$work/repeat" reference
```

For the isolated 0.3.0 comparison, install the wheel in a private directory and put it first on the worker interpreter’s import path. The resulting configuration uses the original shadow settings.

```sh
python -m pip install --no-deps --target "$work/wheel" dimforge-nexus3d==0.3.0
python - "$work/wheel" render_probe.py "$repo" "$work/motor-targets.f64" "$work/nexus-0.3.0" reference <<'PY'
import importlib.metadata
from pathlib import Path
import runpy
import sys

wheel = Path(sys.argv.pop(1)).resolve()
sys.path.insert(0, str(wheel))
import nexus3d
assert Path(nexus3d.__file__).resolve().is_relative_to(wheel)
assert importlib.metadata.version("dimforge-nexus3d") == "0.3.0"
sys.argv.pop(0)
runpy.run_path(sys.argv[0], run_name="__main__")
PY
```
