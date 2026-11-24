# Preserve a single named Nano transfer

An actual headless Studio session sent “Move the red floor cube onto the coffee table” to DeepSeek through Octos. Its first controller requested red, green and blue. The instrumentation caught the mismatch before starting physics. Replaying that saved controller through the revised worker returned a target mismatch before any motor journal was created.

For direct English commands to move, carry, transfer, put or place a single named cube on the coffee table, the author receives a task-specific schema containing the requested color alone. The worker checks both the newly authored program and previously saved programs at execution. Compound and camera-dependent instructions continue through the existing authoring and perception flow.

| Author | Requested | Settled on table | Continuously wheel-supported steps | Invalid motor rows |
| --- | --- | ---: | ---: | ---: |
| Codex | Red only | 1/1 | 6,848/6,848 | 0 |
| DeepSeek through Octos | Red only | 1/1 | 6,848/6,848 | 0 |

Both services received the same red-only schema in separate requests. Each resulting program ran the same built-in transfer controller on simulated Nano RGB and encoders on layout 0. The explicit request fixes the color before inference. Their target journals matched byte for byte. Both trials recorded 3,182 opposing-finger airborne steps and final table contact for the last 100 steps. This is one paired task. [Original draft, rejected execution and both new outcomes](results.json) · [Provider requests, decisions and raw motor traces](paired-traces.zip)


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
