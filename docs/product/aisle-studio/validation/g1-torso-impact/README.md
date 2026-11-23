# Repeat G1 standing reach with a calibrated torso impact

The original 20-case cohort passed. On the matched rerun, case 9 produced zero contact impulses under both controllers. Across six additional replays of this disclosed case, the original frozen runtime registered an impact once in three attempts and the newer runtime registered none in three. G1 kept both feet loaded in every replay. [All six measurement records](legacy-repeats.json) · [Original qualification](../g1-standing-reach/README.md)

The revised fixture accounts for gravitational drop. The neutral G1 torso mesh spans +0.035 to +0.356 m vertically in the pelvis frame. The fixture launches from x = 0.29 m and compensates for the flight time to x = 0.05 m, with a target height of 0.195 m. Mass, horizontal speed, offsets, shoulder command, wrist tolerance, balance, actuator limits and recovery criteria retain their original ranges. Each trial now also requires positive normal contact on the actual torso link. The fixed calibration belongs to the evaluator. Policy inputs remain the simulated IMU and robot encoders. Existing archives identify their original launch conditions.

## Predeclared replication

Execute all 20 cases twice in independent Nexus processes using the exact same inputs. Each cohort contains the eight boundary recipes and twelve newly sealed interior recipes. Each of the 40 runs must satisfy the ten-second standing-reach criteria, including continuous two-foot support, 10 mm settled wrist tolerance, monitored torso contact and the post-impact stability window. A missing impact or missing trial counts as a failure. Each case receives one attempt per replicate. Both replicates were specified together before any case ran. [Public commitments and source digests](suite.json) · [Exact DeepSeek task](controller.json)

The manifest SHA-256 for both replicates is `135fdc294115e43d39a008a68a48c7758721ff353de519d013a635f5ad89b021`.
