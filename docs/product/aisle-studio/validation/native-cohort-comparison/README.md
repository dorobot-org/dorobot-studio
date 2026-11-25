# Compare two evaluations in Studio

Open **AISLE Studio → Evidence → Batch evaluation**. Put the candidate ZIP path in **Evaluation** and the earlier ZIP path in **Baseline**, then select **Compare with baseline**. Each ZIP needs its own corresponding public manifest named `suite.json` in the same directory. **Verify evaluation** opens the candidate on its own.

Studio rechecks each archive against its public commitment and replays its saved physics measurements. It pairs Nano trials by living-room seed and G1 trials by block mass, approach speed and lateral offset. G1 comparisons also require matching impact fixtures and scoring criteria. The result shows each archive's short SHA-256, pass count, and how many pairs gained, lost, passed both, failed both or had incomplete evidence. **Previous case** and **Next case** open the changed and unverified pairs, including both original case numbers and the measured failure reasons. The most recent verified result reopens after restart. A rejected comparison leaves the previous result available.

## Measurements

| Reference task | Baseline | Candidate | What changed |
| --- | --- | --- | --- |
| Nano, three cubes on the coffee table | Two-view release, 40/50 | Three-view release, 40/50 | Both passed 37 and both failed seven. The candidate gained layouts 34, 40 and 49, and lost 9, 19 and 36. [Original cohorts and release decision](../transfer-wrist-timing/README.md) |
| G1, standing reach under bounded frontal impacts | Upright trunk, 17/20 | 20° left waist turn, 20/20 | The candidate gained impact cases 9, 12 and 13. Each baseline failure involved the impact or stable-recovery criterion. [Upright controller](../g1-upright-trunk/README.md) · [20° controller](../g1-standing-heading/README.md) |
| G1, repeated 30° standing reach | First fresh run, 20/20 | Same recipes repeated, 20/20 | All 20 pairs had matching verified outcomes. [Original qualification](../g1-thirty-degree-reach/README.md) |

[Compact Nano comparison](nano-compact.png) · [Desktop G1 comparison](g1-desktop.png)

Seven native Makepad journeys opened comparisons and restored them after a process restart. Tests covered 1024 × 768 headless, 1024 × 768 offscreen and 1440 × 900 offscreen. The instruments rejected unrelated Nano seeds, different G1 recipes and cross-robot selections. Each reopened a single evaluation, restored the comparison, then restarted immediately after rejecting the incompatible archive. The previous verified comparison survived in all seven journeys. A separate diagnostic fixture reversed two known Nano layouts and omitted one completed physics result. Studio matched the reordered seeds and showed one verified pass and one unverified pair. [Native journeys, source manifests and restart records](native.zip)

## Reproduce a comparison

Run these commands from the Studio checkout with the [AISLE Python environment and instrumented Makepad build](../../README.md). The original measurements were collected in Nexus. Opening a comparison uses CPU replay of the recorded trials. Linux and macOS differed by 3.2 × 10⁻¹⁴ mm on the same G1 wrist trace. The verifier allows floating-point roundoff in the reported wrist error and positive work. It still recomputes each verdict from the original trace and requires exact agreement on the verdict, contacts, guards and cohort totals.

~~~sh
v=docs/product/aisle-studio/validation
work=$(mktemp -d)
mkdir "$work/baseline" "$work/candidate"
cp "$v/transfer-wrist-timing/matched-baseline.zip" "$work/baseline/cohort.zip"
cp "$v/transfer-wrist-timing/matched-baseline-manifest.json" "$work/baseline/suite.json"
cp "$v/transfer-wrist-timing/qualification.zip" "$work/candidate/cohort.zip"
cp "$v/transfer-wrist-timing/qualification-manifest.json" "$work/candidate/suite.json"
"$DOROBOT_AISLE_PYTHON" tools/aisle_workbench/verify.py \
  --studio "$studio_binary" --remote "$makepad_remote" \
  --backend headless --size 1024x768 --out "$work/native" \
  --compare-cohorts "$work/baseline/cohort.zip" "$work/candidate/cohort.zip"
~~~

For the G1 improvement, use `matched.zip` and `matched-suite.json` from `g1-upright-trunk` for the baseline and `g1-standing-heading` for the candidate. For desktop screenshots, select `--backend offscreen --size 1440x900`. The native journey also checks that a rejected input preserves the last good result.

To audit the checked-in native records against every original cohort row, run the following command with Python 3.11 or newer.

~~~sh
python3 docs/product/aisle-studio/validation/native-cohort-comparison/audit.py
~~~
