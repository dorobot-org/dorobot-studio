# Verify the distance commanded during Nano placement

The wheel driver now tracks the total absolute sideways travel commanded for each cube over the episode. Braking, changing direction and revisiting the placement phase preserve that total. A command exceeding 140 mm is rejected before any wheel targets are written. The existing 18 mm/s lateral limit, zero forward speed and 0.2 rad/s steering limit still apply. The cap bounds commands. Actual base motion can differ because the wheels can slip.

After a sidestep, the controller measures the movement of the same previously placed color along the table rim in fresh wrist images. A cube can move from one camera's field of view to the other's. If no usable color is shared, or any optical displacement differs from the encoder estimate by more than 25 mm, the robot holds the cube. The arm plans around earlier obstacles using the optical displacement. During a turn the controller again tracks successive observations and brakes if they lose their common landmark or disagree on motion.

The development suite reuses the five disclosed failed layouts and four controls to measure what these checks change. All nine trials count, including stops and earlier wheel gaps. The original F and G runs use their previously frozen controllers. [Inputs for the nine disclosed layouts](suite.json)
