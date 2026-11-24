# AISLE Studio product requirements

Version 1.0 · 2026-11-23 · Proposed product specification

## Purpose and users

A robotics developer describes a task, reviews the generated controller, runs it in Nexus, diagnoses a failed attempt, and tests a revised version. A lab operator configures the robot, watches measurements and stops a run. An evaluator can reconstruct the result from immutable inputs and retained measurements. The development loop changes software between runs. The execution loop observes sensors and sends bounded commands through the robot guard.

The workbench lives in DoRobot Studio. Existing Robot control, Data collection, Replay and Training & inference retain ownership of their workflows. Nexus Studio contributes the scene, robot, validation and run-history concepts. Google's Robotics Plugin contributes the reference workflow for skills, simulation-first development, perturbation testing and migration to hardware. Related implementations are [Studio](https://github.com/dorobot-org/dorobot-studio/tree/8ff8c32), [Nexus Studio](https://github.com/dorobot-org/dorobot-nexus/tree/959b67b), [AISLE](https://github.com/moxin-org/AISLE/tree/dbd34a7) and [Google Robotics](https://github.com/google-deepmind/robotics-antigravity-plugin). The Google plugin uses Apache 2.0. Dependencies, assets and model services retain their own terms.

## Reference tasks and product outcomes

| Profile | First integration | Beta qualification |
| --- | --- | --- |
| Adora Nano | Import the actual extended-arm URDF, drive a bounded joint in a stationary inspection fixture, render a fresh observation, verify measured response | Use the three onboard RGB cameras and encoders to find and move three floor cubes onto the coffee table. Check reachability, ground contact, joint limits, obstacle clearance, physical grasp, release and settled placement |
| Unitree G1 | Import the 29-DOF URDF, read limits, inspect a bounded arm movement on a supported fixture, record and judge the measurements | In a floating-base simulation, stand, track a target and recover from a bounded disturbance. Measure balance, foot contacts, tracking error and energy. Enable whole-body execution after the profile has a validated controller |

For Nano's beta, the entire three-object task must succeed on at least 45 of 50 predeclared hidden layouts with zero guard bypasses. Retain every failure. The selected G1 qualification is a standing reach under bounded impacts. All eight envelope corners and twelve sealed interior cases must satisfy the wrist, support, recovery and actuator limits in the [preregistered protocol](validation/g1-standing-reach/README.md). Fixed-base joint inspection is a separate qualification on both profiles. A camera used by the operator to inspect a robot must be labeled separately from onboard policy observations.

## Page contracts

| Page | Main user decisions and actions | Data, feedback and acceptance |
| --- | --- | --- |
| Task | Select robot, enter task, author or revise a controller, inspect the change, choose a revision. Offer a runnable reference controller for first use | Bind robot, task text, controller version and capabilities. Show invalid or unsupported requests. New drafts retain earlier results. Any model-generated command is constrained by the profile |
| Scene | Inspect selected robot and physical scene, joint names, units and limits. Load a versioned simulation recipe. In beta import calibrated camera poses, reference photos and obstacles and preview changes | Resolve URDF and meshes before running. Record input hashes. Missing meshes or an unknown joint identify the failed input. Scene edits start a new revision. Private evaluator geometry stays outside policy input |
| Run | Start the selected task in Nexus, see the latest measured RGB image and joint feedback, cancel, return to another page and resume viewing | One local simulation per workbench session initially. Show starting, running, finished, stopped and failed states. Drop stale preview frames while retaining complete evidence. Termination closes the worker. A second launch cannot overwrite the first |
| Evidence | Open the latest recorded trial, see measured outcome, request and actual displacement, limits, frame counts and artifacts. Compare revisions on matched layouts in beta | Distinguish process completion from task success. Keep the camera frame, measured trajectory, controller, robot identity and verdict under a unique run directory. Failed or cancelled jobs remain inspectable on restart |

Existing Data workspace handles episode curation and replay. The full product links AISLE episode IDs to LeRobot datasets, training selections, checkpoints and subsequent evaluations. The existing SO-101 ACT pipeline supplies the integration pattern.

## Functional acceptance by operator action

| Action | Required behavior | Evidence of acceptance |
| --- | --- | --- |
| Author from spatial language | An operator can request a visible target by relative location. Record the actual camera exposure, calibration and worksheet of robot-frame estimates supplied to the chosen provider. Bind the resulting program to the observed scene. | An identical instruction selects different colors when the camera sees different arrangements. Changing the scene invalidates the previous program before actuation. [Measured Nano approaches](validation/onboard-authoring/README.md) |
| Transfer authorship | An evaluator can take a selected, measured controller to a second provider. Preserve the task, parent hash, robot profile, model identifier and signed quantities. | Codex to DeepSeek G1 handoff, with the revised displacement measured after execution. [Native handoff](validation/provider-comparison.md) |
| Lose a live sensor | The local runtime can brake, wait for usable observations and recover, or stop the trial with the reason visible in Evidence. Record capture time separately from receive time. | Fresh, delayed and persistently frozen approach packets exercise the same Nano controller. [Timing experiment](validation/camera-timing/README.md) |
| Inspect a release claim | The operator opens a preregistered cohort, verifies all disclosed trials, sees sample size and interval, and drills into individual problems. Include missing traces and failed trials in the denominator. | The native archive reader independently checks the hashed inputs, disclosed layouts and motor envelopes. It restores the verified result after restart. [Batch verification](validation/nano-qualification.md) |
| Promote to hardware | A lab operator selects a matching physical adapter, validates calibration and monitored stop, dry-runs the exact program and retains its measured outcome beside the simulated one. | Requires measured-device evidence for each profile. Physical device access remains outstanding. |
| Scale or train | A researcher can schedule profile-compatible workers, recover a lease after disconnection, and link a dataset and trained checkpoint to a held-out evaluation. | Measure concurrent end-to-end throughput and matched checkpoint outcomes on the selected hosts. Single-host G1 now exports a real 29-joint episode, trains a bounded shoulder correction, and matches its checkpoint against the teacher on the same 20 impacts. Multi-host capacity remains open. [Dataset and matched checkpoint evaluation](validation/g1-standing-checkpoint/README.md) |

## Execution and integration requirements

1. AISLE owns experiment definitions, task evaluation and the provider interface. Use the actual AISLE Nexus adapter for local simulation. Expose model and engine identity, timing and artifacts to Studio. Makepad receives asynchronous results and never waits for inference or physics on its event thread.
2. The outer agent can create a reviewable controller revision from task text and public evidence. Initially use a constrained declarative controller for joint inspection. Broader skills add observe, navigate, pick, place, recover and hold. Version skill inputs, outputs and allowed observations. Show model identity when a model participates.
3. The runtime reads measured robot state. Sensor timestamps, units, coordinate frames and camera calibration are explicit. For autonomous Nano tasks, enforce the RGB-only boundary at the transport. Derive odometry and metric localization from allowed measurements. The evaluator alone receives privileged object and contact data.
4. Start the first inspection in a fixed-base fixture. Subsequently validate collision geometry, self-collision, support and dynamic contacts. Regulated local controllers own actuator updates. A model response cannot relax limits. Hardware deployment requires a calibrated matching device, stop path and a verified controller artifact.
5. Scene authoring accepts a description plus reference images and produces editable geometry, asset provenance, scale and physics diagnostics. The operator reviews the reconstruction. Run perturbations on lighting, initial placement, friction, calibration, latency and stale images. Pair runs by seed for baseline comparisons.
6. Multi-host execution uses capability discovery and job leases. Record measured preview cadence separately from physics cadence and inference latency. Start with one foreground stream and configurable background observation cadence. Benchmark the complete pipeline before setting capacity claims.
7. A real deployment binds the same controller artifact to a measured-device adapter. Record dry-run preflight, operator enable, stop and outcome. The Nano base, arms and cameras and the G1 locomotion stack require separate device qualification.

## Design and release gates

Use the established Studio sidebar, compact horizontal page tabs, native typography and controls. Show a single primary next action per stage. Use text for robot, source, running state and verdict. Use a readable layout at 1024 by 768 and 1440 by 900. Keep errors close to the action and preserve draft input after errors.

Score designs before implementation and actual UI separately. Each item is worth ten points: consistency with the shell, page identity, next-action clarity, robot identity, simulation context, state visibility, task-result separation, readable typography, grouping and spacing, artifact discoverability. Record each deduction and the reviewed image. Pass requires at least 90/100. Functional evidence is separate: instrument actual native page selection, robot switching, authoring, execution, cancellation, recovery and opening results. Headless event evidence and offscreen raster evidence answer different questions.

## Delivery sequence and ownership

| Stage | Deliverable | Release evidence |
| --- | --- | --- |
| Reference workbench | Four pages and local, reproducible joint-inspection task on both models | Two real AISLE runs, observable joint response, generated UX board, design score and headless page journeys |
| Autonomous Nano | RGB reconstruction, mobile localization, collision-aware approach, grasp and release | Independent per-cube verdicts and hidden-layout suite |
| G1 whole-body | Qualified controller, floating-base scene and disturbance evaluation | Reproducible standing and tracking suite with foot-contact evidence |
| Agent iteration and transfer | Multi-round diagnosis, scene authoring, paired regressions, datasets and real devices | Trace from request through modified program to verified outcome on each supported robot |
| Fleet | Remote workers, quotas, cancellation, reconnect and live view | End-to-end throughput with inference, streams and recording active |

Studio owns pages and artifact navigation. AISLE owns execution and scoring. Robot adapters own units, calibration and physical stop behavior. An unmet hardware or qualification gate remains visible in the release record.

## Capability comparison and release decisions

| Workflow | Google Robotics Plugin reference | DoRobot and AISLE starting point | Acceptance for parity |
| --- | --- | --- | --- |
| Author and revise | Task-specific skills, code generation and iterative controller tests | Task page stores constrained Codex, DeepSeek via Octos and reference controllers by robot. Revisions can consume the selected measured outcome and retain the parent controller hash. Evidence compares matching inspection trials | Feed the recorded failure and public sensor observations into the next revision, retain the diff, run a matched holdout and show whether success improved |
| Build a scene | MuJoCo Studio sidecar and scene generation from reference images | Native URDF preview, surveyed-image import, metric footprint and height editing, versioned collision boxes and calibrated camera mounts. The workbench preserves the current scene after a rejected edit and restores it across sessions. [Surveyed scene](validation/room-survey/README.md) | Load a room recipe or reference photo, edit geometry, validate scale and collisions, persist scene and camera revisions, reproduce the run |
| Observe and manipulate | RGB localization, visual servoing and motion planning | The inspection exposes a separate operator camera and measured shoulder angle. Nano also supports an RGB-only floating-base approach to one visible floor cube in a living room. Studio now also accepts a cube order, runs a mobile pick and transfer, and judges each released cube against independent contact and settling measurements. [Transfer evidence](validation/nano-table-transfer.md). A CLI prototype measured a physical lift and opposing finger contacts on each of three development layouts. [Native trials](validation/nano-rgb-approach.md) | Establish camera calibration and RGB-only policy I/O, localize a moving object, plan a reachable collision-free grasp, detect a miss and recover |
| Evaluate | Iteration tooling, simulated perturbations and sim-to-real comparisons | Recorded source and model hashes, tracking verdicts, named lighting and friction inputs, G1 development impact, blind Nano suite runner and action-by-action comparison on matching layouts. [G1 contact and work trace](validation/g1-standing.md) | Batch matched seeds, show success counts with failures and uncertainty, replay the first divergence and compare controller revisions |
| Bring up hardware | Device-specific calibration, control processes and shared sim/real bridges | Existing Studio hardware workflows and robot-specific reference URDFs | Qualify calibration, watchdog, stop and command units on each real adapter, deploy the exact approved revision and retain a paired sim/real report |
| Handoff to datasets | Example sharing and reusable controller programs | Evidence exports both reference robots with full joint vectors and operator RGB to a replayable LeRobot episode. Nano transfer also exports all three synchronized onboard cameras, encoder states, action vectors and calibration | G1 standing has a complete [dataset-to-checkpoint comparison](validation/g1-standing-checkpoint/README.md). Extend the training path to Nano manipulation |

The comparison uses the official [release](https://developers.googleblog.com/gemini-hydrogen-for-robotics/), [plugin skills at the reviewed revision](https://github.com/google-deepmind/robotics-antigravity-plugin/tree/3dde33820355c70fe8d8e33e42707cdcb742ceb4/plugins/robotics/skills), [repository license](https://github.com/google-deepmind/robotics-antigravity-plugin/blob/3dde33820355c70fe8d8e33e42707cdcb742ceb4/LICENSE) and [Antigravity feature documentation](https://antigravity.google/docs/features). The reference workbench now includes a grounded Nano transfer trial and a floating-base G1 shoulder-tracking trial. [G1 development evidence](validation/g1-standing.md). The hardware gate requires physical devices, measured calibration and an operator.

## Simulation acceptance record

| Gate | Evidence |
| --- | --- |
| Nano simulation qualification | The latest preregistered DeepSeek task passed 43/50 complete layout trials (95% Wilson interval 73.8–93.0%), with 50 verified motor records and zero measured guard bypasses. The release gate remains 45/50. [Cohorts A, B and C](validation/nano-qualification.md) |
| DeepSeek through Octos as the outer author | The same Nano program interface accepts either provider and records the model identity and generated controller. Five matched authoring requests and independently executed retry programs passed for both providers. Native Studio also handed a measured Codex G1 trial to DeepSeek, preserving the parent controller digest. [Provider comparison](validation/provider-comparison.md) |
| G1 development disturbance | An independent 0.25 kg block contacted the torso and pelvis. Native headless Studio recorded 2,000 two-foot support steps, final shoulder error under 0.000001 rad and a 0.50 s stable recovery. Positive actuator work is a PD estimate with the raw joint trace retained. The preregistered G1 [bounded standing-reach suite](validation/g1-standing-reach/README.md) passed all 20 cases, with 40,000 loaded-foot steps, a maximum settled wrist error of 2.624 mm and a 0.50 s recovery in every case. [Trial](validation/g1-standing.md) |
| Scene revision | Import a surveyed image, validate floor scale and collision boxes, edit an object's center and height, and bind the selected revision to a Nano run. The reference test exercises import, rejected and accepted edits, replay and restart. [Scene authoring](validation/room-survey/README.md) |

The native Evidence page opens batch archives, independently replays motor traces and shows the success interval and each failed layout. [Qualification protocol and results](validation/nano-qualification.md)
