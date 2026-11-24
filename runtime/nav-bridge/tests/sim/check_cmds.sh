#!/usr/bin/env bash
# Command-channel check against the running nav-sim-cmd dataflow (run_sim.sh first). Prints PASS/FAIL per step, exits 1
# on any FAIL. `tracer_ctl.py` stands in for the person on the robot (arm / disarm / release over UDP 19998).
set -u
DORA_NAV=${DORA_NAV:-$HOME/nvme/DORA_NAV}
BRIDGE_DIR=${BRIDGE_DIR:-$HOME/nvme/dora-lab/tmp/nav-bridge-target/release}
WORK=${WORK:-$HOME/nvme/dora-lab/tmp/nav-sim-cmd}
CMD=$BRIDGE_DIR/examples/dviz_cmd
TOKEN=$WORK/cmd_token
[ -x "$CMD" ] || { echo "check_cmds: missing $CMD" >&2; exit 2; }
[ -r "$TOKEN" ] || { echo "check_cmds: missing $TOKEN (run run_sim.sh)" >&2; exit 2; }
FAILS=0
dc() { "$CMD" --connect tcp/127.0.0.1:7451 --robot tracer_sim "$@"; }
ctl() { python3 "$DORA_NAV/tools/tracer_ctl.py" --port 19998 "$@"; }
result() { python3 -c 'import json,sys; print(json.loads(sys.stdin.read().strip().splitlines()[-1]).get("result",""))' 2>/dev/null; }
# expect NAME WANT REPLY: PASS when REPLY's "result" is WANT
expect() {
  local got; got=$(printf '%s' "$3" | result)
  if [ "$got" = "$2" ]; then echo "PASS $1 (result=$got)"; else echo "FAIL $1 (want $2, got '${got}': $3)"; FAILS=$((FAILS+1)); fi
}

# 0. wait until the bridge answers: a bare `arm` (no token) must be refused with not_allowed
up=0
for _ in $(seq 1 30); do
  r=$(dc --cmd arm 2>/dev/null) && [ "$(printf '%s' "$r" | result)" = not_allowed ] && { up=1; break; }
  sleep 1
done
if [ $up = 1 ]; then echo "PASS bridge answers"; else echo "FAIL bridge answers (30 s)"; FAILS=$((FAILS+1)); fi
sleep 2   # first gate state

# 1. goal while disarmed
expect "goal while disarmed -> not_armed" not_armed "$(dc --cmd goal --x 0 --y 1 --yaw 0 --id sim-g0 --token-file "$TOKEN" 2>&1)"

# 2. arm on the robot, goal to P, wait for SUCCEEDED
ctl arm >/dev/null 2>&1; sleep 1
R=$(dc --cmd goal --x 0 --y 1 --yaw 0 --id sim-g1 --token-file "$TOKEN" --wait-goal-state SUCCEEDED --wait-s 60 2>/dev/null); rc=$?
expect "goal P (0,1,0) accepted" ok "$R"
if [ $rc = 0 ]; then echo "PASS goal P SUCCEEDED"; else echo "FAIL goal P SUCCEEDED (dviz_cmd exit $rc)"; FAILS=$((FAILS+1)); fi

# 3. goal back to P5, then estop
expect "goal P5 (0,0,0) accepted" ok "$(dc --cmd goal --x 0 --y 0 --yaw 0 --id sim-g2 --token-file "$TOKEN" 2>&1)"
sleep 2
expect "estop with token -> ok" ok "$(dc --cmd estop --token-file "$TOKEN" 2>&1)"
# NOTE: dviz_cmd cannot read nav/status outside --wait-goal-state, so the status "estop" flag is not checked here;
# watch it in Studio (step 5) or with dviz_probe.

# 4. remote arm / release are refused
expect "arm -> not_allowed" not_allowed "$(dc --cmd arm --token-file "$TOKEN" 2>&1)"
expect "release -> not_allowed" not_allowed "$(dc --cmd release --token-file "$TOKEN" 2>&1)"

# 5. token handling
expect "goal without token -> bad_token" bad_token "$(dc --cmd goal --x 0 --y 1 --yaw 0 --id sim-g3 2>&1)"
expect "estop without token -> ok" ok "$(dc --cmd estop 2>&1)"

# cleanup: the person on the robot releases the estop and disarms
ctl release >/dev/null 2>&1; ctl disarm >/dev/null 2>&1

if [ $FAILS -gt 0 ]; then echo "check_cmds: $FAILS FAIL"; exit 1; fi
echo "check_cmds: all PASS"
