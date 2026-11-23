# dorobot-nav-bridge

Dora node that republishes DORA_NAV state to Dorobot Studio over Zenoh using the
`dviz-protocol` wire format (topics `dviz/<robot_id>/<entity>`). It also forwards the
Dora logs of every node of the dataflow and the dataflow graph.

This directory is its own Cargo workspace (own `Cargo.lock`, Dora 1.1 / Zenoh 1.10.2). It is
not part of the Studio root workspace and is independent of `runtime/` (Dora 1.0.1).

## Build and test

```bash
cd runtime/nav-bridge
export TMPDIR=$HOME/nvme/dora-lab/tmp     # on the Orin
cargo build --release --locked            # target/release/dorobot-nav-bridge
cargo test --locked
cargo clippy --locked --all-targets
```

The tests need no Dora daemon and no network beyond loopback: `tests/dora_node.rs` runs
the real binary through dora-node-api's integration-testing mode (`DORA_TEST_WITH_INPUTS`,
inputs and Stop from a JSON file), and every test uses its own `tcp/127.0.0.1:<port>`
with multicast scouting off.

## Running it

```bash
dorobot-nav-bridge <config.json>
```

It must be started by Dora as a node of the dataflow (outside Dora it exits with
"not started by Dora"). Dataflow entry, for example:

```yaml
  - id: viz_bridge
    path: /path/to/target/release/dorobot-nav-bridge
    args: /path/to/runtime/nav-bridge/config/tracer.json
    env:
      NAV_VIZ_DATAFLOW: /path/to/the/dataflow.yml
    inputs:
      pose: {source: pose_fuser/map_base, queue_size: 2}
      path: {source: astar_planner/path, queue_size: 2}
      # ... one input per entry of the config's `inputs` table
```

Every input is copied into a bounded queue (64 entries, the oldest dropped when full)
and the Dora event loop does nothing else; a worker thread parses, rate-limits, encodes
and publishes. Inputs must be UInt8 arrays (bytes) or strings.

### Environment variables

| Variable | Meaning |
|---|---|
| `NAV_VIZ_DATAFLOW` | Path of the dataflow YAML. The graph (`node_definition` / `graph_update`) is built from it. Not set: no graph. Unreadable or invalid: no graph and a WARN log entry naming the file. |
| `NAV_VIZ_LOG_DIR` | Directory of Dora's `log_<node>.jsonl` files. Not set: `<current dir>/out/<dataflow_id>/`, where Dora 1.1 writes them (Dora runs nodes in the directory of the dataflow YAML; `dataflow_id` comes from dora-node-api). Set it when the dataflow YAML that Dora started is not in the directory whose `out/` holds the logs, e.g. a generated overlay. |

### Network

- Zenoh peer mode, listening on `listen` from the config (default `tcp/0.0.0.0:7450`),
  multicast scouting off, no outgoing connections. Studio connects to
  `tcp/<robot address>:7450` in client mode with prefix `dviz`.
- Drop-style congestion control: a slow or stalled Studio never blocks the bridge;
  messages are dropped instead. An entity nobody subscribes to is not encoded.

### Topics

| Topic `dviz/<robot_id>/...` | Type | When |
|---|---|---|
| `map/grid`, `map/cloud`, `robot/pose`, `robot/footprint`, `nav/plan`, `nav/trail`, `nav/goal`, `sensors/scan`, `sensors/cloud`, `nav/status` | see `dviz_protocol::nav_entities` | rate caps from the config |
| `logs` | `log` | node log lines, at most `log_forward_per_node_hz` per node (read once a second); the bridge's own warnings |
| `nodes` | `node_definition`, one per node | every 5 s |
| `graph` | `graph_update` | every 5 s, after the node definitions |

### Node logs

The bridge follows `log_*.jsonl` in the log directory (new lines only; a truncated or
replaced file is read again from its start; at most the last 64 KB per file per second;
lines over 16 KB are dropped). Each Dora log line becomes a `log` entry:

- the text contains "error" (any case), or Dora's level is `error`: ERROR;
- otherwise stream `stderr`, or Dora's level `warn`: WARN;
- otherwise INFO.

Text is cut to 2048 bytes. Lines read while nobody subscribes to `logs` are not sent
later. The bridge's own log lines are not forwarded; its warnings (a map or the
dataflow it cannot read) are sent as WARN entries directly, again to each new
subscriber.

In `graph_update` a node is `error` if it logged an ERROR in the last 10 s, `active` if
it logged anything in the last 10 s, else `idle`; `last_seen` is seconds since the
bridge started. The bridge's own node is always `active`.

### Configuration (`config/tracer.json`)

Every field has a default (the Tracer Mini values below).

| Field | Default | Meaning |
|---|---|---|
| `robot_id` | `"tracer"` | topic prefix `dviz/<robot_id>` |
| `listen` | `["tcp/0.0.0.0:7450"]` | Zenoh listen endpoints |
| `map_yaml` | none | occupancy map (`map_server` YAML + PGM, trinary) for `map/grid` |
| `map_pcd` | none | PCD map for `map/cloud` |
| `footprint` | Tracer Mini rectangle | polygon in `base_footprint` |
| `inputs` | `pose`, `path`, `goal`, `goal_status`, `scan`, `cloud`, `trust`, `refine_state`, `gate_state`, `chassis` | Dora input id to wire format |
| `rates_hz` | pose 20, cloud 5, scan 10, trail 2, status 5 | publish caps |
| `cloud_max_points` | 5000 | `sensors/cloud` points |
| `map_cloud_max_points`, `map_cloud_voxel_m` | 50000, 0.10 | `map/cloud` downsampling |
| `trail_min_step_m`, `trail_max_points` | 0.05, 3000 | `nav/trail` |
| `colors` | RViz-like | layer colours (RGBA) |
| `log_forward_per_node_hz` | 5 | node log lines per node per second (0: none) |

A missing or invalid map does not stop the bridge: the layer is not published, the
problem is printed on stderr and sent as a WARN log entry naming the file.

### Stopping

On Dora's Stop (or the end of the event stream) the worker leaves its loop at once and
closes Zenoh. A Studio reader that is connected but stalled holds that close for seconds
(zenoh-link-tcp lingers up to 10 s on unsent data), so the bridge waits at most 1 s for
it and then ends the process with `_exit` after flushing stdout and stderr; the process
is gone within about 1 s of Stop. If the worker thread dies, the bridge prints an ERROR
line on stderr and keeps consuming Dora events (dropping them) until Stop.
