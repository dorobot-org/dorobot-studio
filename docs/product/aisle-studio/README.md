# AISLE Studio

Open **AISLE Studio** in the DoRobot Studio sidebar. The four pages take an inspection from a written instruction to a measured robot response. Both the Adora Nano and Unitree G1 use their repository URDFs. Nexus advances the physics and renders the operator camera through AISLE's Python adapter. The Studio worker evaluates the retained measurements.

[Product requirements](PRD.md) · [Codex-generated UX board](workbench-design.png) · [Design review](design-review.md) · [Native UI and validation](validation/README.md)

## Run from a Studio source checkout

Use an Apple Silicon Mac with Metal, a working Studio Rust toolchain, Python 3.13 and uv. The tested AISLE source revision is `dbd34a71b6f82a011541b433306ca7e6a705e823`. Set `DOROBOT_AISLE_ROOT` to a checkout of that revision. To obtain a separate checkout from GitHub, run the following from the Studio repository root.

```sh
git clone https://github.com/moxin-org/AISLE.git ../aisle-workbench-runtime
git -C ../aisle-workbench-runtime checkout --detach dbd34a71b6f82a011541b433306ca7e6a705e823
export DOROBOT_AISLE_ROOT="$(cd ../aisle-workbench-runtime && pwd)"
uv venv --python 3.13 .venv-aisle
uv pip install --python .venv-aisle/bin/python numpy==2.5.5 pillow==13.0.0 dimforge-nexus3d==0.2.1 pyarrow==26.0.1 pyyaml==6.0.3
python3 tools/fetch_robot_meshes.py g1
cargo run --locked --no-default-features -p dorobot-flex --bin dorobot-flex -- --screen aisle-task
```

An existing checkout at `../AISLE` and a Studio-local `.venv-aisle/bin/python` are discovered automatically. Override the interpreter with `DOROBOT_AISLE_PYTHON`. **Ask Codex** requires an authenticated Codex CLI on PATH. **Use reference task** works locally without an inference service. The source workbench uses external Python and a GPU. Desktop release bundles require separate integration.

1. **Task** — select a robot. Use the reference task or enter “Inspect the right shoulder with a displacement of minus 0.08 rad” and choose **Ask Codex**. Read the joint, signed displacement, provider and revision. Select an earlier controller to repeat it.
2. **Scene** — inspect the mesh inventory, movable joint count and limits. Standard and Compliant select reproducible position gains of 28 and 18 respectively. Both fixtures anchor the root and compensate gravity.
3. **Run** — start the prepared controller. The operator camera and position feedback come from the active simulation. Stop cancels the job and retains a receipt. Each run has its own directory.
4. **Evidence** — read the requested and measured displacement, guard verdict and trace. Navigate older runs or open the run folder. A prior run with the same robot, URDF hash, fixture, joint and displacement supplies a comparison. After restarting Studio, saved controllers and completed or stopped trials appear again.

The inspection lasts 1.21 seconds of simulated time. It tests the selected shoulder against its URDF bounds at all 121 physics steps and saves 25 sampled angles. Success requires an absolute endpoint error below 0.025 rad. The external camera is an operator view. Autonomous RGB perception, floating-base contact and hardware qualification have separate requirements in the PRD.

Files live under the Studio user data directory in `aisle/`. On macOS the default is `~/Library/Application Support/DoRobot Studio/aisle/`. Each completed run retains the controller, model metadata, adapter and model hashes, most recent RGB image, sampled trajectory and verdict. Set `DOROBOT_STUDIO_STATE_DIR` to isolate an experiment.

## Reproduce the native checks

Build the remote input client from the Makepad revision pinned in Cargo.lock, then build Studio. From the Studio root, with the runtime configured as above, run these commands. Use a fresh output directory for each invocation.

```sh
cargo install --locked --git https://github.com/OctoSense-org/makepad.git --rev 44d8220f12acfa46157019267e119e0866dd6b05 makepad-remote
cargo build --locked --no-default-features -p dorobot-flex --bin dorobot-flex
python3 tools/aisle_workbench/verify.py --studio target/debug/dorobot-flex --backend headless --dark --size 1024x768 --codex --out /tmp/aisle-headless-proof
python3 tools/aisle_workbench/verify.py --studio target/debug/dorobot-flex --backend offscreen --size 1440x900 --codex --out /tmp/aisle-offscreen-proof
```

Use `--remote /path/to/makepad-remote` for a locally built client. Omit `--codex` to use reference controllers throughout. The driver sends pointer and keyboard events to the running application, restarts it with the same state directory and reads the saved trial files. The offscreen run also captures pixels. Both modes still use real Nexus physics and sensor rendering.
