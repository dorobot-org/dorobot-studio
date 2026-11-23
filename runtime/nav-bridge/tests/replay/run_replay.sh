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

# shellcheck disable=SC1090
source "$HOME/dora-lab/env.sh" >/dev/null 2>&1
export TMPDIR=${TMPDIR:-$HOME/nvme/dora-lab/tmp}
if dora list 2>/dev/null | grep -qw tracer-nav; then
  echo "run_replay: the live dataflow tracer-nav is running; stop it first (apps/tracer/autostop.sh), then run this again" >&2
  exit 1
fi
if dora list 2>/dev/null | grep -qw nav-replay; then
  echo "run_replay: nav-replay is already running (dora stop --name nav-replay)" >&2
  exit 1
fi
cd "$WORK"   # dora writes the node logs to ./out/<dataflow_id>/ next to the yaml (the bridge follows them there)
dora up >/dev/null 2>&1 || true
dora start "$YML" --name nav-replay --detach
echo "run_replay: started nav-replay; stop it with: dora stop --name nav-replay"
