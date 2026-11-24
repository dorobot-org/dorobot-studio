# AISLE Studio

Open **AISLE Studio** in the DoRobot Studio sidebar. DeepSeek through Octos and Codex can each author simulation tasks on the repository’s Adora Nano and Unitree G1 models. The Nano can approach colored cubes using onboard RGB or transfer them from the floor to a coffee table. The G1 can track a shoulder target in a fixed fixture or while standing on both feet. AISLE drives the Nexus simulation, and Studio displays independent measurements for each trial.

[Watch DeepSeek place three cubes in observed left-to-right order](validation/onboard-authoring/three-cubes.mp4)

[Product requirements](PRD.md) · [Codex-generated UX board](workbench-design.png) · [Design review](design-review.md) · [Native UI and validation](validation/README.md)

[Author from the actual Nano camera view](validation/onboard-authoring/README.md) · [Test camera delay and recovery](validation/camera-timing/README.md) · [Verify a 50-layout Nano evaluation](validation/nano-qualification.md) · [Import and edit a surveyed living room](validation/room-survey/README.md) · [Train and evaluate the G1 standing checkpoint](validation/g1-standing-checkpoint/README.md) · [Inspect the latest repeated G1 frontal impacts](validation/g1-frontal-impact/README.md) · [Inspect the previous torso-impact evaluations](validation/g1-torso-impact/README.md) · [Verify the original 20 G1 standing reaches under bounded impacts](validation/g1-standing-reach/README.md) · [Stand and track a shoulder target on G1](validation/g1-standing.md) · [Move cubes to the table with DeepSeek via Octos](validation/nano-table-transfer.md) · [Three-camera demo](validation/nano-table-transfer.mp4) · [Replayable three-cube dataset](validation/nano-table-transfer-lerobot.zip) · [Run a Nano cube approach with DeepSeek or Codex](validation/nano-rgb-approach.md) · [Watch the actual camera recording](validation/nano-rgb-approach.mp4)

## Run from a Studio source checkout

Use an Apple Silicon Mac with Metal, a working Studio Rust toolchain, Python 3.13, uv and FFmpeg (including ffprobe and the development libraries for Studio video playback). The tested AISLE source revision is `dbd34a71b6f82a011541b433306ca7e6a705e823`. Set `DOROBOT_AISLE_ROOT` to a checkout of that revision. To obtain a separate checkout from GitHub, run the following from the Studio repository root.

```sh
git clone https://github.com/moxin-org/AISLE.git ../aisle-workbench-runtime
git -C ../aisle-workbench-runtime checkout --detach dbd34a71b6f82a011541b433306ca7e6a705e823
export DOROBOT_AISLE_ROOT="$(cd ../aisle-workbench-runtime && pwd)"
uv venv --python 3.13 .venv-aisle
uv pip install --python .venv-aisle/bin/python numpy==2.5.5 pillow==13.0.0 dimforge-nexus3d==0.2.1 pyarrow==26.0.1 pyyaml==6.0.3 opencv-python-headless==5.1.0.95 scipy==1.18.1
python3 tools/fetch_robot_meshes.py g1
cargo run --locked -p dorobot-flex --bin dorobot-flex -- --screen aisle-task
```

An existing checkout at `../AISLE` and a Studio-local `.venv-aisle/bin/python` are discovered automatically. Override the interpreter with `DOROBOT_AISLE_PYTHON`. **DeepSeek via Octos** uses the Octos CLI and a DeepSeek credential configured in Octos. Set `DOROBOT_OCTOS_PROFILE` to your stored DeepSeek profile if required. The default model is `deepseek-v4-flash`; `DOROBOT_OCTOS_MODEL` selects another DeepSeek model. Studio requests a one-turn read-only Octos session and checks the returned model identity and controller fields. **Ask Codex** uses an authenticated Codex CLI on PATH. **Use reference task** works locally without an inference service. The source workbench uses external Python and a GPU. Desktop release bundles require separate integration.

### Use DeepSeek as the only author

Install Octos on the Studio machine and configure a DeepSeek profile. On this Mac the profile was named `dsflash`. With an existing profile, the following setup reads the credential interactively into Keychain. [Octos project](https://github.com/octos-org/octos)

```sh
export DOROBOT_OCTOS_PROFILE=dsflash
octos auth set-key --profile "$DOROBOT_OCTOS_PROFILE" DEEPSEEK_API_KEY
octos --version
```

Select **DeepSeek via Octos** for a new task and **Revise with DeepSeek** for a selected measured run. Enable **Onboard RGB** for location-dependent Nano instructions. The Octos CLI runs locally and inference uses the DeepSeek service. [Verified operation on both robots with Codex absent from PATH](validation/provider-comparison.md)

### Work through the four pages

1. **Task** — select a robot. Use the reference task or enter “Inspect the right shoulder with a displacement of minus 0.08 rad” and choose **DeepSeek via Octos** or **Ask Codex**. Read the joint, signed displacement, provider and revision. Select an earlier controller to repeat it. After a scored run, select it on Evidence, update the task text and choose **Revise with DeepSeek** or **Revise with Codex**. Either author can revise the selected trial, including a trial prepared by the other provider or the local reference controller. The agent receives the measured outcome and signed displacement. The new controller records the previous controller hash and measurements.
2. **Scene** — inspect the mesh inventory, movable joint count and limits. Standard and Compliant select reproducible position gains of 28 and 18 respectively. These two inspection fixtures anchor the root and disable gravity. The Nano living-room and G1 standing tasks select their own floating-base scenes with gravity.
3. **Run** — start the prepared controller. The operator camera and position feedback come from the active simulation. Stop cancels the job and retains a receipt. Each run has its own directory.
4. **Evidence** — read the requested and measured displacement, guard verdict and trace. Navigate older runs or open the run folder. A prior run with the same robot, URDF hash, fixture, joint and displacement supplies a comparison. Choose **Export episode** to encode the 25 synchronized RGB frames and all measured joints and commands as a LeRobot episode. **Open in Replay** opens it in the existing Studio player. After restarting Studio, saved controllers and completed or stopped trials appear again.

The inspection lasts 1.21 seconds of simulated time. It tests the selected shoulder against its URDF bounds at all 121 physics steps and saves 25 sampled angles. Success requires an absolute endpoint error below the smaller of 0.025 rad and 15% of the requested movement. The fixed-base inspection uses an external operator camera. The Nano mobile controller uses only its calibrated onboard cameras and encoders. Hardware qualification has its own gate in the PRD.

Files live under the Studio user data directory in `aisle/`. On macOS the default is `~/Library/Application Support/DoRobot Studio/aisle/`. Each completed run retains the controller, model metadata, adapter and model hashes, operator RGB sequence, sampled encoder and command vectors, trajectory and verdict. The separate export job’s `dataset/` contains MP4, Parquet, feature statistics and `meta/aisle.json` with the trial ID, controller and model hashes. The inspection records its operator camera at 20 FPS. Nano table transfer records head and wrist cameras at 5 FPS. Set `DOROBOT_STUDIO_STATE_DIR` to isolate an experiment.

## Reproduce the native checks

Build the remote input client from the Makepad revision pinned in Cargo.lock, then build Studio. From the Studio root, with the runtime configured as above, run these commands. Use a fresh output directory for each invocation.

```sh
cargo install --locked --git https://github.com/OctoSense-org/makepad.git --rev 44d8220f12acfa46157019267e119e0866dd6b05 makepad-remote
cargo build --locked -p dorobot-flex --bin dorobot-flex
.venv-aisle/bin/python tools/aisle_workbench/verify.py --studio target/debug/dorobot-flex --backend headless --dark --size 1024x768 --codex --out /tmp/aisle-headless-proof
.venv-aisle/bin/python tools/aisle_workbench/verify.py --studio target/debug/dorobot-flex --backend offscreen --size 1440x900 --codex --out /tmp/aisle-offscreen-proof
```

Use `--remote /path/to/makepad-remote` for a locally built client. Omit `--codex` to use reference controllers throughout. The driver sends pointer and keyboard events to the running application, restarts it with the same state directory and reads the saved trial files. The offscreen run also captures pixels. Both modes still use real Nexus physics and sensor rendering.

Use `--octos-deepseek` in place of `--codex` in the native verification command to run the Nano approach and both G1 authoring turns through Octos. See the [measured provider comparison](validation/provider-comparison.md).
