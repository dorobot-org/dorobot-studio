# Verify the distance commanded during Nano placement

The wheel driver now tracks the total absolute sideways travel commanded for each cube over the episode. Braking, changing direction and revisiting the placement phase preserve that total. A command exceeding 140 mm is rejected before any wheel targets are written. The existing 18 mm/s lateral limit, zero forward speed and 0.2 rad/s steering limit still apply. The cap bounds commands. Actual base motion can differ because the wheels can slip.

After a sidestep, the controller measures the movement of the same previously placed color along the table rim in fresh wrist images. A cube can move from one camera's field of view to the other's. If no usable color is shared, or any optical displacement differs from the encoder estimate by more than 25 mm, the robot holds the cube. The arm plans around earlier obstacles using the optical displacement. During a turn the controller again tracks successive observations and brakes if they lose their common landmark or disagree on motion.

The development suite reuses the five disclosed failed layouts and four controls to measure what these checks change. All nine trials count, including stops and earlier wheel gaps. The original F and G runs use their previously frozen controllers. [Inputs for the nine disclosed layouts](suite.json)

## Results

| Disclosed E layout | Prior camera-selected maneuver | Guarded maneuver |
| --- | --- | --- |
| 9 | Placed all three | Braked because the turn images disagreed about movement of an earlier cube. |
| 15 | Placed all three | Placed all three. The red landmark crossed between wrist cameras and agreed with encoder travel. The motor journal was identical. |
| 19 | Placed all three | Held green. Neither final wrist image gave a complete red silhouette to confirm the sidestep. |
| 36 | Placed all three | Placed all three with an identical motor journal. |
| 16 | Held green | Held green after the cameras disagreed. The motor journal was identical, including the earlier wheel-contact gap during red localization. |

The revised checks recovered **2 of the 5** previously failing layouts. All four regression controls passed with identical motor journals, giving **6/9 overall**. All nine motor records and 20 synchronized release exposures verified. There were zero guard bypasses. This revision fell short of the four-of-five development criterion for another hidden qualification. The two additional holds are retained.

Native Studio displayed each failure and restored the same result after restart. [Native screen](evaluation.png) · [Native interaction record](native.json) · [All nine trial records](evidence.zip) · [Archive digest](evidence.zip.sha256)
