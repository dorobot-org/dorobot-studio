# Select a placement maneuver from visible cubes

The Nano first tries the existing stationary release. If no slot is clear and reachable, it counts the distinct previously placed colors with complete, unambiguous RGB silhouettes. With two prior colors visible it can turn and register the measured tabletop between wrist views. With one prior color visible it shifts the base slowly while keeping the observed rim heading. With no reliable landmark it holds the cube.

The shift is at most 14 cm. Lateral speed is limited to 18 mm/s and steering correction to 0.2 rad/s. The sideways command is accepted only in experimental placement with zero forward speed. The controller checks the measured table standoff and heading after every wheel command. Both maneuvers brake, acquire a fresh observation, confirm the cube in the gripper, retain earlier obstacles and check the full arm path before release. A slipping wheel can affect the sidestep odometry. The independent judge still requires physical finger contact, continuous wheel support and three separately settled cubes.

| Previously failed E layout | Observed maneuver | Three-cube result |
| --- | --- | --- |
| 9 | Turn | Passed |
| 15 | Steered sidestep | Passed |
| 16 | Steered sidestep | Stopped with green held. No measured slot passed the final check. Earlier in the trial the base had lost loaded wheel support for one tick. |
| 19 | Steered sidestep | Passed |
| 36 | Turn | Passed |

Four of five disclosed failures were recovered. All five motor journals verified and none recorded a guard bypass. The two turning runs and the three sideways runs reproduced the motor journals of their individually tested maneuvers. These development seeds were used to design the selection rule. [Verified trial archive](evidence.zip) · [Development inputs](suite.json) · [Independent cohort G protocol](../cohort-g/protocol.md)
