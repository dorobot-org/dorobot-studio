# Reference workbench acceptance

Tested on 2026-11-23 with the native Makepad app on Apple Silicon, AISLE `dbd34a7`, Nexus 0.2.1 on Metal and the Studio implementation at `c5d1a17`. The two robot roots were fixed in a zero-gravity inspection fixture. The evaluated quantity was the selected shoulder's measured angular displacement.

| Trial | Controller | Requested | Measured | Outcome |
| --- | --- | ---: | ---: | --- |
| Nano left shoulder, standard fixture | Reference | +0.10000 rad | +0.09993 rad | Passed |
| G1 right shoulder, compliant fixture | Codex from text | −0.08000 rad | −0.08005 rad | Passed |
| G1 right shoulder, compliant fixture | Codex revised from previous trial | −0.06000 rad | −0.06003 rad | Passed |

The second G1 instruction specified three quarters of the prior signed displacement. Codex received the previous measurements, chose the same joint and produced a revision linked to the previous controller hash. All three trials sampled 25 observations and checked the selected joint at every one of 121 physics steps. The supplied episode files contain the actual 17 or 29 joint readings and commanded targets. Each MP4 has 25 frames at 480 × 320. Each saved Parquet state, action and timestamp was compared with its source recording. The operator camera and its pixel statistics are identified in the dataset metadata. [Machine-readable measurements](measurements.json) · [Three Studio-readable LeRobot episodes](reference-episodes.zip)

## Native interaction checks

| Mode | Size and appearance | Records |
| --- | --- | --- |
| Makepad headless, authenticated Codex | 1024 × 768 dark | [24 checks](headless-dark/journey.json) and [2 recovery checks](headless-dark/recovery.json) |
| Makepad offscreen, authenticated Codex | 1440 × 900 light | [24 checks](desktop-light/journey.json), [2 recovery checks](desktop-light/recovery.json) and [nine screenshots](desktop-light/) |
| Makepad offscreen, authenticated Codex | 1024 × 768 dark | [24 checks](desktop-dark/journey.json), [2 recovery checks](desktop-dark/recovery.json) and [nine screenshots](desktop-dark/) |
| Makepad headless, local reference provider | 1440 × 900 light | [21 checks](headless-reference/journey.json) and [2 recovery checks](headless-reference/recovery.json) |

Each mode includes a manifest of the native binary hash, launched process IDs, observed window sizes and appearance, trial IDs and output hashes. [Headless](headless-dark/mode-manifest.json) · [Light desktop](desktop-light/mode-manifest.json) · [Dark desktop](desktop-dark/mode-manifest.json) · [Reference provider](headless-reference/mode-manifest.json)

The journeys used native pointer and keyboard events. They prepared controllers, inspected the URDFs, selected a gain profile, started each simulation, exported datasets, navigated saved evidence and controller history, stopped an active run and opened a 25-frame G1 episode in the existing Replay page. Restarting the app recovered the stopped trial and the G1 controller. The video-enabled offscreen build displayed the recorded operator camera beside the G1 model in Replay. [Replay screenshot](desktop-light/G1-replay.png)

The local Rust workspace suite passed 113 tests with video disabled. macOS exposes two spellings of its temporary directory. The existing Nexus test compares path strings, so the full suite was run with TMPDIR set to its canonical path. Three Python contract tests exercised rejected commands, mismatched feedback and shutdown of a worker and its child after the Studio pipe closes. Rust regression tests covered failed preparation retaining the previous controller and late cancellation retaining an already scored outcome. The rendered journeys used an external GPU-backed worker with the same released AISLE adapter in all four modes.

## Implementation UX review

Codex scored the rendered native pages separately from the generated design board. This is an internal heuristic review using the ten PRD criteria. The screenshots and measurements support independent rescoring. [Generated design review](../design-review.md)

| Criterion | Points | Evidence and deductions |
| --- | ---: | --- |
| Shell consistency | 10 | Existing Studio shell, plugin navigation, light and dark appearances |
| Page identity | 10 | Four distinct titles and active tabs |
| Next action | 9 | Prepare, inspect, run, export and replay are actionable. Task offers two entry points of similar prominence |
| Robot identity | 9 | Named profile and actual URDF. Operator geometry uses a uniform material |
| Simulation context | 10 | AISLE, Nexus and fixed-base fixture remain visible |
| State visibility | 8 | Running, completion, stopped and failed receipts are supported. Very short trials can complete before the user reads the intermediate status |
| Process and outcome | 10 | Run completion, measured endpoint, selected joint limits and scored outcome are visible separately |
| Readability | 8 | Controls fit both sizes. At the narrower width, long single-line task input scrolls horizontally |
| Grouping and spacing | 8 | Controls and artifacts group by task, with excess vertical space on wide desktop layouts |
| Artifact discoverability | 9 | Open folder, export and replay actions are explicit. Full provenance is in files |
| **Total** | **91/100** | Meets the internal 90-point gate |

The tests establish reference joint inspection, revision and dataset handoff. Room navigation, three-cube manipulation using onboard RGB, G1 balance, physical robot deployment and full Google workflow parity retain the separate gates in the [PRD](../PRD.md).
