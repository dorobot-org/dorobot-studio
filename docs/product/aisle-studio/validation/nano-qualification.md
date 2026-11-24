# Verify a Nano three-cube layout evaluation

The simulator varies the initial locations of three colored floor cubes across the reference living room. The local controller receives the head and two wrist RGB streams, the Nano joint encoders and the robot and camera calibration. The evaluator measures cube positions, release, opposing finger contacts under load, the final 100 steps of tabletop contact and wheel support. Every requested cube must settle at least 35 mm inside the table boundary. A run also fails on robot–furniture contact, an actuator guard violation, an incomplete trace or an unverified result.

The preregistered simulation qualification requires 45 whole-task successes in 50 cases, 50 verified receipts and zero measured guard bypasses. The three cohorts use independent random layouts. Their aggregate scores describe performance in this living-room distribution. A paired comparison of controller revisions requires reusing the same layouts.

## Examine a completed evaluation

1. Obtain the archived evidence and the separately published `suite.json` from the same cohort directory. Keep them beside each other.
2. In Studio, open **AISLE Studio → Evidence → Batch evaluation**. Enter the absolute path to `evidence.zip` and click **Verify evaluation**.
3. Studio checks the manifest and controller hashes, opens each salted layout commitment, replays the recorded motor targets against the Nano URDF limits, recomputes the cube verdicts and checks the entire report. It shows the number of full successes, the denominator, the 95% Wilson interval, verified traces and measured guard bypasses.
4. Use **Next failure** and **Previous failure** to inspect the retained seed, cube or contact details. Reopening Studio restores the latest successfully verified evaluation.

## Repeat from the command line

From a configured Studio checkout, use the AISLE source selected for the original cohort. The archive can also be verified from the current checkout when it contains the matching Nano URDF and receipt format.

```sh
export PYTHONPATH=/absolute/path/to/AISLE/src
.venv-aisle/bin/python tools/aisle-validation/nano_suite_evidence.py verify \
  /path/to/evidence.zip --manifest-sha256 "$MANIFEST_SHA256"
```

For a new qualification, retain the frozen checkout and its matching dependency environment. Author the three-cube controller once and publish the public manifest and controller before evaluating. Keep the seed openings private until all trials finish. Each interrupted attempt consumes its original case slot.

```sh
.venv-aisle/bin/python tools/aisle-validation/nano_transfer_suite.py prepare \
  --controller /path/to/prepared/controller.json --suite /path/to/new-suite
.venv-aisle/bin/python tools/aisle-validation/nano_transfer_suite.py run \
  --suite /path/to/new-suite --manifest-sha256 "$MANIFEST_SHA256" --jobs 6
.venv-aisle/bin/python tools/aisle-validation/nano_suite_evidence.py pack \
  /path/to/new-suite /path/to/evidence.zip
```

Set `MANIFEST_SHA256` to the published digest printed by `prepare`. The runner exits with status 1 when a hidden cohort misses its gate and still writes its full report.

## Frozen cohorts

| Cohort | Preregistration | Manifest SHA-256 | Execution |
| --- | --- | --- | --- |
| A | [Manifest](nano-holdout-a/suite.json) · [controller](nano-holdout-a/controller.json) | `3f64faea894cd691c2c0d51c4e1905bbc2031bb724adb0d45d77b86a2f7af027` | [Frozen source](https://github.com/dorobot-org/dorobot-studio/tree/97c3201). The runner derived the requested colors from the frozen artifact. |
| B | [Manifest](nano-holdout-b/suite.json) · [controller](nano-holdout-b/controller.json) | `042605bd9781697c10b143c9b00a5e5879886f6930447a7acbfb8cbec9903767` | [Frozen source](https://github.com/dorobot-org/dorobot-studio/tree/a52204b). Each worker validated, executed and archived the exact controller bytes. This version also changed the grasp aperture and used RGB to avoid occupied release points. |
| C | [Manifest](nano-holdout-c/suite.json) · [controller](nano-holdout-c/controller.json) | `b4a2e3d27f93e3b52c49cafb7b66c4be7388cad5ea75cb77c09ccd042ce6fa89` | [Frozen source](https://github.com/dorobot-org/dorobot-studio/tree/0637697). The visible clear-point search also preflights the open-jaw withdrawal. |


### Cohort A

32 of 50 complete tasks passed (64%, 95% Wilson interval 50.1–75.9%). All 50 motor traces verified. The audit counted zero measured guard bypasses. This version missed the 45-success threshold. Four failures had all three cubes placed successfully but lost loaded wheel contact for 1, 12, 1 and 9 physics steps respectively. These remained whole-task failures. The full report discloses every seed and salt and retains all 18 failures. [Scored report](nano-holdout-a/report.json) · [Raw evidence for independent verification](nano-holdout-a/evidence.zip)

[Native 50-case verification and restart](nano-holdout-a/native-verification.json) · [Studio Evidence screenshot](nano-holdout-a/studio.png)

### Cohort B

36 of 50 complete tasks passed (72%, 95% Wilson interval 58.3–82.5%). All 50 motor traces verified with zero measured guard bypasses. This cohort also missed the 45-success threshold. Four trials found no reachable unoccupied release point, four encountered an unreachable arm waypoint, three placed every cube but lost loaded wheel support for one step, one lost the red cube in transit and two missed a placement criterion. Each independent cohort provides a separate estimate. [Scored report and disclosed seeds](nano-holdout-b/report.json) · [Exact controller, motor logs and evidence for every case](nano-holdout-b/evidence.zip)

The native Evidence page also rejected an unreadable replacement archive and restored the last verified 50-case result after restart. [Native checks](nano-holdout-b/native-verification.json) · [Evidence screenshot](nano-holdout-b/studio.png)

### Cohort C

The [frozen simulator and controller](https://github.com/dorobot-org/dorobot-studio/tree/0637697) preflight both the loaded approach and the open-jaw withdrawal before selecting a visible clear tabletop location. This cohort uses the same DeepSeek task program and gate as B. The release candidate passed both nominal development layouts and three of four previously disclosed withdrawal failures. Those six cases do not count toward this qualification. All 50 new layouts were committed before running under manifest SHA-256 `b4a2e3d27f93e3b52c49cafb7b66c4be7388cad5ea75cb77c09ccd042ce6fa89`. [Public manifest](nano-holdout-c/suite.json) · [Exact controller](nano-holdout-c/controller.json)

The folded-arm three-cube demo also passed on this candidate with its original camera-selected blue, green, red order, 27,282 grounded steps and zero furniture contacts. A separate change to smooth every wheel command during target acquisition fixed two short open-loop loss-of-support examples but passed only one of four closed-loop development trials. Cohort C retains the original wheel behavior. [Every development trial and motor digest](nano-holdout-c/development.json)


**43 of 50 complete tasks passed** (86%, 95% Wilson interval 73.8–93.0%). All 50 audited traces verified with zero measured guard bypasses. The result missed the preregistered threshold by two trials. Cases 10 and 14 settled all three cubes but each lost one loaded-wheel tick. The remaining failures were one unconfirmed blue grasp, two lost red transports, one lost green transport and one blue cube without the required final tabletop load. The original seeds, input commitments and full denominator remain available. [Scored report](nano-holdout-c/report.json) · [Raw evidence archive](nano-holdout-c/evidence.zip)

The archive SHA-256 is `77cf1ee21be8041ca0ad9d332d48ef55978ce8ad87c1df85ea35399e2ce31857`. The three cohorts scored 32/50, 36/50 and 43/50. Their independent draws do not establish a paired treatment effect.

The native headless and offscreen sessions each independently verified all 50 cases, navigated the failed layouts, rejected a missing replacement and restored cohort C on restart. [Native verification](nano-holdout-c/native-verification.json) · [Studio Evidence page](nano-holdout-c/studio.png)

## Cohort D

The wrist-confirmed supported recovery met the preregistered gate on **45 of 50 fresh layouts**, with 50 verified records and zero measured guard bypasses. The independent cohorts scored 32/50, 36/50, 43/50 and 45/50. The 45/50 result measures this frozen controller on cohort D. [Protocol, all five failures, raw evidence and native verification](nano-holdout-d/README.md)

[Matched replay of all 50 disclosed cohort D layouts and per-case motor journals](nano-d-matched-replay/README.md)
