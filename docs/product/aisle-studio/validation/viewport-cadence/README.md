# Live camera rate on the Run page

Start a local simulation on **Run**, or choose **Workers → Stream run** to watch a worker. The frame rate beside the camera counts distinct RGB exposures drawn by Makepad. The job inspector separately shows physics steps. A background snapshot has its own label.

The display uses draw intervals from the preceding two seconds, measured at the most recent draw. It needs at least half a second of observations to show a number. A two-second interruption switches the label to **Stream stalled**. Fresh frames restart the measurement. Changing the camera source clears its previous measurements. After the run, the last image remains available for inspection.

![The Nano head camera and its live frame rate in the native Run page](run-page.png)

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

Run the following from this directory in a Python environment with NumPy. The audit verifies the motor journal against the published full transfer and recalculates comparisons from the original pixels, configuration samples and per-tick nanosecond records.

```sh
python audit.py --check
```

[Raw samples](render-samples.zip) · [Machine-readable comparisons](render-comparison.json) · [Measurement program and arguments](render_probe.py)
