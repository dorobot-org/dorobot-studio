# Train and evaluate a G1 standing shoulder correction

In Studio, run a DeepSeek standing reach with **Scene → Impact test**, open **Evidence** and choose **Export episode**. The LeRobot episode contains 100 real operator-camera frames at 10 Hz and synchronized measured and commanded positions for all 29 G1 joints. **Open in Replay** opens the recorded trial. [Replay screenshot](replay.png) · [Complete training episode and teacher motor trace](teacher-episode.zip)

Select **Train shoulder correction** on the exported teacher trial. The Workbench fits a three-coefficient ridge regression to the shoulder position correction observed in that episode. The inputs are the phase of the commanded one-second reach and measured shoulder error. The checkpoint is specific to the selected shoulder, signed displacement and G1 model. It retains a digest of every dataset file and the parent controller. Run the new revision in Nexus. Evidence identifies the learned correction and records the exact checkpoint and dataset. The local controller constrains its correction to ±0.04 rad and applies the same joint and rate limits.

The comparison uses the same 20 impact recipes as the [published standing-reach qualification](../g1-standing-reach/README.md). These are disclosed matched trials. The separate training episode used a central 0.25 kg, 1.20 m/s impact. Both controllers run from the same implementation under every paired condition. Verification reconstructs each shoulder target from the archived checkpoint and measured encoder state.

| Revision | Source |
| --- | --- |
| DeepSeek task and original tracking law | [Controller](teacher/controller.json) · [Matched manifest](teacher/suite.json) |
| Same DeepSeek task with learned correction | [Controller](learned/controller.json) · [Checkpoint](learned/reach-checkpoint.json) · [Training inputs and loss](learned/training.json) · [Matched manifest](learned/suite.json) |

The first fit paired each target with the pose recorded at the end of that tick. The full-rate trial trace also retains the encoder exposure immediately before the command, which is the information available at inference time. The final trainer uses those causal encoder inputs with the same 100 LeRobot action labels. The largest within-tick difference in the training episode was 0.00058 rad. Both fitted revisions use the same runtime and actuator limits. [Aligned checkpoint](aligned/reach-checkpoint.json) · [Input digests](aligned/training.json) · [Matched evaluation manifest](aligned/suite.json)

## Measured comparison

| Revision | Complete trials | Worst settled wrist error | Mean estimated positive joint work | Evidence |
| --- | ---: | ---: | ---: | --- |
| Original tracking law | 19/20 | 2.624 mm | 0.815 J | [Report](teacher/report.json) · [Raw traces](teacher/evidence.zip) |
| First fit | 19/20 | 2.643 mm | 0.789 J | [Report](learned/report.json) · [Raw traces](learned/evidence.zip) |
| Fit using pre-command encoders | 19/20 | 2.647 mm | 0.792 J | [Report](aligned/report.json) · [Raw traces](aligned/evidence.zip) |

Case 9 registered no block contact on all three revisions and counts as a failure. The original qualification had recorded positive impulses on that same recipe. Every other case recovered in 0.50 s. All 60 traces verified and all retained two-foot support throughout. The learned correction reproduced the standing task and slightly increased wrist error. Its estimated positive motor work was lower on these matched simulations. The task success rate did not improve.

The 1024 × 768 headless and 1440 × 900 offscreen journeys each used a fresh DeepSeek task, exported a real LeRobot episode, trained from it, executed the learned revision, opened the teacher episode in native Replay and recovered the learned trial after restart. The verifier checked the recorded 29-dimensional states and actions against the full-rate motor log. [Native training, rollout, replay and recovery checks](native-training.json) · [Checkpoint Evidence page](standing-checkpoint.png)
