# G1 standing reach with a frontal torso impact

## Fixture development

The preceding fixture produced three logo-only impacts in each 20-case execution. At release, the measured G1 posture rotates the torso forward by approximately 9°. Transforming the torso collision mesh by this measured pose places its front face at x = 109 mm at z = 195 mm on the centerline. The logo collision hull extends down to approximately z = 240 mm. A 65 mm block moving toward this surface makes contact when its center reaches about x = 140 mm. [Previous paired results](../g1-torso-impact/README.md) · [Standing poses, impulses and outcomes from both development fixtures](development.json)

Two fixtures used the same four disclosed development cases. A lower chest aim from x = 290 mm hit the torso on all four. On three, the block later rested on a foot through the end of the trial. Only one met the full recovery requirement. The frontal fixture starts at x = 195 mm and targets z = 195 mm at x = 140 mm. Its 55 mm free flight yields a predicted gravitational drop of 9–12 mm across the speed envelope. All four reached the torso, registered positive impulses, recovered and kept both feet loaded. The block remained free to hit the robot again throughout each trial. Both sets used the same controller as the prior study.

## Preregistered replication

Run the exact [DeepSeek program](controller.json) on all eight mass, speed and lateral-offset corners and twelve newly sealed interior recipes. Execute the same 20 cases twice in independent Nexus processes, one attempt per case per replicate. Qualification requires **20/20 in each replicate**. Each case requires positive normal impulse on the named torso link, both feet loaded at all 2,000 scored steps, at least 15 mm of requested reach, final 100-step wrist error at most 10 mm and 100 stable steps within two seconds of the final robot–block contact. The existing tilt, drift and actuator gates also apply. [Complete criteria](../g1-standing-reach/README.md) · [Executable fixture and input commitments](suite.json)

The public manifest SHA-256 is `00f6cc81988127b5a08c08e5adc9242d294815069e3b2f807fc18a4eb83e7a2d`. Report each replicate separately and retain the raw motor, wrist and contact records of every execution. Report the observed torso impulse range alongside the pass counts. The shorter flight changes the incident angle and the disturbance delivered to the torso. Results characterize this particular frontal fixture.
