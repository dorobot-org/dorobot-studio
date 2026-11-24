# Author a task with Nano’s onboard view

Enable **Onboard RGB** on the Nano Task page, select the development layout on Scene, then enter “Approach the leftmost visible floor cube as seen by the robot and stop 0.45 metres away.” Choose **DeepSeek via Octos** or **Ask Codex**. Task displays the captured head-camera image and the color selected by the model. **Run** executes the controller in the selected Nexus scene.

![DeepSeek selecting the blue cube from the actual head-camera exposure](task.png)

Nano captures all three RGB cameras and 17 joint encoders. The local detector uses the calibrated camera rays, encoder-derived camera poses and the nominal floor-cube height to estimate horizontal range and left/right position in the robot frame. Both providers receive the resulting measurements, visibility and camera calibration. With **Ask Codex**, Studio also attaches the original head and both wrist exposures as lossless PNGs. Task identifies which inputs reached the selected provider. The initial trials below used the measurement worksheet. [Current camera inputs, transfer trials and replay](camera-inputs/README.md) New preparation jobs also retain the exact workbench instruction and output schema in `controller-request.json`. The model receives no layout number, engine object coordinates, depth image or segmentation labels. Changing the selected layout, surveyed room or relevant source code invalidates the observation-bound controller.

| Layout | Both providers selected | Independent final range | Grounded physics steps |
| --- | --- | ---: | ---: |
| 0 | Red | 0.4681 m | 460/460 |
| 1 | Blue | 0.4677 m | 600/600 |

Each provider ran both cases in a fresh Studio session. Each run measured zero obstacle contacts. The native driver also changed the scene, confirmed the old controller was rejected before motion and recovered the successful replacement trial on restart. The same local wheel servo interpreted fresh RGB while driving in all four runs. [Provider calls, controllers and headless/offscreen measurements](native-trials.json)

The archived packets contain raw camera pixels, 17 encoder readings and a capture timestamp. The accompanying calibration supplies camera intrinsics and rigid transforms. [Layout 0 packet](layout-0-onboard-scene.npz) · [calibration](layout-0-onboard-calibration.json) · [model input](layout-0-onboard-scene.json). [Layout 1 packet](layout-1-onboard-scene.npz) · [calibration](layout-1-onboard-calibration.json) · [model input](layout-1-onboard-scene.json).

## Place all three in observed left-to-right order

The instruction was “Use the current onboard view to place all three floor cubes on the coffee table, from the robot's left to right as seen before moving.” Each provider independently selected **blue, green, red** on layout 1. Both initially stopped while holding green because the local placement search skipped a reachable gap. Replaying the camera and encoder evidence exposed that gap. In a separate 106.91-second diagnostic call, DeepSeek proposed a 20 mm lateral search and a second strip 20 mm deeper. The implemented search orders visible grid points by distance from the preferred release point and checks occupancy and inverse kinematics before moving.

| Local controller | Full outcome on each provider’s program |
| --- | --- |
| Original | Stopped after blue, with no reachable clear release point for green. 17,024 guarded steps. |
| Finer release search | All three cubes settled. The spare right finger contacted a table leg for 29 steps during reacquisition. The whole task failed. |
| Finer search and folded spare arm | All three cubes settled and detached. All 27,282 physics steps retained wheel support. Zero robot–furniture contact steps and zero rejected motor writes. |

Each provider's original controller bytes were used for all three attempts. The two final runs produced byte-identical motor journals. The outer model selected the order from camera-derived measurements. The local feedback controller performed the grasping, driving and recovery. This is a matched development layout. [Watch the complete DeepSeek run at 6× speed, with head, left wrist and right wrist from left to right](three-cubes.mp4) · [All three versions, proposal and measured outcomes](spatial-transfer.json)

## Reproduce through native Studio

Use the Python environment, AISLE checkout and Makepad remote client from the [workbench setup](../../README.md). Set `PYTHONPATH` to the AISLE `src` directory for the independent observation decoder in the journey.

```sh
.venv-aisle/bin/python tools/aisle_workbench/verify.py \
  --studio target/debug/dorobot-flex --backend offscreen --size 1440x900 \
  --observed-author --octos-deepseek --out /tmp/nano-observed-deepseek
```

Use `--codex` and a fresh output directory to repeat the same task with Codex.
