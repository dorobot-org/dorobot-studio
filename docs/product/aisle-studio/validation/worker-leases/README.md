# AISLE worker leases

A Studio worker accepts prepared Nano and G1 simulation controllers and reports their physical verdicts. Its SQLite journal assigns a durable job ID and rejects a request that reuses that ID for different inputs. Each process group belongs to its own simulation. A submitted job keeps running when the client disconnects. The worker enforces a configured slot count and one live stream per worker. Background transfers keep the onboard observations needed for control and omit the synchronized camera dataset.

## Connect a worker

On the target machine install Studio, the AISLE runtime, Nexus and the URDF assets for the robots it will simulate. Initialize a spool on the target's local filesystem with the desired slot count.

```sh
.venv-aisle/bin/python tools/aisle_workbench/fleet_worker.py \
  --root /absolute/path/to/worker-state init --slots 2
```

Create a local node registry. All paths in a node entry refer to that worker. Omit `ssh` for local execution. For a remote node use the SSH alias already configured for its operator account. `ssh_config` optionally selects a particular OpenSSH configuration file.

```json
{
  "workstation": {
    "ssh": "studio-worker",
    "python": "/opt/studio/.venv-aisle/bin/python",
    "checkout": "/opt/studio",
    "aisle_root": "/opt/AISLE",
    "spool": "/home/operator/aisle-worker"
  }
}
```

```sh
worker() {
  .venv-aisle/bin/python tools/aisle_workbench/fleet_client.py \
    --nodes /absolute/path/to/nodes.json --node workstation "$@"
}
worker hello
worker submit /path/to/controller.json --mode stream --seed 0
worker submit /path/to/another/controller.json --mode observations --seed 1
worker status "$JOB_ID"
worker frame "$JOB_ID" /tmp/nano.rgb
worker cancel "$BACKGROUND_ID"
worker fetch "$JOB_ID" /tmp/completed-trial.zip
```

Use the UUID returned by submit as `JOB_ID` or `BACKGROUND_ID`. A client that needs to retry after a network interruption can generate its UUID before submission and pass `--id` with the same request on every attempt. Use `--room /path/to/room.json` to send an accepted scene revision with its image and survey. Controller companion files are bound to their original identities. The worker reports the actual Nexus backend, package version, available slots and installed URDF hashes. An expired heartbeat produces a lost-job receipt. OS file locks also protect physical slots while the owned process still exists. Downloading a closed trial verifies the full archive digest before publishing the local file.

## Measured behavior

On one Mac, a foreground red-cube transfer and a background three-cube controller advanced simultaneously in Nexus. A second stream and a third slot were rejected. Repeating the first request returned its original job. Cancelling the background job at its recorded 200-step progress boundary left the foreground trial running. It placed red in 6,848 ticks with continuous wheel support. The foreground evidence archive downloaded and passed its checksum.

A second worker spool on the same Mac used real OpenSSH over loopback. Independent client processes rediscovered the worker, submitted the controller together with a reviewed photographic room, read 29 live frames and retrieved its final verdict. The Nano completed all 6,848 physics ticks with full wheel support and the requested room hash. These measurements cover local process concurrency and SSH reconnect. Physical multi-host throughput remains open. [Job and frame receipts](measurement.json)

## Run jobs from Studio

Prepare a controller on Task, then open **Run → Workers**. Leave the connection field empty for the two-slot local worker, or enter an alias from the JSON file named by `DOROBOT_AISLE_WORKERS`. **Connect** discovers the backend, model identities and occupied slots. **Stream run** submits the selected controller, room and fixture with live RGB. **Background run** starts a second simulation with occasional progress updates. The Studio workspace admits one foreground job across its configured nodes.

Use **Previous job** and **Next job** to inspect each state and physics step. **Cancel job** stops the selected lease. **Fetch evidence** imports the worker archive into the native Evidence history and verifies its checksum and controller identity. Cancelled trials retain their stopped receipt. Completed recordings can be exported and opened in Replay. The workspace journal retains the worker and job identities across restarts and reuses the original request if a submission response was lost.

The headless 1024×768 and offscreen 1440×900 journeys each exercised two concurrent jobs on this Mac, rejected a duplicate foreground stream, imported the cancelled background job, placed the red cube in 6,848 continuously supported steps, and opened all 342 recorded frames in Replay before and after restart. [Native journeys and provider metrics](native.json) · [Worker view](foreground.png) · [Concurrent jobs](overlap.png) · [Imported evidence](imported-evidence.png) · [Replay](imported-replay.png)

A separate sample of the native headless view covered 115.1 wall seconds under concurrent qualification load. It observed 264 distinct frames, 2.29 visible updates per wall second and 45.9 physics steps per wall second. DeepSeek authoring took 2.03 seconds before execution. These measurements describe one foreground job and this system load. They do not establish machine capacity or a 25–30 FPS streaming rate. [Timestamped samples](cadence.json)
