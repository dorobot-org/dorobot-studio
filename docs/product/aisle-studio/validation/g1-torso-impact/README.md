# Repeat G1 standing reach with a calibrated torso impact

The original 20-case cohort passed. On the matched rerun, case 9 produced zero contact impulses under both controllers. Across six additional replays of this disclosed case, the original frozen runtime registered an impact once in three attempts and the newer runtime registered none in three. G1 kept both feet loaded in every replay. [All six measurement records](legacy-repeats.json) · [Original qualification](../g1-standing-reach/README.md)

The revised fixture accounts for gravitational drop. The neutral G1 torso mesh spans +0.035 to +0.356 m vertically in the pelvis frame. The fixture launches from x = 0.29 m and compensates for the flight time to x = 0.05 m, with a target height of 0.195 m. Mass, horizontal speed, offsets, shoulder command, wrist tolerance, balance, actuator limits and recovery criteria retain their original ranges. Each trial now also requires positive normal contact on the actual torso link. The fixed calibration belongs to the evaluator. Policy inputs remain the simulated IMU and robot encoders. Existing archives identify their original launch conditions.

## Predeclared replication

Execute all 20 cases twice in independent Nexus processes using the exact same inputs. Each cohort contains the eight boundary recipes and twelve newly sealed interior recipes. Each of the 40 runs must satisfy the ten-second standing-reach criteria, including continuous two-foot support, 10 mm settled wrist tolerance, monitored torso contact and the post-impact stability window. A missing impact or missing trial counts as a failure. Each case receives one attempt per replicate. Both replicates were specified together before any case ran. [Public commitments and source digests](suite.json) · [Exact DeepSeek task](controller.json)

The manifest SHA-256 for both replicates is `135fdc294115e43d39a008a68a48c7758721ff353de519d013a635f5ad89b021`.

## Results

| Outcome | First execution | Independent repeat |
| --- | ---: | ---: |
| Robot received a measured physical block impact | 20/20 | 20/20 |
| Wrist settled within 10 mm and recovery completed in 0.50 s | 20/20 | 20/20 |
| Continuous two-foot support | 40,000/40,000 steps | 40,000/40,000 steps |
| Worst settled wrist error | 2.625 mm or less | 2.626 mm or less |
| Full preregistered pass including positive contact on the named torso link | **17/20** | **17/20** |
| Verified traces and measured motor guard bypasses | 20 verified, 0 bypasses | 20 verified, 0 bypasses |

Cases 2, 6 and 18 contacted only `logo_link` in both executions. The G1 URDF fixes that link to `torso_link` through `logo_joint`. All six outcomes failed because the predeclared criterion requires positive impulse on `torso_link`. The stricter 40/40 qualification gate was missed. The archives retain the loaded links and impulses for every case. [First report](replicate-a.json) · [First raw archive](replicate-a.zip) · [Repeat report](replicate-b.json) · [Repeat raw archive](replicate-b.zip) · [G1 URDF](../../../../../data/g1/g1.urdf)

A separate native DeepSeek trial measured contact on the head, logo and torso links and returned to the stable window in 0.50 s. The Evidence page identifies the contacted links. [Operator trial and restart](native-trial.json) · [Native Evidence page](torso-evidence.png)

Studio independently reopened both complete archives, navigated the failed cases, rejected a missing replacement archive and restored the last verified result on restart. [Native headless and offscreen checks](native-batch.json) · [Batch result in Studio](replicated-evidence.png)
