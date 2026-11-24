# Run the controller selected in Task after a restart

In **AISLE Studio → Task**, select Nano or G1. Prepare a controller with **Ask Codex**, **DeepSeek via Octos**, or the reference program. Use **Earlier revision** and **Later revision** to choose the program to run. Studio remembers a separate selection for each robot. Switching robots or restarting Studio restores the instruction and revision number shown in Task. Evidence keeps its own selected trial, so a Nano task can reopen alongside an earlier G1 measurement.

**Run → Start simulation** executes the selected controller. A fleet submission carries the same program digest. If its file changes after selection, local execution and fleet submission reject the changed bytes. If a saved file disappears, changes while Studio is closed, or its selection record is unreadable, Task asks the operator to choose a revision. Workspaces created before revision selection was introduced start with their newest available controller for each robot.

## Measured before and after restart

Codex and DeepSeek V4 Flash through Octos each authored a −0.06 radian left-shoulder inspection on both robot models. In each journey Studio then prepared and ran a newer +0.10 radian reference program for both robots. The operator selected the earlier programs, switched back to Nano and restarted Studio with G1 still selected in Evidence.

| Robot | Newer reference before restart | Earlier agent program after restart |
| --- | ---: | ---: |
| Adora Nano | +0.099932 rad | −0.059959 rad |
| Unitree G1 | +0.099917 rad | −0.059967 rad |

Both providers produced these measured displacements. Each inspected joint stayed within the URDF limits. The eight trials each recorded 121 Nexus physics steps. The controller bytes saved with each post-restart trial matched the program chosen before restart. In both journeys a final attempt with one additional byte in the selected G1 file stopped before a trajectory was produced. These are fixed-base inspections. The floating-base transfer and standing-reach qualifications have separate evidence.

The Codex journey used native headless Makepad at 1024 × 768. The DeepSeek journey used native offscreen Makepad at 1440 × 900. The archive includes the widget interactions, restored choices, exact prepared and executed programs, model identifiers, measured trajectories and worker rejections. [Original measurements](measurements.zip) · [Independent replay](check.py)

With the Studio AISLE environment and the relevant provider configured, run the native journey from the repository root. Use new output directories for further runs.

```sh
python3 docs/product/aisle-studio/validation/controller-selection/check.py
.venv-aisle/bin/python tools/aisle_workbench/verify.py \
  --studio target/debug/dorobot-flex --remote /path/to/makepad-remote \
  --revision-selection --codex --backend headless --size 1024x768 \
  --out /tmp/aisle-selected-codex
.venv-aisle/bin/python tools/aisle_workbench/verify.py \
  --studio target/debug/dorobot-flex --remote /path/to/makepad-remote \
  --revision-selection --octos-deepseek --backend offscreen --size 1440x900 \
  --out /tmp/aisle-selected-deepseek
```
