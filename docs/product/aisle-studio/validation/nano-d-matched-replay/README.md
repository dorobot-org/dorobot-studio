# Nano matched replay after integration

The original wrist-confirmed carry recovery reached 45/50 on a blind three-cube evaluation. This replay uses the same controller bytes and all 50 previously disclosed seeds on Studio revision [43dc179](https://github.com/dorobot-org/dorobot-studio/commit/43dc17959610f1f695b2f66b7ae45dda87409bdd). The original success threshold applies to the blind run. The matched run measures reproducibility after integrating the Nano grasp detector and G1 work.

| Measurement | Matched replay |
| --- | ---: |
| Complete three-cube trials | 45/50 |
| Independently verified pairs | 50/50 |
| Same pass or failure | 50/50 |
| Identical motor target journals | 50/50 |
| Identical action journals | 50/50 |
| Identical physics tick counts | 50/50 |
| Total physics ticks | 1,360,974 |
| Measured guard bypasses | 0 |

The comparison opens both archives, independently replays the actuator audits, checks the scene recipe, robot model and requested colors, then joins the records by seed. A run with missing motor evidence cannot count as an identical motor replay. Two independently sampled cohorts with different seeds are rejected. [Pairwise receipts and source hashes](comparison.json) · [Matched manifest](suite.json) · [Matched raw evidence](matched-evidence.zip) · [Original blind cohort](../nano-holdout-d/README.md)

From the Studio checkout, with the AISLE environment active, reproduce the comparison with the original and matched archive digests.

```sh
base=docs/product/aisle-studio/validation
.venv-aisle/bin/python tools/aisle-validation/nano_suite_evidence.py compare \
  "$base/nano-holdout-d/evidence.zip" "$base/nano-d-matched-replay/matched-evidence.zip" \
  --baseline-manifest 949e2dc1e704cbad9b461ad203b8fb42b6d3e76aae4459211f3f3a4d6b53bb61 \
  --candidate-manifest 4a038b9e6bf44054d1bc7caf235a1c87a301e53d54f9b64623f83a50d2aa2d98
```
