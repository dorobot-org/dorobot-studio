# Nano camera-selected placement on 50 hidden layouts

The candidate completed **38/50** full three-cube transfers (76%, 95% Wilson interval 62.6–85.7%). The preregistered threshold was 45. All 50 motor journals verified and the independent replay found zero guard bypasses. It inspected 95 synchronized occupied-table and turning packets. The candidate remains experimental. The integrated workbench keeps the continuous-tracking controller that qualified on cohort F.

In this candidate, an exhausted stationary release search could request a short turn when two earlier colors were clearly visible, or a slow steered sidestep when one was visible. On five previously disclosed failures it recovered four transfers and retained all four regression controls. G used 50 fresh layouts with commitments and source hashes published before dispatch. F and G shared no seeds. Their totals are independent qualification outcomes. [Original G commitment](https://github.com/dorobot-org/dorobot-studio/commit/d38d9b3c9fb56e7f9e63de5f622aff774e866cdf) · [Candidate commit used for the run](https://github.com/dorobot-org/dorobot-studio/commit/3bec9043281f5a331491d20cf3c918aa6639b413) · [Registered protocol](protocol.md) · [Qualified F controller](../cohort-f/README.md)

## What happened during the 50 trials

| Placement command observed in the action log | Layouts | Complete transfers |
| --- | ---: | ---: |
| No base reposition requested | 45 | 37 |
| Turn requested | 3 | 0 |
| Sidestep requested | 2 | 1 |

The largest absolute sideways travel commanded for one cube was 78.2 mm. Two layouts had at least one unloaded wheel-contact step. None recorded robot–furniture contact. Some failures had more than one cause. The table gives a primary observed problem for each failed layout.

| Index | Seed | Observed stop or scored failure |
| ---: | ---: | --- |
| 8 | 3251334093 | Lost red during carry retries. |
| 10 | 3610842878 | Could not confirm the blue grasp in the wrist camera. |
| 14 | 2477546864 | Could not identify an earlier cube before changing the placement pose. |
| 15 | 1470892736 | Lost blue during carry retries. |
| 16 | 500052062 | No earlier cube matched across the placement turn. |
| 19 | 3949413544 | Timed out docking to the table. |
| 23 | 3012640272 | Earlier cube images disagreed about motion during the turn. |
| 25 | 4038521605 | No reachable clear release location. |
| 30 | 433391573 | Lost loaded wheel contact for nine physics steps. |
| 36 | 1769054292 | Could not confirm the red grasp in the wrist camera. |
| 46 | 2864151294 | Blue missed tabletop contact throughout the final 100 steps. |
| 47 | 326165322 | No reachable clear release location. |

[Complete verdicts and all measured failure criteria](verification.json) · [All 50 trial records](evidence.zip) · [Published output receipt](evidence.zip.sha256) · [Frozen inputs](suite.json) · [Historical source bindings](provenance.json)

## Open the result

In **AISLE Studio → Evidence → Batch evaluation**, enter the absolute path to this directory's `evidence.zip` and select **Verify evaluation**. The native screen shows 38/50 and preserves all 12 failed layouts for inspection. Restart Studio to reopen the verified result.

To reproduce the archive audit from the Studio checkout with its AISLE Python environment active, run the following command.

```sh
guide=docs/product/aisle-studio/validation/nano-release-workspace
python "$guide/audit_qualification.py" "$guide/cohort-g" --check
```

A subsequent development revision required the cameras to confirm the sidestep against encoders and capped cumulative commanded lateral travel at 140 mm per cube. It retained all four controls but recovered two of the five earlier failures. It stopped on inconsistent turn images or missing final landmarks in two cases the G controller passed. All nine motor records verified with zero guard bypasses. [Guarded revision and the nine measured outcomes](https://github.com/dorobot-org/dorobot-studio/pull/79)

The native journey opened two failure records, rejected a missing archive and restored the published result after restart. [Native Evidence](evaluation.png) · [Instrumented actions and verified counts](native.json)
