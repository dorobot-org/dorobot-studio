# G1 standing reach under bounded impacts

The operator selected a standing reach under a bounded disturbance for G1 qualification. Studio uses the actual 29-DOF G1 in a floating-base Nexus scene and a previously authored DeepSeek controller. Its −0.08 rad right-shoulder command produces approximately 23 mm of planned wrist travel. After warm-up, the evaluator fixes the target in world coordinates using the URDF and the initial measured attitude and joint positions. The controller consumes its simulated IMU and joint sensors. The evaluator reads wrist pose and floor and block impulses independently.

## Preregistered simulation envelope

The fixture launches a 65 mm cube toward the standing robot after three seconds. Mass ranges from 0.20 to 0.30 kg, speed from 1.10 to 1.30 m/s, and lateral offset from −15 to +15 mm. The suite contains all eight combinations of endpoints and twelve randomly selected interior recipes. The interior recipes remain sealed until scoring. The public manifest binds the exact task, URDF and meshes, execution and verifier source, and salted commitment for every recipe. [Manifest](suite.json) · [DeepSeek controller](controller.json)

Every one of the 20 complete trials must meet all of the following conditions.

- Record a physical block impact and 2,000 scored steps (10 seconds), both feet loaded throughout, with no other link contacting the floor.
- Produce at least 15 mm of requested wrist travel. Keep the measured wrist within 10 mm of the world-frame target for the final 100 steps.
- After the final impact, complete 100 consecutive stable steps within 2 seconds. Stability requires wrist error ≤10 mm, shoulder error <0.015 rad, tilt <4°, drift <30 mm and two-foot support.
- Across the trial, keep tilt below 12°, pelvis drift below 100 mm, measured joint excursion within 0.005 rad and all recorded targets within URDF position and rate limits. Finish within the smaller of 0.025 rad and 15% of the shoulder request.

This qualifies one simulated short reach within a specified forward-impact envelope. The eight corners are a deterministic challenge set. Counts for the combined set have no binomial confidence interval. The simulated estimated work uses capped PD effort and measured motion.

## Execute and verify

Use the [configured Python and AISLE runtime](../../README.md). Keep the independent public manifest alongside the archive in Studio. Select **Evidence → Batch evaluation**, enter the archive path and select **Verify evaluation**. The raw per-step wrist, motor and contact records are sufficient to recheck the target, measured travel, support, recovery, actuator envelope and all 20 results.

```sh
.venv-aisle/bin/python tools/aisle_workbench/g1_reach_suite.py prepare /tmp/new-g1-suite /path/to/controller.json
.venv-aisle/bin/python tools/aisle_workbench/g1_reach_suite.py run /tmp/new-g1-suite "$MANIFEST_SHA256" --jobs 2
.venv-aisle/bin/python tools/aisle_workbench/g1_reach_suite.py pack /tmp/new-g1-suite /tmp/evidence.zip
.venv-aisle/bin/python tools/aisle_workbench/g1_reach_suite.py verify /tmp/evidence.zip "$MANIFEST_SHA256"
```

The original manifest SHA-256 is `00386e6bdb7f118de306637d38814de61219c3be22c782316bb82817f11540b3`. Publish a new manifest before starting a new cohort. Each case runs once.
