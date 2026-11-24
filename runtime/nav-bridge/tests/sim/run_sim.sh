#!/usr/bin/env bash
# 2D-simulation test of the dviz command channel (no robot). Renders nav_sim_cmd.yml.in and starts it detached as
# dataflow `nav-sim-cmd` (nav_command on UDP 19998, bridge on tcp 7451, robot id tracer_sim). Writes $WORK/cmd_token.
#   env: DORA_NAV  BRIDGE  WORK  RENDER_ONLY=1 (render and check only, no dora call)
#   then: bash check_cmds.sh        stop: dora stop --name nav-sim-cmd
set -eu
HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
export DORA_NAV=${DORA_NAV:-$HOME/nvme/DORA_NAV}
export BRIDGE=${BRIDGE:-$HOME/nvme/dora-lab/tmp/nav-bridge-target/release/dorobot-nav-bridge}
WORK=${WORK:-$HOME/nvme/dora-lab/tmp/nav-sim-cmd}

for f in "$BRIDGE" "$DORA_NAV/build/modules/simulation/sim2d_dora/sim2d_dora_node" \
         "$DORA_NAV/modules/interface/nav_command/nav_command.py"; do
  [ -e "$f" ] || { echo "run_sim: missing $f" >&2; exit 1; }
done

mkdir -p "$WORK"
YML=$WORK/nav_sim_cmd.yml
CFG=$WORK/bridge_sim.json
for pair in "nav_sim_cmd.yml.in:$YML" "bridge_sim.json.in:$CFG"; do
  sed -e "s|@DORA_NAV@|$DORA_NAV|g" -e "s|@BRIDGE@|$BRIDGE|g" -e "s|@HERE@|$HERE|g" -e "s|@WORK@|$WORK|g" \
      "$HERE/${pair%%:*}" > "${pair##*:}"
  if grep -q '@[A-Z_]*@' "${pair##*:}"; then echo "run_sim: unreplaced tokens in ${pair##*:}" >&2; exit 1; fi
done
( umask 077; head -c 24 /dev/urandom | base64 > "$WORK/cmd_token" )
chmod 600 "$WORK/cmd_token"
echo "run_sim: rendered $YML $CFG, token in $WORK/cmd_token"
[ "${RENDER_ONLY:-0}" = 1 ] && exit 0

die() { echo "run_sim: $*" >&2; exit 1; }
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

cd "$WORK"   # dora writes the node logs to ./out/<dataflow_id>/ next to the yaml
dora start "$YML" --name nav-sim-cmd --detach
echo "run_sim: started nav-sim-cmd; stop it with: dora stop --name nav-sim-cmd"
