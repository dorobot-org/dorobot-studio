# Preserve a single named Nano transfer

An actual headless Studio session sent “Move the red floor cube onto the coffee table” to DeepSeek through Octos. Its first controller requested red, green and blue. The instrumentation caught the mismatch before starting physics. Replaying that saved controller through the revised worker returned a target mismatch before any motor journal was created.

For direct English commands to move, carry, transfer, put or place a single named cube on the coffee table, the author receives a task-specific schema containing the requested color alone. The worker checks both the newly authored program and previously saved programs at execution. Compound and camera-dependent instructions continue through the existing authoring and perception flow.

| Author | Requested | Settled on table | Continuously wheel-supported steps | Invalid motor rows |
| --- | --- | ---: | ---: | ---: |
| Codex | Red only | 1/1 | 6,848/6,848 | 0 |
| DeepSeek through Octos | Red only | 1/1 | 6,848/6,848 | 0 |

Both services received the same red-only schema in separate requests. Each resulting program ran the same built-in transfer controller on simulated Nano RGB and encoders on layout 0. The explicit request fixes the color before inference. Their target journals matched byte for byte. Both trials recorded 3,182 opposing-finger airborne steps and final table contact for the last 100 steps. This is one paired task. [Original draft, rejected execution and both new outcomes](results.json) · [Provider requests, decisions and raw motor traces](paired-traces.zip)


## Stationary and moving pickups

For a stationary red cube, enter **Move the red floor cube onto the coffee table.** For a moving target, enter **Move the sliding red floor cube onto the coffee table.** Studio binds that request to red and enables pickup tracking. A revision of a tracked parent can continue tracking. A stopped pickup can also supply recorded camera evidence for changing the tracking rule. Previously saved controllers retain their recorded pickup strategy.

Two headless Makepad fleet journeys used the same Nano model, layout 0, stationary target, operator instruction and DeepSeek V4 Flash through Octos. In the first, the author enabled moving-target tracking. After two failed wrist-camera confirmations, the controller stopped. Red was still on the floor. The revised authoring schema restricts a plain named transfer to stationary pickup. DeepSeek returned the required mode, and Studio ran the generated controller through the same local worker.

| Measured outcome | Original draft | Revised draft |
| --- | ---: | ---: |
| Supported physics ticks | 4,364 / 4,364 | 6,848 / 6,848 |
| Opposing-finger airborne ticks | 0 | 3,182 |
| Final consecutive table-contact ticks | 0 | 100 |
| Recorded frames per onboard camera | 218 | 342 |

The revised trial streamed to Run, continued while a second observation-only lease was cancelled, exported a dataset, and reopened in Studio after restart. Its motor journal matched the previously measured stationary red transfer byte for byte. Codex returned the required stationary mode for the same instruction. DeepSeek enabled tracking in a free-form moving-cube request and returned the required red-only tracking mode for the sliding-cube instruction above. Both moving examples here measured authoring. [Measured moving grasps](../moving-grasp/README.md) The matched fleet comparison covers one layout. [Original model requests, both leases and motor journals](stationary-fleet.zip) · [Replay the checks with Python](check_stationary.py). Add `--datasets` in the Studio environment to decode all three videos and verify frame timestamps against all 342 Parquet rows. The original fleet failure used [the earlier authoring code](https://github.com/dorobot-org/dorobot-studio/tree/7872a9845fb66807827a5b8ea5b4aa32f177f570). The archived successful native fleet rerun used [the revised authoring code](https://github.com/dorobot-org/dorobot-studio/tree/cda37d2c5976b4810ff6847072f26d879e272957).

## Reproduction

Use the [Studio AISLE runtime setup](../../README.md#run-from-a-studio-source-checkout) and the [worker revision used for the two recorded runs](https://github.com/dorobot-org/dorobot-studio/tree/6687f5b). Configure each provider, then run from the Studio checkout.

```sh
scratch=$(mktemp -d)
for author in codex octos-deepseek; do
  .venv-aisle/bin/python tools/aisle_workbench/worker.py prepare --provider "$author" --robot Nano \
    --task 'Move the red floor cube onto the coffee table.' --out "$scratch/$author"
  .venv-aisle/bin/python tools/aisle_workbench/worker.py run --robot Nano --seed 0 \
    --controller "$scratch/$author/controller.json" --out "$scratch/$author-trial"
done
```

Extract the archived `original/controller.json` and pass it to the same `run` command with a fresh output directory to reproduce the rejected transfer. A controller from an older integration may have no task text. The worker can check its allowed motion and joint limits, but that artifact provides no original instruction for a target comparison.

The headless Studio journey authored with DeepSeek, placed red, checked the Nano hardware adapter, exported the simulated episode and restored its receipt after restarting. [Native verification](native.json)
