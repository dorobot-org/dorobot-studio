# G1 standing reach with a 20-degree left waist turn

DeepSeek through Octos prepared the right shoulder command of −0.08 rad and a 20° left waist turn. The local controller establishes the posture on both feet before the scored reach. The wrist target uses the measured starting pose and the same forward-kinematics rule as the neutral trial. This evaluates a reach relative to the standing posture. The frontal block trajectory, mass, speed and lateral-offset envelope are identical across the paired recipes.

A development study of previously disclosed frontal impacts passed all 20 with this posture. The frozen product controller must repeat those recipes and pass all 20 fresh cases twice. Each fresh run includes the same eight envelope corners and twelve sealed interior recipes. Each case must contact the torso, keep both feet loaded for 2,000 steps, respect joint limits, reach the wrist target and recover within the established interval. The independent verifier checks every waist motor target and the settled encoder measurement. [Paired manifest](matched-suite.json) · [Fresh manifest](suite.json) · [Authored controller](controller.json)

## Results

| Cohort | Reaches meeting every criterion | Both feet loaded | Worst settled wrist error | Trial records |
| --- | ---: | ---: | ---: | --- |
| Previously disclosed impacts | 20/20 | 40,000/40,000 steps | 3.973 mm | [Results](matched.json) · [Raw archive](matched.zip) |
| Fresh impacts | 18/20 | 40,000/40,000 steps | 3.974 mm | [Results](a.json) · [Raw archive](a.zip) |
| Repeat of fresh impacts | 18/20 | 40,000/40,000 steps | 3.974 mm | [Results](b.json) · [Raw archive](b.zip) |

Both fresh runs failed cases 9 and 11. The block struck the torso, then remained against the left ankle through step 2,000. The robot maintained its stance and reached the wrist target. The required interval after final contact never began. All 60 trials registered a torso impact and zero measured actuator-guard bypasses. The standing release gate remains open. [Impulse ranges and audit results](measurements.json)

The native offscreen journey used DeepSeek through Octos to select the heading, checked every waist command and the settled encoder value, exported the episode and opened it in Replay. Headless Evidence independently verified the fresh archive, navigated both failures and reopened the result after restart. [Native checks and sources](native.json) · [Native Evidence page](evidence.png)
