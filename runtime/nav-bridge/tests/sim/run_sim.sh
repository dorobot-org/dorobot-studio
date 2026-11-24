#!/usr/bin/env bash
# 2D-simulation test of the dviz command channel (no robot). Renders nav_sim_cmd.yml.in and starts it detached as
# dataflow `nav-sim-cmd` (nav_command on UDP 19998, bridge on tcp 7451, robot id tracer_sim).
# The bridge's token is TOKEN_FILE: default $WORK/cmd_token, made (mode 600) at start, after the "already running" check,
# only when it does not exist, so a running sim keeps its token. Or an existing token, e.g. ~/.config/dviz/cmd_token.
#   env: DORA_NAV  BRIDGE  WORK  TOKEN_FILE  RENDER_ONLY=1 (render and check only, no dora call)
#   then: bash check_cmds.sh (same WORK / TOKEN_FILE)        stop: dora stop --name nav-sim-cmd
set -eu
HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
export DORA_NAV=${DORA_NAV:-$HOME/nvme/DORA_NAV}
export BRIDGE=${BRIDGE:-$HOME/nvme/dora-lab/tmp/nav-bridge-target/release/dorobot-nav-bridge}
WORK=${WORK:-$HOME/nvme/dora-lab/tmp/nav-sim-cmd}
OWN_TOKEN=$WORK/cmd_token
TOKEN_FILE=${TOKEN_FILE:-$OWN_TOKEN}
case $TOKEN_FILE in /*) ;; *) TOKEN_FILE=$PWD/$TOKEN_FILE ;; esac
die() { echo "run_sim: $*" >&2; exit 1; }

for f in "$BRIDGE" "$DORA_NAV/build/modules/simulation/sim2d_dora/sim2d_dora_node" \
         "$DORA_NAV/modules/interface/nav_command/nav_command.py"; do
  [ -e "$f" ] || die "missing $f"
done
# A token given by the caller is used as it is, never made.
[ "$TOKEN_FILE" = "$OWN_TOKEN" ] || [ -r "$TOKEN_FILE" ] || die "TOKEN_FILE $TOKEN_FILE is missing or unreadable"

mkdir -p "$WORK"
YML=$WORK/nav_sim_cmd.yml
CFG=$WORK/bridge_sim.json
for pair in "nav_sim_cmd.yml.in:$YML" "bridge_sim.json.in:$CFG"; do
  sed -e "s|@DORA_NAV@|$DORA_NAV|g" -e "s|@BRIDGE@|$BRIDGE|g" -e "s|@HERE@|$HERE|g" -e "s|@WORK@|$WORK|g" \
      -e "s|@TOKEN_FILE@|$TOKEN_FILE|g" "$HERE/${pair%%:*}" > "${pair##*:}"
  if grep -q '@[A-Z_]*@' "${pair##*:}"; then echo "run_sim: unreplaced tokens in ${pair##*:}" >&2; exit 1; fi
done
echo "run_sim: rendered $YML $CFG"
if [ "${RENDER_ONLY:-0}" = 1 ]; then
  if [ -e "$TOKEN_FILE" ]; then echo "run_sim: token in $TOKEN_FILE"; else echo "run_sim: no token yet ($TOKEN_FILE is made at start)"; fi
  exit 0
fi

ENV_SH=${ENV_SH:-$HOME/dora-lab/env.sh}
[ -r "$ENV_SH" ] || die "cannot read $ENV_SH"
set +u   # env.sh is not -u clean
# shellcheck disable=SC1090
source "$ENV_SH" >/dev/null 2>&1 || { set -u; die "sourcing $ENV_SH failed"; }
set -u
export TMPDIR=${TMPDIR:-$HOME/nvme/dora-lab/tmp}
command -v dora >/dev/null 2>&1 || die "dora is not on PATH after sourcing $ENV_SH"

list_dataflows() { dora list 2>&1; }
if ! LIST=$(list_dataflows); then
  dora up >/dev/null 2>&1 || true
  LIST=$(list_dataflows) || die "dora list fails even after dora up: $LIST"
fi
printf '%s\n' "$LIST" | awk -v n=nav-sim-cmd '{for(i=1;i<=NF;i++) if($i==n) f=1} END{exit !f}' \
  && die "nav-sim-cmd is already running (dora stop --name nav-sim-cmd)"
# After that check, and never over an existing file: a running bridge read its token at its start.
if [ ! -e "$TOKEN_FILE" ]; then
  ( umask 077; head -c 24 /dev/urandom | base64 > "$TOKEN_FILE" )
  chmod 600 "$TOKEN_FILE"
  echo "run_sim: new token in $TOKEN_FILE"
else
  echo "run_sim: token in $TOKEN_FILE"
fi

cd "$WORK"   # dora writes the node logs to ./out/<dataflow_id>/ next to the yaml
dora start "$YML" --name nav-sim-cmd --detach
echo "run_sim: started nav-sim-cmd; stop it with: dora stop --name nav-sim-cmd"
