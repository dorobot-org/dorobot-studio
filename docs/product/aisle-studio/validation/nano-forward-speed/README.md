# Revise Nano forward speed after a measured loss of wheel support

A Nano can place a cube successfully and still fail the task. On the original Linux run the red cube settled on the coffee table. At ticks 305 and 4,067, Nexus measured no wheel carrying floor load above the existing threshold. At those ticks the controller was requesting 120 mm/s during localization and 55 mm/s during carry. [Original Linux trial](../remote-nano/README.md) · [The two intervals in Studio](support-intervals.png) · [Per-wheel contact impulses around the original gaps](wheel-contacts.png)

## Revise the controller in Studio

Select the failed trial in **Evidence**, then open **Task**. Ask to preserve the red floor-to-table transfer and limit positive forward speed to 0.080 m/s. Choose **Revise with Codex** or **Revise with DeepSeek**. Review the speed in **Prepared controller**. Start the run and return to **Evidence** to check both placement and wheel support. For a registered worker, open **Run → Workers**, connect to the alias and queue the foreground job.

Both authors received the same public support counts and the commands active during those intervals. Each prepared the requested limit through native Makepad. The 80 mm/s value was supplied in the operator instruction after the engineering trials. The local RGB controller still computes each action from onboard cameras and encoders. It caps positive translation, keeps its existing reverse and steering commands, and the motor worker rejects requests above the prepared limit. The support verdict comes from Nexus contact impulses and is used during revision and evaluation. [Native Task page](drive-revision.png) · [Codex request](codex/controller-request.json) · [Codex reply](codex/reply.json) · [DeepSeek request](deepseek/controller-request.json) · [DeepSeek reply](deepseek/reply.json)

The Mac then submitted the exact DeepSeek controller to the H100 from native Studio. The remote simulation settled red and recorded loaded wheel contact on all 6,948 ticks. Studio downloaded the original episode, exported all three synchronized 347-frame videos and reopened Replay after restart. [Live worker view](linux-foreground.png) · [Scored Linux outcome](linux-imported-evidence.png) · [Recovered Replay](linux-imported-replay.png) · [Download the LeRobot dataset](linux-dataset.zip) · [Native receipts](linux-native.json)

## Matched experiments

| Platform and task | Original controller | 80 mm/s revision |
| --- | ---: | ---: |
| Linux H100, original red-cube layout | Failed, 6,806/6,808 supported ticks | Passed, 6,948/6,948 |
| Mac M3 Max, original layout | Passed, 6,848/6,848 | Passed, 12,608/12,608 |
| Linux H100, three held-out red-cube layouts | 3/3 passed | 2/3 passed |
| Mac M3 Max, same three held-out layouts | 2/3 passed | 3/3 passed |

The cap fixed the original Linux failure. Across the six held-out platform and layout pairs, each controller passed five. On seed 6217381 the original Mac run lost red during carry, and the revision completed. On Linux the original passed on that seed. The revision placed red but lost wheel support at tick 7,411. The speed revision is an operator choice. Historical controllers retain their original wheel behavior.

The preregistered development comparison tested 40, 60 and 80 mm/s once per platform on layout 0. All six trials passed. The highest jointly passing limit and exact controller bytes were frozen before revealing three new layout seeds. Each platform used the same seed for each baseline and revision pair. The 50-layout three-cube qualification is a separate experiment. [Preregistered selection rule](protocol.json) · [All six development measurements](development.json) · [Frozen controller](selected-controller.json) · [Seed disclosure](disclosure.json) · [Three-cube qualification](../nano-qualification.md)

## Verify the measurements

The archive contains the original controllers, action journals and full motor targets for 23 executions. Sixteen trials also have an independent Nexus replay of those exact motor targets on their originating backend. Each replay starts a fresh living room on the recorded layout and measures the peak normal contact impulse for each wheel on every tick. The original Linux journal reproduced both unsupported ticks exactly. [All outcomes and archive checksum](measurements.json) · [Motor journals, contact traces and program bytes](trials.zip)

```sh
.venv-aisle/bin/python docs/product/aisle-studio/validation/nano-forward-speed/audit.py
```

## Reproduce

Configure the Studio AISLE Python environment and Nano meshes. On each platform run the paired evaluation with a fresh output directory. The second command re-executes a motor journal in a fresh Nexus scene on the matching backend. It reads the maximum individual wheel-floor normal contact impulse for each of the three wheels at every tick. The evaluator's threshold is 0.000001 N·s. [Runtime setup](../../README.md)

```sh
proof=docs/product/aisle-studio/validation/nano-forward-speed
.venv-aisle/bin/python "$proof/evaluate.py" /tmp/aisle-nano-forward
.venv-aisle/bin/python "$proof/replay_support.py" \
  /tmp/aisle-nano-forward/selected-6217381 /tmp/aisle-nano-contact-replay
```

The following commands exercise the actual native pages. Supply the full original Linux foreground trial with its recorded episode and use a new output directory for each invocation. Substitute `--octos-deepseek` to author with DeepSeek. The worker command reuses a prepared controller and verifies the downloaded result and replay after restarting Studio. [SSH worker setup](../worker-leases/README.md)

```sh
.venv-aisle/bin/python tools/aisle_workbench/verify.py \
  --studio target/debug/dorobot-flex --backend offscreen --size 1440x900 \
  --revise-drive /path/to/original-linux-trial --codex --out /tmp/aisle-drive-revision
.venv-aisle/bin/python tools/aisle_workbench/verify.py \
  --studio target/debug/dorobot-flex --backend offscreen --size 1440x900 \
  --fleet --fleet-node h100 --fleet-controller /tmp/aisle-drive-revision/state/aisle/*-Nano-prepare \
  --out /tmp/aisle-drive-remote
```
