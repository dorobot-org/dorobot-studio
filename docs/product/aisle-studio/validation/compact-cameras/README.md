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

## Single H100 with a Mac Studio client

A headless Makepad client selected the new option and ran a previously authored DeepSeek/Octos red-cube transfer. On layout 0, Nano maintained wheel support through 6,948 physics ticks. The red cube detached and settled on the table for the final 100 ticks. There were zero rejected motor writes, root-pose writes or joint-limit excursions. The original controller hash was preserved through worker import, dataset export and restart. This run covers one cube and one layout.

| Measurement | Distinct frames | Duration | Mean | Full seconds at or above 25 FPS | Lowest full second |
| --- | ---: | ---: | ---: | ---: | ---: |
| H100 head camera | 4,202 | 153.08 s | 27.44 FPS | 152/153 | 20 |
| Native viewport over raw SSH RGB | 3,768 | 152.67 s | 24.67 FPS | 105/152 | 4 |

The first experimental run with the same visual settings averaged 25.08 FPS in Studio. Neither run sustained 25 FPS throughout. The rendered controller inputs differ from the original profile, so the original three-cube held-out acceptance results apply to that original configuration.

## Reproduction

Use the repository AISLE Python environment and a registered H100 worker. Run the native verification with the existing fleet options, a prepared red-cube controller and `--compact-visuals`. The recorded source and dataset metadata include the visual bundle SHA-256 and shadow settings. The eight-worker harness accepts `--compact-foreground` to apply the setting only to the single streaming worker.

Run `python docs/product/aisle-studio/validation/compact-cameras/audit.py` to recompute the image and frame metrics. The archive includes the eight encoder samples and 24 RGB images per setting, all measured render intervals, the native draw tick series, worker source times and the scored trial. [Measurements](measurements.zip) · [Computed results](summary.json)

To rebuild the optional camera bundle, install NumPy 2.5.5 and fast-simplification 0.2.0 in a separate Python environment and run `python tools/aisle-validation/build_nano_visual_profile.py --out /tmp/visual-compact-v1.zip`. The runtime reads the shipped archive.
