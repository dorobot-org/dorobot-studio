# Codex → Nano → Nexus → Studio: three cubes

Codex operates the simulated Nano through JSON commands, reads measured state
and camera images, and places three floor cubes on a coffee table. DoRobot
Studio displays the running physics through its external live-stream socket.
The left arm performs all three pickups; the right arm remains parked.

## Exact model and Studio version

This demo uses Studio development branch `feat/nano-rust-runtime`, commit
`c76b6c5a8966235feed76dd95d4363ec6954c7bf`, checked against GitHub on 2026-11-16.
It is newer than `main` (`f218095`) and release `v0.1.0-rc.2` (`f73ea08`).
Local demo changes are on `demo/nano-three-cubes`.

The loaded model is [`data/adora_nano/adora_nano.urdf`](../../data/adora_nano/adora_nano.urdf),
SHA-256 `a03319b78dcf338cbd64bed7bdc2062066af6f8a189cdb2cb67f9ba1e9b6b627`.
Nexus renders its **28 visual STL meshes with the URDF materials**, separately
from its collision shapes. Studio renders the same source model.

This is the repository's Nano model, including its extended arms, not a
substituted SO-100 model. Its [provenance](../../data/adora_nano/provenance.json)
explicitly identifies the mirrored left-arm simulation override as **not a
hardware-validated assembly**. Camera optics and inertias are nominal.

Each run generates a derivative URDF adding two planar translation joints for
the base. Original model assets remain unchanged. This provides consistent
transport velocity in Nexus; it is not a wheel-traction or physical-control
validation. The arm and all three cubes use contact physics: no cube pose
writes, attachment, weld, or scripted object transport.

## Run and watch live

Tested on macOS Apple Silicon with Metal, Python 3.13.5, and sibling AISLE
checkout `../AISLE` at `845e31f5d1871905e69c9d838f90a32c62bd6476`.
AISLE itself is not modified by this demo.

Build Studio once from the repository root. Raw live textures do not require
the optional FFmpeg playback feature:

```sh
cargo build --locked -p dorobot-flex --bin dorobot-flex --no-default-features
```

In terminal 1, open Studio. It reconnects when the source becomes available:

```sh
target/debug/dorobot-flex --live-socket /tmp/nano-three-demo.sock --screen live
```

In terminal 2, execute the saved command plan in a **fresh** output directory:

```sh
uv run --project ../AISLE --frozen --python 3.13.5 \
  --with 'dimforge-nexus3d==0.2.1' \
  --with 'opencv-python-headless==5.0.0.93' \
  --with 'scipy==1.18.1' \
  python -u tools/nano-three-cubes/demo.py \
  --socket /tmp/nano-three-demo.sock \
  --output tools/nano-three-cubes/runs/my-run-001 \
  --plan tools/nano-three-cubes/plan.json

uv run --project ../AISLE --frozen --python 3.13.5 \
  python tools/nano-three-cubes/verify.py tools/nano-three-cubes/runs/my-run-001
```

The plan replays Codex's saved commands; it does not invoke a new Codex agent.
Omit `--plan` for interactive operation, supplying one JSON object per line:

```json
{"op":"select","cube":"red"}
{"op":"navigate","position":[-0.91,-0.73]}
{"op":"snapshot","name":"inspect"}
{"op":"finish"}
```

The command log records accepted and rejected commands with observations.
Navigation checks furniture, room bounds, and unlifted cubes before moving.
Codex supplies waypoints and IK targets using known scene geometry and measured
simulator poses. This is not visual SLAM or a general collision-free arm planner.
Nominal arm motors can contact the table during lowering; verification judges
the released objects, not an assumed success when commands finish.

## What Studio shows

- A movable 3D room view with Nano, furniture, and all three measured cube poses.
- Three synchronized, link-mounted cameras: head, left wrist, right wrist.
- Seventeen measured joint channels and their targets on the physics clock.
- Connection state, actual preview rate, stale-frame indication, and completion.

Use the 3D panel's maximize button, **View: Follow**, or drag/scroll to inspect
the task. The local UI fix makes `--live-socket` select an external-stream
layout, exposes reconnect/disconnect, and hides unrelated gamepad instructions.
Built-in controller sessions retain their separate layout.

Preview is sampled at **5 Hz of simulation time**, with a bounded latest-frame
transport. Rendering may run faster or slower than real time. Physics pauses
while waiting for an interactive command or computing an IK path; Studio then
honestly shows the last frame as stale. It resumes when new samples arrive.
The source is an external Python AISLE/Nexus process; Studio remains the latest
native Rust application. The built-in Rust gamepad simulator is not the source
for this Codex-controlled run.

## Evidence and limits

`verify.py` independently checks all three cubes: floor start, lift above the
table, held during base transport, placement inside table edges at tabletop
height, two seconds of stable rest, hand withdrawal, and open gripper. It also
checks joint limits, finite states, and the sample clock.

Runs contain `demo.mp4` (overview plus external grasp close-up), `commands.jsonl`,
`trajectory.json`, `verification.json`, model/source hashes in `manifest.json`,
and atomic `live.png` / `live.json`. `stream.json` records published frames and
connected clients. `observe_studio.py` can save the app's opt-in read-only
`DOROBOT_UX_REPORT` telemetry to independently confirm displayed samples.

The overview video is 20 fps in simulation time; it is distinct from Studio's
three live camera textures. This demo records video and measured JSON evidence,
not a finalized LeRobot episode. Studio's native Record/Save workflow and
physical Nano validation are separate.

## Completed interactive run

Local evidence is in `runs/probe-001/` (generated files are Git-ignored).
All **30 physical-outcome checks** passed over 250.3 seconds of simulation.
All three cube centres settled at 0.400 m on the 0.380 m tabletop, with zero
recorded joint-limit excess. `demo-h264.mp4` is the shareable overview video.

The Studio observer checked **557 displayed samples** from simulation time
138.8–250.2 s: all 17 joints, the base, and all three cube positions matched
the physics log exactly. All nine live-stream checks passed, including the
three camera identities and the end-of-stream notification. This observation
window covers the end of the green cycle and the complete blue cycle.
Studio was also visually inspected during red pickup and blue transport.

The interactive command log retains one rejected navigation segment near blue.
No physics advanced on rejection; Codex supplied a clearance waypoint and
completed the task. The interactive CLI reports exit status 1 because it retains
command errors; `verification.json` and `studio-verification.json` both report
`ok: true`. `plan.json` contains the **46 accepted commands**, excluding that
rejection. A negative verification check also confirmed that leaving blue on
the floor fails the task.

Studio build, six live-protocol tests, four panel-layout tests, five Nano model
symmetry tests, and Python lint checks passed. To collect UI evidence on another
run, launch Studio with `DOROBOT_UX_REPORT=/tmp/nano-studio-report.json`, then run:

```sh
python3 tools/nano-three-cubes/observe_studio.py \
  --report /tmp/nano-studio-report.json --run tools/nano-three-cubes/runs/my-run-001

uv run --project ../AISLE --frozen --python 3.13.5 \
  python tools/nano-three-cubes/verify_studio.py tools/nano-three-cubes/runs/my-run-001
```

Start the observer while the source is running; run the verifier after completion.
