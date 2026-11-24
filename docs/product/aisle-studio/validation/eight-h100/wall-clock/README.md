# Eight H100s with wall-clock RGB observations

The Mac ran one native Studio viewport. Each of eight H100 NVLs ran one AISLE/Nexus Nano simulation. The foreground worker streamed head RGB and recorded the head and both wrists. Each of the other seven workers targeted a 480 × 360 head-camera publication every 0.5 wall seconds. The comparison reuses the original controller files and author receipts. All 16 executions have a byte-identical motor journal to the corresponding earlier run on the same red-cube layout.

| Controller author | Eight-way overlap | Worker head RGB | Mac viewport | Seven feeds received on the Mac | Earlier seven feeds, simulation clock | Tasks passed |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| DeepSeek via Octos | 102.08 s | 18.62 FPS | 17.02 FPS | 1.08–1.10 Hz | 0.67–0.72 Hz | 8/8 |
| Codex | 101.14 s | 18.08 FPS | 13.40 FPS | 0.70–0.71 Hz | 0.66–0.70 Hz | 8/8 |

All 14 background workers published 2.00 images per second. Delivery at the Mac varied between runs. Seven receivers exceeded 1 Hz in the DeepSeek cohort and none did in the Codex cohort. Both foreground runs remained below 25 FPS. The same saved motor programs ran under both observation schedules. Network, host load and transport can vary between trials.

The rates above cover the common interval when all eight simulations were advancing. The original simulation-clock setting published after every 100 physics ticks. In this run the wall-clock setting schedules exposures independently of simulated time. The audit matches each received image to its worker's exposure hash. Each foreground dataset reopened after Studio restarted and contains 347 synchronized frames per camera. Both local authoring batches came from the earlier study. There were no new model requests during these simulations.

## Timing within the common interval

A feed can average more than one image per second while leaving some individual seconds empty. The table splits the common interval into complete one-second windows on the Mac, starting when all eight workers were active. The longest gap is between successive received background images inside that interval.

| Controller author | Complete seconds | Foreground seconds below 25 images | Empty seconds on GPUs 1 through 7 | Longest background gap |
| --- | ---: | ---: | --- | ---: |
| DeepSeek via Octos | 102 | 102 | 8, 7, 8, 8, 9, 9, 8 | 3.01 s |
| Codex | 101 | 101 | 35, 35, 36, 35, 35, 34, 35 | 2.92 s |

## Reproduce the comparison

Configure one available worker per GPU, use the same prepared controllers, and run one foreground cohort at a time. Append the following settings to the benchmark runner. [Worker setup and cohort command](../README.md#workload-and-measurement)

```sh
--background-clock wall --background-period-s 0.5
```

From the repository root, use the Studio Python environment to audit both archives. The optional dataset check decodes all six new camera videos, verifies their timestamps against Parquet and checks all 17 action and encoder channels.

```sh
guide=docs/product/aisle-studio/validation/eight-h100/wall-clock
.venv-aisle/bin/python "$guide/compare.py" --datasets
```

The archive includes native draws, background publication and receipt logs, the final RGB plane from every observer, task verdicts, motor journals and both LeRobot episodes. SHA-256 `d90626284134e2b930958ace2a72d02472f321924aeb7aa22f9cc85983dfc54f`. [New measurements](measurements.zip) · [Paired results and delivery gaps](comparison.json) · [Original simulation-clock measurements](../measurements.zip)
