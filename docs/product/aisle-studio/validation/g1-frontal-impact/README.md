# G1 standing reach with a frontal torso impact

## Fixture development

The preceding fixture produced three logo-only impacts in each 20-case execution. At release, the measured G1 posture rotates the torso forward by approximately 9°. Transforming the torso collision mesh by this measured pose places its front face at x = 109 mm at z = 195 mm on the centerline. The logo collision hull extends down to approximately z = 240 mm. A 65 mm block moving toward this surface makes contact when its center reaches about x = 140 mm. [Previous paired results](../g1-torso-impact/README.md) · [Standing poses, impulses and outcomes from both development fixtures](development.json)

Two fixtures used the same four disclosed development cases. A lower chest aim from x = 290 mm hit the torso on all four. On three, the block later rested on a foot through the end of the trial. Only one met the full recovery requirement. The frontal fixture starts at x = 195 mm and targets z = 195 mm at x = 140 mm. Its 55 mm free flight yields a predicted gravitational drop of 9–12 mm across the speed envelope. All four reached the torso, registered positive impulses, recovered and kept both feet loaded. The block remained free to hit the robot again throughout each trial. Both sets used the same controller as the prior study.

## Preregistered replication

Run the exact [DeepSeek program](controller.json) on all eight mass, speed and lateral-offset corners and twelve newly sealed interior recipes. Execute the same 20 cases twice in independent Nexus processes, one attempt per case per replicate. Qualification requires **20/20 in each replicate**. Each case requires positive normal impulse on the named torso link, both feet loaded at all 2,000 scored steps, at least 15 mm of requested reach, final 100-step wrist error at most 10 mm and 100 stable steps within two seconds of the final robot–block contact. The existing tilt, drift and actuator gates also apply. [Complete criteria](../g1-standing-reach/README.md) · [Executable fixture and input commitments](suite.json)

The public manifest SHA-256 is `00f6cc81988127b5a08c08e5adc9242d294815069e3b2f807fc18a4eb83e7a2d`. Report each replicate separately and retain the raw motor, wrist and contact records of every execution. Report the observed torso impulse range alongside the pass counts. The shorter flight changes the incident angle and the disturbance delivered to the torso. Results characterize this particular frontal fixture.

## Results

| Measure | First execution | Independent repeat |
| --- | ---: | ---: |
| Complete standing reach under the declared rules | **18/20** | **18/20** |
| Positive impact on the named torso link | 20/20 | 20/20 |
| Sum of normal torso impulses per case | 0.0170–0.1282 N·s | 0.0170–0.1281 N·s |
| Worst final 100-step wrist error | 2.624 mm | 2.624 mm |
| Both feet loaded | 40,000/40,000 steps | 40,000/40,000 steps |
| Recovery in 0.50 s after the last robot–block contact | 18/20 | 18/20 |
| Verified motor traces and guard bypasses | 20 traces, 0 bypasses | 20 traces, 0 bypasses |

All eight envelope corners passed in both executions. Cases 14 and 15 hit the torso, then the block fell against the left or right ankle respectively. Positive ankle impulses persisted through step 2,000. Neither trial could complete the required half-second recovery window after final contact. The robot remained upright with the wrist inside tolerance in both. The case-level records include every secondary contact. The 20/20 release gate remains open. [First report](replicate-a.json) · [First raw archive](replicate-a.zip) · [Repeat report](replicate-b.json) · [Repeat raw archive](replicate-b.zip)

The controller and command stayed fixed. The earlier fixture delivered positive total torso impulses up to 0.2743 N·s, compared with 0.1282 N·s on this fixture. These measurements apply to different incident trajectories and different interior cases. The contact names come from Nexus's batch-local multibody link slots and the robot's native link order. The G1 occupies the only multibody in this scene.

The native Task, Scene, Run and Evidence journey authored a separate DeepSeek controller, ran the nominal frontal impact, recorded 100 synchronized frames, opened the episode in Replay and restored the scored trial after restarting Studio. [Native trial, source and impulses](native-trial.json)

Studio reopened both raw archives in native Evidence, displayed 18/20 with both failures, rejected a missing replacement and restored the last verified archive on restart. [Headless and offscreen checks](native-batch.json) · [Evidence page](studio.png)
