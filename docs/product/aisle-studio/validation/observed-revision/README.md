# Revise a stopped Nano pickup from onboard evidence

A Nano placed red and green on the coffee table, then stopped at the moving blue cube. At physics tick 17,814 the head camera saw two blue candidates. The left wrist saw one. In the calibrated base frame, the head candidates were about 243 mm and 4 mm from the wrist candidate. The right wrist saw none. The earlier program required a unique detection in each contributing camera. [Recorded failure and camera replay](../first-action-change/README.md)

## Use the Studio workflow

1. Open **AISLE Studio → Evidence** and select the failed Nano trial that retained its pickup observation. To recreate the development trial, run the baseline command below.
2. Open **Task** and enter the revision request. Ask to preserve the red, green, blue order and coffee-table destination, and to stop if the cameras cannot identify a single target. Select **Revise with Codex** or **Revise with DeepSeek**. The latter uses the configured Octos profile.
3. Read the selected pickup rule and the author's reason in Task. Studio saves the full model request, original reply and controller alongside the exposure used for the revision. The request contains the measured candidate positions, units, coordinate frame, public per-cube verdicts and available jaw and table readings. The model receives measurements derived from RGB and encoders. The raw exposure stays with the prepared controller so an operator can verify them. [Exact Codex request](codex/controller-request.json) · [Exact DeepSeek request](deepseek/controller-request.json)
4. In **Scene**, select layout 0 and the negative slide. Start the controller in **Run**. Open **Evidence** to review the first moving lift and settled placement for each color, ground support, joint limits and actuator journal. The selected trial and controller can be reopened after restarting Studio.

The outer model selects a bounded association rule. Wrist agreement requires a unique left-wrist candidate and exactly one match within 35 mm in an ambiguous contributing view. The local visual controller rechecks the live observations during descent. Nexus supplies the independent contact and placement verdicts. Both models selected wrist agreement on their first reply. [Codex reply](codex/reply.json) · [DeepSeek reply](deepseek/reply.json)

## Measured outcomes

| Layout | Baseline | Codex | DeepSeek via Octos | Physical outcome |
| --- | --- | --- | --- | --- |
| Development, 0 | Failed | Passed | Passed | The baseline settled two cubes. Both revisions caught and placed all three, with wheel support on all 23,596 ticks. |
| Held out, 519984327 | Passed | Passed | Passed | All three cubes met the moving-grasp and placement criteria. |
| Held out, 792835454 | Failed | Failed | Failed | All three settled on the table. Red missed the first moving lift. |
| Held out, 1048767817 | Failed | Failed | Failed | Red settled. After two attempts the controller could not confirm green in the wrist camera and stopped before blue. |

**Each program passed 1/3 held-out layouts.** The two revisions reproduced the baseline's complete motor journal on each held-out layout. The change repaired the demonstrated ambiguity and produced no observed improvement on this small matched sample. Every trial retained wheel support throughout and recorded zero joint-limit excursion. The two authors made the same controller decision. Codex took 7.77 s and DeepSeek took 5.09 s on this single revision request. [All 12 trial measurements](measurements.json) · [Raw RGB exposures, motor journals and physical contact traces](trials.zip)

The first native authoring run obtained both replies. An instrumentation assertion compared decimal representations of a displayed coordinate and stopped before execution. Studio then reopened the exact prepared bytes, ran them once each on layout 0 and restored the scored trials after restart. Headless verification used 1024×768 dark mode and offscreen verification used 1440×900 light mode. The controller explanation scrolls while the revision actions remain reachable at both widths. [Native receipts and source hashes](native.json) · [Compact Task](revised-controller-compact.png) · [Desktop Task](revised-controller.png) · [Native Evidence](revised-evidence.png)

To independently check the published experiment, run the command below with the same environment. It reconstructs each trial from the archive and replays recorded camera observations, contact histories and motor commands.

```sh
.venv-aisle/bin/python docs/product/aisle-studio/validation/observed-revision/audit.py
```

## Reproduce the experiment

Use the repository's AISLE environment and Nano assets. The first command records the measured failure. The second drives the actual Makepad controls to request a new revision and test it on the development layout. Substitute `--octos-deepseek` for `--codex` to exercise the other author. The third command runs the published frozen programs on the three held-out layouts, without making further model calls. Choose fresh output directories. [Runtime setup](../../README.md)

```sh
proof=docs/product/aisle-studio/validation/observed-revision
.venv-aisle/bin/python tools/aisle_workbench/worker.py run --robot Nano \
  --controller "$proof/baseline/controller.json" --target-motion slide-negative \
  --seed 0 --out /tmp/aisle-stopped-pickup
.venv-aisle/bin/python tools/aisle_workbench/verify.py \
  --studio target/debug/dorobot-flex --backend offscreen --size 1440x900 \
  --revise-pickup /tmp/aisle-stopped-pickup --codex --out /tmp/aisle-revised-pickup
.venv-aisle/bin/python "$proof/evaluate.py" /tmp/aisle-revision-holdout --jobs 3
```

The protocol committed a salted digest of the three new layout seeds before the authors received the failure. The first accepted replies and resulting controller files were frozen in Git before disclosing the seeds. Each frozen program gets one execution on each layout. Both revisions use the same local implementation of wrist agreement. Agreement between authors tests the choice of rule on this case. [Protocol](protocol.json) · [Frozen controller and request checksums](frozen.json) · [Seed disclosure](disclosure.json)
