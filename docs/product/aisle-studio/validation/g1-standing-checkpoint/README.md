# Train and evaluate a G1 standing shoulder correction

In Studio, run a DeepSeek standing reach with **Bounded impact**, open **Evidence** and choose **Export episode**. The LeRobot episode contains 100 real operator-camera frames at 10 Hz and synchronized measured and commanded positions for all 29 G1 joints. **Open in Replay** opens the recorded trial. [Replay screenshot](replay.png) · [Complete training episode and teacher motor trace](teacher-episode.zip)

Select **Train shoulder correction** on the exported teacher trial. The Workbench fits a three-coefficient ridge regression to the shoulder position correction observed in that episode. The inputs are the phase of the commanded one-second reach and measured shoulder error. The checkpoint is specific to the selected shoulder, signed displacement and G1 model. It retains a digest of every dataset file and the parent controller. Run the new revision in Nexus. Evidence identifies the learned correction and records the exact checkpoint and dataset. The local controller constrains its correction to ±0.04 rad and applies the same joint and rate limits.

The comparison uses the same 20 impact recipes as the [published standing-reach qualification](../g1-standing-reach/README.md). These are disclosed matched trials. The separate training episode used a central 0.25 kg, 1.20 m/s impact. Both controllers run from the same implementation under every paired condition. Verification reconstructs each shoulder target from the archived checkpoint and measured encoder state.

| Revision | Source |
| --- | --- |
| DeepSeek task and original tracking law | [Controller](teacher/controller.json) · [Matched manifest](teacher/suite.json) |
| Same DeepSeek task with learned correction | [Controller](learned/controller.json) · [Checkpoint](learned/reach-checkpoint.json) · [Training inputs and loss](learned/training.json) · [Matched manifest](learned/suite.json) |
