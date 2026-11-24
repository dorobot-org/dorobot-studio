# Nano three-cube qualification D

The Nano uses its onboard RGB cameras and joint encoders to find three floor cubes and place them on the coffee table. The judge runs separately in Nexus. Each trial uses the same frozen DeepSeek/Octos controller artifact as cohort C.

In C, the robot lost the cube twice in three layouts. A probe reproduced the original motor journal in each case. Five of the six losses began during a base turn and one during arm elevation. Moving into a supported wrist posture on every carry fixed six of seven disclosed failures, but the passing control then placed green 33.7 mm from the edge. The required clearance is greater than 35 mm.

The selected controller keeps the first two carries unchanged. After two wrist-confirmed transport failures it attempts one more pickup and rotates the fixed finger under the cube before driving. All three disclosed transport cases then passed, with wheel support throughout. The passing control produced the original motor journal. [Development measurements](development.json)

## Preregistered decision

Run each of the 50 new committed layouts once. Retain failed and interrupted attempts. Require at least 45 complete three-cube transfers, valid receipts for every layout and zero actuator-guard bypasses. Each cube must have an opposing-finger airborne grasp, rest independently on the tabletop through the final 100 ticks, exceed the edge-clearance threshold and meet the existing settling criteria. Full wheel support and collision checks apply. The manifest pins the scene, robot, policy, judge, dependencies and controller before evaluation. [Manifest](suite.json) · [Controller](controller.json)
