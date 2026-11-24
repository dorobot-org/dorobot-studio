# Compact Nano onboard cameras

**Scene → Onboard camera rendering → Compact** applies to a prepared Nano table transfer. Start it in **Run**, locally or on a registered worker. The fleet checks that both ends have the same bundle. The three cameras use the profile for control, streaming and recording. Restarting Studio defaults the next run to **Original**. Evidence and the exported dataset retain the profile used for the completed trial.

The real URDF still supplies the physical bodies, joint limits and camera mounts. Compact reduces 1,343,189 visual triangles to 329,485 and uses four 2,048-pixel shadow layers. The original profile uses four 4,096-pixel layers. Both use four-sample MSAA. Finger and camera meshes, and all meshes below 6,000 faces, retain the original data. A change to the source model or a reduced mesh fails validation.

## What changes in the images

One H100 NVL replayed the same 320 steps per setting. The times below cover steps 65–320. Eight snapshots of all three cameras supply 24 pairs of 480 × 360 RGB images. The encoders matched at all eight sampled ticks.

| Visuals and shadows | Mean step, scoring and head render | Median head render | Pixels with any channel changed | Mean absolute channel difference, 0–255 |
| --- | ---: | ---: | ---: | ---: |
| Original | 45.89 ms | 32.02 ms | 0% | 0 |
| Compact geometry, original shadows | 33.79 ms | 19.65 ms | 1.57% | 0.67 |
| Compact geometry and shadows | 24.26 ms | 10.45 ms | 3.44% | 0.77 |

## Single H100 with Studio on an M3 Max

A headless Makepad client selected the new option and ran a previously authored DeepSeek/Octos red-cube transfer. On layout 0, Nano maintained wheel support through 6,948 physics ticks. The red cube detached and settled on the table for the final 100 ticks. There were zero rejected motor writes, root-pose writes or joint-limit excursions. The original controller hash was preserved through worker import, dataset export and restart. This run covers one cube and one layout.

| Measurement | Distinct frames | Duration | Mean | Full seconds at or above 25 FPS | Lowest full second |
| --- | ---: | ---: | ---: | ---: | ---: |
| H100 head camera | 4,202 | 153.08 s | 27.44 FPS | 152/153 | 20 |
| Native viewport over raw SSH RGB | 3,768 | 152.67 s | 24.67 FPS | 105/152 | 4 |

The first experimental run with the same visual settings averaged 25.08 FPS in Studio. Neither run sustained 25 FPS throughout. The rendered controller inputs differ from the original profile, so the original three-cube held-out acceptance results apply to that original configuration.

## Three-cube transfer on the M3 Max

The local AISLE/Nexus worker ran the [frozen DeepSeek controller](../nano-holdout-d/controller.json) on layout 0 with the compact profile. The scorer confirmed that Nano released red, green and blue onto the coffee table. All three remained in contact with the table during the final 100 physics ticks. Their centers finished 89.4, 110.1 and 50.5 mm inside the nearest table edge. Nano had wheel support for all 33,462 ticks and recorded zero joint-limit excursions, rejected motor writes or root-pose writes.

The exported LeRobot episode contains 1,673 frames per camera at 5 FPS. The three decoded video frame counts agree with the Parquet rows, and each stored float32 timestamp equals its frame index divided by the recording rate at that precision. This is one layout using the compact profile. [Watch the head-camera recording](three-cube-head.mp4) · [Trial and dataset receipt](three-cube.json)

## Eight H100s with one Studio viewport

Each H100 ran one simulation. Only the foreground used compact cameras. The other seven ran with original cameras and a 0.5-second wall-clock observation period. The existing author receipts bind each trial to its own Codex or DeepSeek/Octos controller. These runs measure execution of those saved controllers. The full fleet audit also verified the exported videos against their Parquet frame times.

| Author | Studio build | Eight-way overlap | Worker FPS | Viewport FPS | Seven background feeds | Full seconds meeting both targets | Tasks passed |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| DeepSeek/Octos | Development | 100.95 s | 28.26 | 24.30 | 1.98–2.00 Hz | 52/100 | 8/8 |
| Codex | Development | 102.02 s | 28.21 | 24.06 | 1.98–2.01 Hz | 46/102 | 8/8 |
| DeepSeek/Octos | Release | 99.20 s | 28.25 | 24.15 | 1.98–2.01 Hz | 47/99 | 8/8 |
| Codex | Release | 99.87 s | 28.15 | 24.69 | 1.99–2.01 Hz | 51/99 | 8/8 |
| DeepSeek/Octos | Release, buffered | 100.90 s | 28.10 | 26.61 | 1.97–2.01 Hz | 88/100 | 8/8 |
| Codex | Release, buffered | 99.18 s | 28.03 | 26.47 | 1.99–2.00 Hz | 86/99 | 8/8 |

Both targets in a full second means at least 25 distinct native draws and at least one verified image received from each of the seven background workers in that second. The fleet acceptance test separately checks the mean rates across at least 30 seconds of eight-way overlap. Both builds with latest-frame-only display fell below the 25 FPS mean. In the Codex run the live receiver observed 4,130 losslessly compressed frames, carrying 99.7 MB of image payload for 2.14 GB of decoded RGB and tick data.

Studio now retains the previous complete foreground exposure and can present it while it is at most 80 ms old. A longer delay skips ahead to the latest frame. Each native draw carries a strictly increasing simulation tick. Both buffered runs met the mean-rate gate. The lowest one-second rates and the longest uninterrupted interval meeting both targets are in the audit output.

The viewer change preserved the recorded data. All sixteen controller and motor journals matched their corresponding latest-frame-only runs. Each foreground also had 347 identical decoded RGB frames in each of its three recorded cameras. [Journal and image digests](viewport-pairs.json) · [Buffered DeepSeek cohort](buffer-deepseek-cohort.json.gz) · [Buffered Codex cohort](buffer-codex-cohort.json.gz)

Run `python docs/product/aisle-studio/validation/compact-cameras/cohorts.py` to recompute the native and background rates from timestamped draws, exposures and received image digests. [DeepSeek cohort](deepseek-cohort.json.gz) · [Codex cohort](codex-cohort.json.gz) · [Release DeepSeek cohort](release-deepseek-cohort.json.gz) · [Release Codex cohort](release-codex-cohort.json.gz)

## Reproduction

Use the repository AISLE Python environment and a registered H100 worker. Run the native verification with the existing fleet options, a prepared red-cube controller and `--compact-visuals`. The recorded source and dataset metadata include the visual bundle SHA-256 and shadow settings. The eight-worker harness accepts `--compact-foreground` to apply the setting only to the single streaming worker.

Run `python docs/product/aisle-studio/validation/compact-cameras/audit.py` to recompute the image and frame metrics. The archive includes the eight encoder samples and 24 RGB images per setting, all measured render intervals, the native draw tick series, worker source times and the scored trial. [Measurements](measurements.zip) · [Computed results](summary.json)

To rebuild the optional camera bundle, install NumPy 2.5.5 and fast-simplification 0.2.0 in a separate Python environment and run `python tools/aisle-validation/build_nano_visual_profile.py --out /tmp/visual-compact-v1.zip`. The runtime reads the shipped archive.
