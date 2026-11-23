#!/usr/bin/env bash
# Offline replay of the Tracer nav chain + Studio bridge on a bag (no robot). Renders nav_replay.yml.in and starts it
# detached as dataflow `nav-replay`. Refuses while the live `tracer-nav` dataflow runs (it holds UDP 9998 and the CPU).
#   env: DORA_NAV  NAV_TEST  BAG  BRIDGE  WORK  RENDER_ONLY=1 (render and check only, no dora call)
#   then: dviz_probe --connect tcp/127.0.0.1:7450 --seconds 60 --expect expect.json
#   stop: dora stop --name nav-replay
set -eu
HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
export DORA_NAV=${DORA_NAV:-$HOME/nvme/DORA_NAV}
export NAV_TEST=${NAV_TEST:-$HOME/dora-lab/nav_test}
export BAG=${BAG:-$HOME/nvme/tracer_slam/bags/map_20261121_211128}
export BRIDGE=${BRIDGE:-$(cd "$HERE/../.." && pwd)/target/release/dorobot-nav-bridge}
WORK=${WORK:-$HOME/nvme/dora-lab/tmp/nav-replay}

for f in "$BAG" "$BRIDGE" "$NAV_TEST/nodes/dn_bag_player.py" "$DORA_NAV/build/modules/odometry/super_lio_dora/super_lio_dora_node"; do
  [ -e "$f" ] || { echo "run_replay: missing $f" >&2; exit 1; }
done

mkdir -p "$WORK"
YML=$WORK/nav_replay.yml
sed -e "s|@DORA_NAV@|$DORA_NAV|g" -e "s|@NAV_TEST@|$NAV_TEST|g" -e "s|@BAG@|$BAG|g" \
    -e "s|@BRIDGE@|$BRIDGE|g" -e "s|@HERE@|$HERE|g" -e "s|@DATAFLOW@|$YML|g" \
    "$HERE/nav_replay.yml.in" > "$YML"
if grep -q '@[A-Z_]*@' "$YML"; then echo "run_replay: unreplaced tokens in $YML" >&2; exit 1; fi
echo "run_replay: rendered $YML"
[ "${RENDER_ONLY:-0}" = 1 ] && exit 0

die() { echo "run_replay: $*" >&2; exit 1; }
ENV_SH=${ENV_SH:-$HOME/dora-lab/env.sh}
[ -r "$ENV_SH" ] || die "cannot read $ENV_SH"
set +u   # env.sh is not -u clean
# shellcheck disable=SC1090
source "$ENV_SH" >/dev/null 2>&1 || { set -u; die "sourcing $ENV_SH failed"; }
set -u
export TMPDIR=${TMPDIR:-$HOME/nvme/dora-lab/tmp}
command -v dora >/dev/null 2>&1 || die "dora is not on PATH after sourcing $ENV_SH"
command -v ss >/dev/null 2>&1 || die "ss not found; cannot check the ports"

# Dataflow names from `dora list` (any field equal to the name). A failing list is never read as "nothing running".
list_dataflows() { dora list 2>&1; }
if ! LIST=$(list_dataflows); then
  dora up >/dev/null 2>&1 || true   # the coordinator may just not be up yet
  LIST=$(list_dataflows) || die "dora list fails even after dora up; cannot tell whether tracer-nav runs: $LIST"
fi
has_name() { printf '%s\n' "$LIST" | awk -v n="$1" '{for(i=1;i<=NF;i++) if($i==n) f=1} END{exit !f}'; }
has_name tracer-nav && die "the live dataflow tracer-nav is running; stop it first (apps/tracer/autostop.sh), then run this again"
has_name nav-replay && die "nav-replay is already running (dora stop --name nav-replay)"

bound() { ss "$1" 2>/dev/null | awk -v p=":$2" '{for(i=1;i<=NF;i++) if(substr($i,length($i)-length(p)+1)==p) f=1} END{exit !f}'; }
bound -lun 9998 && die "UDP port 9998 is already bound (navcmd of the live dataflow?); refusing"
bound -ltn 7450 && die "TCP port 7450 is already bound (another bridge?); refusing"

cd "$WORK"   # dora writes the node logs to ./out/<dataflow_id>/ next to the yaml (the bridge follows them there)
dora start "$YML" --name nav-replay --detach
echo "run_replay: started nav-replay; stop it with: dora stop --name nav-replay"
