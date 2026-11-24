# Nano three-cube qualification D

The Nano uses its onboard RGB cameras and joint encoders to find three floor cubes and place them on the coffee table. The judge runs separately in Nexus. Each trial uses the same frozen DeepSeek/Octos controller artifact as cohort C.

In C, the robot lost the cube twice in three layouts. A probe reproduced the original motor journal in each case. Five of the six losses began during a base turn and one during arm elevation. Moving into a supported wrist posture on every carry fixed six of seven disclosed failures, but the passing control then placed green 33.7 mm from the edge. The required clearance is greater than 35 mm.

The selected controller keeps the first two carries unchanged. After two wrist-confirmed transport failures it attempts one more pickup and rotates the fixed finger under the cube before driving. All three disclosed transport cases then passed, with wheel support throughout. The passing control produced the original motor journal. [Development measurements](development.json)

## Preregistered decision

Run each of the 50 new committed layouts once. Retain failed and interrupted attempts. Require at least 45 complete three-cube transfers, valid receipts for every layout and zero actuator-guard bypasses. Each cube must have an opposing-finger airborne grasp, rest independently on the tabletop through the final 100 ticks, exceed the edge-clearance threshold and meet the existing settling criteria. Full wheel support and collision checks apply. The manifest pins the scene, robot, policy, judge, dependencies and controller before evaluation. [Manifest](suite.json) · [Controller](controller.json)

## Qualification result

**45/50 complete three-cube transfers passed** (90%, Wilson 95% interval 78.6–95.7%). All 50 receipts verified across 1,360,974 physics ticks. There were zero measured guard bypasses and zero rejected motor writes. The frozen simulation controller met the preregistered gate. [Case-level results](report.json) · [Full motor and action archive](evidence.zip) · [Independent verification](verification.json)

| Failed layout | Measured outcome |
| --- | --- |
| 0 | All three cubes settled. One of 31,618 physics ticks lost wheel support. |
| 19 | The controller could not align blue. Its final edge clearance was 15 mm. |
| 26 | Red was lost on the supported recovery. The wrist also contacted the coffee table. |
| 29 | Blue settled with 25 mm edge clearance. The gate requires greater than 35 mm. |
| 37 | Green was lost on the supported recovery. |

The archive SHA-256 is `e9eca578e5f264b8cffcc9dbaa0b42800cd6aa1255fd839b86723dd7c59bbe9f`. Native headless and offscreen Studio verified the same 50-case archive, opened the failure details, rejected a missing replacement and restored the successful qualification verdict after restart. The result applies to this URDF, controller, frozen runtime and randomized three-cube scene family. [Native checks](native-verification.json) · [Evidence page](studio.png)
