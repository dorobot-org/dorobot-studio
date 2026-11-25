# Upright-trunk standing reach

The previous frontal-impact study scored 18/20 twice. Two blocks made sustained secondary contact with opposing toes after the initial torso contact. An evaluator-only replay on these disclosed cases located the resting block centers at approximately (0.120, 0.170) m and (0.140, −0.118) m in the horizontal world plane. The reach controller receives body attitude and encoder measurements.

Four disclosed layouts compared the original neutral control, a 0.12 rad toe-out target, a support stance widened by 0.06 rad at each hip, the same widening after the shoulder reach, and a 0.10 rad reduction of the forward trunk angle. Toe-out left one original failure. Widening from the start recovered both original failures and introduced one failure on a passing case. Delayed widening left both original failures. The upright trunk cleared all four and kept both feet loaded. [All development outcomes](development.json)

## Preregistered test

The new operator instruction requests an upright trunk and a −0.08 rad right shoulder reach. DeepSeek through Octos selected the −0.10 rad waist joint target. The program uses the existing free-pelvis balance and shoulder controllers. The independent block, launch, floor and disturbance envelope stay identical to the previous frontal study. Each scored motor sample must contain the selected waist joint target. Evidence links the reported settled waist angle to the final 100 encoder samples.

Run the 20 previously disclosed impact recipes as a paired regression. Separately, execute all eight envelope corners and twelve sealed interior cases twice in separate Nexus processes. Both fresh replicates must pass 20/20. Count each attempt, retain secondary contacts, and apply every existing reach, two-foot support, recovery, limit and drift criterion. [Matched manifest](matched-suite.json) · [Fresh manifest](suite.json) · [Exact controller](controller.json)

## Results

The upright controller scored **17/20 on the paired recipes**, compared with 18/20 for the neutral control. It recovered both earlier failures (14 and 15) and introduced three sustained ankle contacts on previously passing cases (9, 12 and 13). Both fresh repeats scored **17/20**. Cases 4, 14 and 15 again held contact through the last scored step. Every one of the 60 trials recorded a torso impact, 2,000 loaded-foot steps and zero measured guard bypasses. The 20/20 release gate remains open.

| Cohort | Complete trials | Records |
| --- | ---: | --- |
| Previous frontal recipes | 17/20 | [Paired results](matched.json) · [Raw archive](matched.zip) |
| Fresh cases | 17/20 | [First results](a.json) · [Raw archive](a.zip) |
| Fresh cases repeated | 17/20 | [Repeat results](b.json) · [Raw archive](b.zip) |

The offscreen DeepSeek/Octos session placed the nominal wrist within 1.758 mm of its world target and retained two-foot support at every scored step. The final 100 waist encoder samples averaged −0.0327 rad against a −0.10 rad target. Native headless Evidence reopened the published batch, recorded its three failed cases and restored the archive after restart. Neutral remains the default posture. [Evidence page](evidence.png) · [Native receipts](native.json)
