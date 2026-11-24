# dorobot-nav-bridge

Dora node that republishes DORA_NAV state to Dorobot Studio over Zenoh using the
`dviz-protocol` wire format (topics `dviz/<robot_id>/<entity>`). It also forwards the
Dora logs of every node of the dataflow and the dataflow graph, and takes Studio's
navigation commands (see "Command channel").

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

## Installing it for the robot

DORA_NAV starts the bridge from the path in `NAV_VIZ_BRIDGE`. Point that at an installed
copy, not at this checkout's `target/release`: a rebuild there would change the file of
the running node.

```bash
./install.sh /home/nvidia/nvme/dora-lab/opt/nav-bridge                      # target/release/dorobot-nav-bridge
./install.sh /home/nvidia/nvme/dora-lab/opt/nav-bridge /path/to/release/dorobot-nav-bridge
```

`install.sh <dest-dir> [<binary>]` copies the binary (default
`${CARGO_TARGET_DIR:-target}/release/dorobot-nav-bridge`) to `<dest-dir>/dorobot-nav-bridge`
and `config/tracer.json` to `<dest-dir>/tracer.json`, each as a temporary file renamed over
the old one (a bridge running from there keeps its file; the next start gets the new
one), and prints the two lines for `~/.config/dora-nav/env`:

```
NAV_VIZ_BRIDGE=<dest-dir>/dorobot-nav-bridge
NAV_VIZ_BRIDGE_CONFIG=<dest-dir>/tracer.json
```

The new copy runs from the next start of `dora-nav`. `install.sh` refuses a `<dest-dir>`
with a character outside `[A-Za-z0-9_./+,:=%~-]` (the whitelist of DORA_NAV's
`viz_overlay.py`; Dora splits a node's `args` shell-style, so whitespace, quotes,
backslashes, `#`, `$`, `;` and non-ASCII characters are all out) and a binary whose
`--help` fails.

The tests need no Dora daemon and no network beyond loopback: `tests/dora_node.rs` runs
the real binary through dora-node-api's integration-testing mode (`DORA_TEST_WITH_INPUTS`,
inputs and Stop from a JSON file), and every test uses its own `tcp/127.0.0.1:<port>`
with multicast scouting off.

## Running it

```bash
dorobot-nav-bridge <config.json>
```

It must be started by Dora as a node of the dataflow. `--help` (alone) prints the usage
and exits 0 without Dora; anything else outside Dora exits 1 with "not started by Dora"
and the usage. Under Dora the bridge connects to Dora before it looks at its arguments
(see "Never failing the dataflow"). Dataflow entry, for example:

```yaml
  - id: viz_bridge
    path: /home/nvidia/nvme/dora-lab/opt/nav-bridge/dorobot-nav-bridge   # the installed copy (install.sh)
    args: /home/nvidia/nvme/dora-lab/opt/nav-bridge/tracer.json
    env:
      NAV_VIZ_DATAFLOW: /path/to/the/dataflow.yml
    inputs:
      pose: {source: pose_fuser/map_base, queue_size: 2}
      path: {source: astar_planner/path, queue_size: 2}
      costmap: {source: astar_planner/costmap, queue_size: 2}
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
- The bridge opens this session at start and holds it until it stops; the worker's
  publishers and the command queryable on `dviz/<robot_id>/cmd` share it, so the command
  channel keeps working if the worker thread dies.
- Drop-style congestion control: a slow or stalled Studio never blocks the bridge;
  messages are dropped instead. An entity nobody subscribes to is not encoded.

### Topics

| Topic `dviz/<robot_id>/...` | Type | When |
|---|---|---|
| `map/grid`, `map/cloud`, `robot/pose`, `robot/footprint`, `sensors/scan`, `sensors/cloud` | see `dviz_protocol::nav_entities` | rate caps from the config; the maps every 5 s / 10 s |
| `nav/plan` | `linestrips3d` | each new path at once; the last one again every 2 s (latched: the planner stops publishing when a goal is done, and a Studio that connects later still sees it); an error path clears it (an empty strip, also resent) |
| `costmap/global` | `grid2d`, kind `costmap` | the planner's `costmap` input (32-byte header, then `width * height` Nav2 cost bytes, row 0 = min y; ~39 KB): at most `rates_hz.costmap` (1 Hz), the newest one kept if the cap held it back, and the last one again every 5 s for a Studio that connects later. A malformed one is counted and skipped |
| `nav/goal` | `arrows3d` | on change and every 2 s; a goal without a yaw is a 0.5 m vertical vector |
| `nav/trail` | `linestrips3d` | `rates_hz.trail`; cleared by a new goal id, kept when goal_resume resends the same goal as `<id>-r<n>` |
| `nav/status` | `status` (`NavStatus`) | on change and `rates_hz.status`; `stale` lists the parts (`localization`, `gate`, `chassis`, `speed`) whose input has not arrived for 2 s (their fields keep the last value); the chassis part carries `chassis_control_mode` (1 CAN, 3 RC, -1 no feedback), `chassis_error_code` and `battery_v` from `tracer/state` |
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
| `inputs` | `pose`, `path`, `goal`, `goal_status`, `scan`, `cloud`, `trust`, `refine_state`, `gate_state`, `chassis`, `costmap` (wire format `costmap`) | Dora input id to wire format |
| `rates_hz` | pose 20, cloud 5, scan 10, trail 2, status 5, costmap 1 | publish caps |
| `cloud_max_points` | 5000 | `sensors/cloud` points |
| `map_cloud_max_points`, `map_cloud_voxel_m` | 50000, 0.10 | `map/cloud` downsampling |
| `trail_min_step_m`, `trail_max_points` | 0.05, 3000 | `nav/trail` |
| `colors` | RViz-like: plan `[0,180,255,255]`, trail `[0,220,0,255]`, goal `[255,170,0,255]`, footprint `[220,220,220,255]`, scan `[255,0,0,255]`, map cloud `[180,180,180,255]` | layer colours (RGBA) |
| `radii` | plan 0.025, trail 0.02, goal 0.04, footprint 0.01 | line and arrow radius in metres (`radius` in `linestrips3d` / `arrows3d`, for Rerun) |
| `robot_model` | `"tracer_mini"` | sent in `nav/status`; the body model Studio draws (`null`: none) |
| `log_forward_per_node_hz` | 5 | node log lines per node per second (0: none) |
| `cmd` | goals off, forward to `127.0.0.1:9998` | the command channel; see "Command channel" |

Upgrade order (costmap): a Studio from delivery ① does not know the `costmap/global` layer. It paints any costmap-kind grid as an opaque unknown-grey static quad at z = 0 over the Rerun map; the Displays toggle cannot clear static data and the bridge's 5 s resend brings it back (the 2D view is unaffected). Update Studio together with, or before, the bridge. Stop-gap when Studio cannot be updated: delete the `"costmap": "costmap"` entry from `inputs` in the installed `tracer.json` and restart the bridge (a Dora input id that is not in `inputs` is ignored, so the layer is not published).

A missing or invalid map does not stop the bridge: the layer is not published, the
problem is printed on stderr and sent as a WARN log entry naming the file.

### Never failing the dataflow

What the bridge guarantees, and what it cannot:

- **From Dora's init on, it never fails.** The first thing the node does is connect to
  Dora (`DoraNode::init_from_env_force`), then it prints
  `nav-bridge: started by Dora (dataflow <id>, node <id>)`. Any problem after that (wrong
  number of arguments, e.g. a config path with a space that Dora split in two; config
  missing or invalid; listen port busy; Zenoh error) is one line
  `nav-bridge: ERROR: <cause>; publishing nothing, Dora inputs are consumed and dropped until Stop`
  on stderr; the node then consumes and drops every Dora event until Stop or the end of
  the event stream, and exits 0.
- **Before that it can still fail the start of the whole dataflow.** Dora 1.1 fails the
  start of every node of a dataflow when one node stops before it subscribes, whatever its
  exit code, a spawn failure included (a missing or non-executable binary), and Dora
  rejects the whole dataflow if one of the bridge's inputs names an output that does not
  exist. The bridge cannot catch either. DORA_NAV's `apps/tracer/autostart.sh` and
  `viz_overlay.py` guard against them: the overlay checks every input source against the
  base dataflow, refuses paths Dora would split, and runs `<bridge> --help` before it adds
  the node; `autostart.sh` starts the dataflow with the bridge without `exec`, waits for it
  to be Running with every node and for the bridge's `nav-bridge: started by Dora` line in
  its Dora log (the sign that every node got through Dora's start), and otherwise stops it
  and starts the plain `tracer_nav.yml`. See DORA_NAV `apps/tracer/README.md`.

Check the bridge's Dora log (`out/<dataflow_id>/log_<bridge id>.jsonl`) when Studio sees
nothing.

### Stopping

On Dora's Stop (or the end of the event stream) the worker leaves its loop at once; then,
on a helper thread, the command queryable is undeclared (no new command comes in), the
command lanes stop (commands already queued are still answered) and the Zenoh session
closes. A Studio reader that is connected but stalled holds the undeclare and the close
for seconds (zenoh-link-tcp lingers up to 10 s on unsent data), so the bridge waits at
most 1 s for all of it and then ends the process with `_exit` after flushing stdout and
stderr; the process is gone within about 1 s of Stop. If the worker thread dies, the
bridge prints an ERROR line on stderr and keeps consuming Dora events (dropping them)
until Stop; the command channel keeps running.

## Command channel

Studio's tools (2D Pose Estimate, 2D Goal Pose, E-STOP, Disarm) send commands as Zenoh
queries on `dviz/<robot_id>/cmd`: a JSON request of at most 1024 bytes,
`{"cmd","id","token"?,"x"?,"y"?,"yaw"?}` (`id`: 1-64 characters of `[A-Za-z0-9_-]`). The
bridge checks each one, forwards the accepted ones over UDP to DORA_NAV's nav_command,
waits for its `{"ok":...}` and replies with `{"id","cmd","result","reason"}`
(`dviz_protocol::CmdAck`).

### Configuration (`cmd`)

```json
"cmd": { "goal_enabled": false, "token_file": "~/.config/dviz/cmd_token", "forward": "127.0.0.1:9998", "reply_timeout_ms": 1000, "max_goal_per_s": 2 }
```

| Field | Default | Meaning |
|---|---|---|
| `goal_enabled` | `false` | accept `goal` and `initialpose`; `estop` and `disarm` are always accepted |
| `token_file` | `"~/.config/dviz/cmd_token"` | the robot's token; a leading `~/` is `$HOME` |
| `forward` | `"127.0.0.1:9998"` | nav_command's UDP address. It must be `127.0.0.1:<port>` (nav_command listens on 127.0.0.1 only); anything else is one `nav-bridge: ERROR: cmd.forward ...` line at start and no command channel at all (no queryable, so every command, estop included, goes unanswered; `cmd_goal_enabled` false), while data is still published |
| `reply_timeout_ms` | 1000 | how long to wait for nav_command's answer |
| `max_goal_per_s` | 2 | `goal` + `initialpose` rate (one token bucket, burst 2) |

A `reply_timeout_ms` outside 100-2500 (nav_command answers on its 100 ms tick; Studio stops
waiting after 3 s) or a `max_goal_per_s` not above 0 (only the first 2 goals ever pass) is
one `nav-bridge: WARNING: ...` line on stderr at start; the values are used as they are.

### Token file

The robot and the Studio machine hold the same token in `~/.config/dviz/cmd_token`:

```bash
( umask 077; mkdir -p ~/.config/dviz; head -c 24 /dev/urandom | base64 > ~/.config/dviz/cmd_token ); chmod 600 ~/.config/dviz/cmd_token
# copy it to the same path on the Studio machine, mode 600
```

The subshell keeps the `umask` out of your shell; the `chmod` tightens a file that was
already there (a `umask` only applies to new files).

The bridge reads it once at start (a new token needs a bridge restart). Surrounding
whitespace is removed on both sides (a trailing newline or CRLF is fine). It is usable only
if it exists, is not empty and has no group or other permission bits at all
(`mode & 0o077 == 0`, e.g. 600 or 400; group-readable or even group-executable is too
open). The comparison takes constant time. The token never appears in a log line, a reply
or the datagram to nav_command.

### Turning goals and initial poses on

Put the token file in place, set `goal_enabled` to `true` in the existing `cmd` entry of
the installed `tracer.json` (it already has one: adding a second `"cmd"` key is a duplicate
key, so the config fails to load and the bridge runs disabled, publishing nothing and
answering no command), and restart dora-nav. `install.sh` overwrites the installed
`tracer.json` with the repository's (`goal_enabled: false`), so do this again after every
install. The bridge prints one line at start:

- `nav-bridge: goal/initialpose commands enabled`, or
- `nav-bridge: goal/initialpose commands closed: <reason>` (the switch is off; the token
  file is missing, empty or too open).

`nav/status` carries `cmd_goal_enabled`: the switch is on and the token is usable. Studio
greys out its pose and goal tools otherwise; E-STOP and Disarm work either way.

**Upgrade order:** install this bridge before the Studio that sends commands. A delivery-③
bridge has no command channel: the new Studio greys its pose and goal tools (`goal / pose
commands not enabled on the robot`) and E-STOP and Disarm get `no reply (robot not
reachable)`, so the robot is not stopped from Studio.

### Rules

| Command | Checks, in order | Forwarded |
|---|---|---|
| `estop`, `disarm` | none: no token, switch, rate limit or gate state; also with the worker thread dead | at once, from their own lane: within 50 ms of arrival, never queued behind a goal |
| `goal` | switch on and token usable, else `disabled`; token matches, else `bad_token`; rate, else `rate_limited`; gate state never received, older than 2 s or unreadable: `gate_unknown`; e-stop latched: `estop_active`; disarmed: `not_armed` | `{"cmd":"goal","x","y","yaw"?,"id"}`; the goal id is the request id |
| `initialpose` | as `goal`, without the gate state checks | `{"cmd":"initialpose","x","y","yaw"}` |
| `arm`, `release`, anything else | `not_allowed`: arming and releasing the e-stop stay with the people at the robot | never |
| a malformed request (bad JSON, bad `id`, over 1024 bytes, a goal without finite `x`, `y`) | `bad_request` | never |

A forwarded command's result is nav_command's answer: `{"ok":true}` is `ok` (reason
`sent to nav_command`), `{"ok":false}` (or anything else) is `nav_rejected`, nothing within
`reply_timeout_ms` is `no_reply`. The ack echoes the request's `id` and its `cmd` cut to 32
characters. The gate state is the latest Dora input of format `gate_state`
(`cmd_gate/state`), stored straight from the Dora event loop, so it is current even when the
worker is behind or dead. Each lane queues at most 16 commands; a query that finds its lane
full gets no reply (Studio shows `no reply (robot not reachable)`). Every command is one
stdout line (in the bridge's Dora log): `nav-bridge: cmd <cmd> <id> [x=.. y=.. yaw=..] ->
<result>` (`cmd` cut to 32 characters, the id in full, control characters as `?`).

### `dviz_cmd`

`examples/dviz_cmd.rs` sends one command the way Studio does and prints the reply JSON:

```text
dviz_cmd --connect tcp/<host>:<port> --robot <id> --cmd <goal|initialpose|estop|disarm|arm|release>
         [--x X --y Y --yaw RAD] [--id ID] [--token-file PATH] [--wait-goal-state STATE --wait-s N]
```

```bash
cargo build --locked --release --target-dir $HOME/nvme/dora-lab/tmp/nav-bridge-target --examples
CMD=$HOME/nvme/dora-lab/tmp/nav-bridge-target/release/examples/dviz_cmd
$CMD --connect tcp/127.0.0.1:7450 --robot tracer --cmd estop
$CMD --connect tcp/127.0.0.1:7450 --robot tracer --cmd goal --x 1.0 --y 0.0 --yaw 0 \
     --token-file ~/.config/dviz/cmd_token --wait-goal-state SUCCEEDED --wait-s 60
```

- `--token-file`: read, trimmed and sent; without it no token is sent (`bad_token` for a
  goal, while `estop` still works).
- `--id`: the request id, default `cli-<unix seconds>-<pid>`.
- `--wait-goal-state` (goal only): after an `ok`, follow `dviz/<robot>/nav/status` until
  `goal_id` is the request id and `goal_state` is STATE, for at most `--wait-s` seconds
  (default 60); state changes go to stderr.
- `estop` and `disarm` are sent with `Priority::RealTime` and `CongestionControl::Block`,
  like Studio; the query times out after 3 s.
- Exit 0: a reply came (any result), or with `--wait-goal-state` the state was reached.
  1: no reply; with `--wait-goal-state` also a result other than `ok` or the state not
  reached in time. 2: bad arguments, an unreadable token file, a Zenoh error.

### Known limitations

- **The token is plain text**, on disk (mode 600) and inside every goal and initial pose
  query on the wire (Zenoh TCP, no TLS). It prevents mistakes (a second Studio, the wrong
  robot); it does not stop someone who can capture the traffic.
- **Disarm-to-goal gap of about 0.1-0.2 s:** between the bridge's armed check and
  nav_command handing the goal to the navigation (nav_command works every 100 ms). A
  disarm that lands exactly in that window leaves the goal in place until it is given up
  after 15 s. Sending goals with `tracer_ctl` on the robot has the same gap; closing it
  needs a DORA_NAV change (e.g. `goal_resume` refusing goals while disarmed).
- **Acks of back-to-back estops queue:** the estop and disarm datagrams go out at once,
  one after another, but their replies are awaited one at a time, each for up to
  `reply_timeout_ms`; the second ack can come up to that much later. The sends do not
  wait.

## Offline replay and probe

`tests/replay/` replays the mapping bag through the nav chain plus this bridge without the
robot (`run_replay.sh`, template `nav_replay.yml.in`, one goal from `goal_once.py`; it
refuses to start while the live `tracer-nav` dataflow runs). `examples/dviz_probe.rs`
subscribes to `dviz/**`, prints a JSON summary and checks it against `tests/replay/expect.json`:

```bash
cargo run --release --locked --example dviz_probe -- --connect tcp/127.0.0.1:7450 --seconds 60 --expect tests/replay/expect.json
```
