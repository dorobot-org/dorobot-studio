# Select a placement maneuver from visible cubes

The Nano first tries the existing stationary release. If no slot is clear and reachable, it counts the distinct previously placed colors with complete, unambiguous RGB silhouettes. With two prior colors visible it can turn and register the measured tabletop between wrist views. With one prior color visible it shifts the base slowly while keeping the observed rim heading. With no reliable landmark it holds the cube.

The controller requests a sideways offset of 8 or 14 cm. Lateral speed is limited to 18 mm/s and steering correction to 0.2 rad/s. The sideways command is accepted only in experimental placement with zero forward speed. The controller checks the measured table standoff and heading after every wheel command. Both maneuvers brake, acquire a fresh observation, confirm the cube in the gripper, retain earlier obstacles and check the full arm path before release. A slipping wheel can affect the sidestep odometry. The independent judge still requires physical finger contact, continuous wheel support and three separately settled cubes.

| Previously failed E layout | Observed maneuver | Three-cube result |
| --- | --- | --- |
| 9 | Turn | Passed |
| 15 | Steered sidestep | Passed |
| 16 | Steered sidestep | Stopped with green held. No measured slot passed the final check. Both the stationary baseline and this run lost wheel support at tick 303 while locating red. The first sidestep began at tick 12806 while holding green. |
| 19 | Steered sidestep | Passed |
| 36 | Turn | Passed |

Four of five disclosed failures were recovered. All five motor journals verified and none recorded a guard bypass. The two turning runs and the three sideways runs reproduced the motor journals of their individually tested maneuvers. These development seeds were used to design the selection rule. [Verified trial archive](evidence.zip) · [Development inputs](suite.json) · [Independent cohort G protocol](../cohort-g/protocol.md)

Native Evidence verified the two output receipts, showed four gains and one layout failing both controllers, and restored the same comparison after restart. [Native Studio comparison](native-comparison.png) · [Instrumented actions and scores](native-comparison.json)

All four control layouts passed and reproduced the earlier control motor journals byte for byte. The four control seeds were 1724068725, 1064838320, 289513494 and 0. The 50 cohort G commitments were published in [d38d9b3](https://github.com/dorobot-org/dorobot-studio/commit/d38d9b3) before the five-case result in [9e7c37a](https://github.com/dorobot-org/dorobot-studio/commit/9e7c37a) and the regression verdict in [3bec904](https://github.com/dorobot-org/dorobot-studio/commit/3bec904). G started after both development suites finished. [Control evidence](../landmark-controls/evidence.zip) · [Control manifest](../landmark-controls/suite.json)

## What the cameras established during the sideways move

All three disclosed sidesteps requested an 80 mm shift. The absolute wheel commands integrated to 78.1–78.2 mm and the encoders indicated 77.7–77.8 mm. On layout 15 the red landmark moved about 75 mm along the measured rim, from the right wrist view to the left. On layout 19 neither camera had a complete red silhouette at the final exposure. On layout 16 the two wrist estimates disagreed. The G controller applies encoder travel to previously seen obstacles when placing after a sidestep. These measurements establish the limit of that estimate. The two-camera default uses a stationary release.

During a turn the G controller checks table standoff and heading after each command. It matches earlier cubes at the endpoint. A lost endpoint match stops the task after braking. The F controller also checks its landmark chain during the turn. The published output digest identifies the exact archived bytes. The verifier separately recomputes command, contact and transfer verdicts from those records.

## Native DeepSeek run on layout 19

DeepSeek V4 Flash through Octos authored the three-cube program in headless Makepad Studio. On seed 1865171047 the Nano used its wrist cameras and encoders to select a sidestep while holding green. It placed all three cubes, kept loaded wheel contact for all 28,504 physics steps, and recorded zero joint-limit excursions. The native motor file matched the original Codex-authored development run byte for byte. Studio exported a 1,425-frame three-camera episode with the same seed and reopened it after restart. The archive includes the provider identity, controller, measured result, motor file, export metadata and native interaction receipts. [Native DeepSeek evidence](native-deepseek-sidestep.zip)

On independently committed cohort G, the camera-selected controller completed 38/50 full transfers. All 50 journals verified with zero guard bypasses. The release threshold is 45. The workbench retained the continuous-tracking controller after this result. [All twelve failed layouts and native replay](../cohort-g/README.md)

The revised camera-selected draft checks cube landmarks throughout every turn and brakes if an exposure cannot be registered. The results above are from the archived cohort G controller. The revised candidate requires its own disclosed development run and a fresh qualification cohort.
