#!/usr/bin/env bash
# install.sh <dest-dir> [<binary>]: put the bridge where DORA_NAV starts it from, outside this dev checkout.
#
# Copies <binary> (default: ${CARGO_TARGET_DIR:-<this dir>/target}/release/dorobot-nav-bridge) to
# <dest-dir>/dorobot-nav-bridge and config/tracer.json to <dest-dir>/tracer.json, each as a temporary file renamed
# over the old one: a bridge running from <dest-dir> keeps its file, the next start gets the new one. Then prints the
# lines for ~/.config/dora-nav/env. Refuses (exit 1, nothing changed) a <dest-dir> with whitespace, quotes or
# backslashes (Dora splits a node's args shell-style) and a binary whose `--help` does not run.
set -euo pipefail
HERE=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)

fail() { echo "install.sh: $*" >&2; exit 1; }

[ $# -ge 1 ] && [ $# -le 2 ] || fail "usage: install.sh <dest-dir> [<binary>]"
dest=$1
bin=${2:-${CARGO_TARGET_DIR:-$HERE/target}/release/dorobot-nav-bridge}
config=$HERE/config/tracer.json

case $dest in
  "") fail "empty <dest-dir>" ;;
  /*) ;;
  *) dest=$PWD/$dest ;;
esac
if [[ $dest =~ [[:space:]\"\'\\] ]]; then
  fail "unsupported character in <dest-dir> (no whitespace, quotes or backslashes): '$dest'"
fi
[ -f "$bin" ] && [ -x "$bin" ] || fail "no executable bridge at $bin (build it: cargo build --release --locked)"
[ -f "$config" ] || fail "no config at $config"
usage=$(timeout 5 "$bin" --help 2>&1) || fail "$bin --help failed: $usage"
case $usage in
  "usage: dorobot-nav-bridge "*) ;;
  *) fail "$bin --help does not print the bridge's usage: $usage" ;;
esac

mkdir -p "$dest"
put() { # put <source> <name> <mode>: copy to a temporary name in <dest>, then rename over <name>
  local tmp
  tmp=$(mktemp "$dest/.$2.XXXXXX")
  if ! { cp "$1" "$tmp" && chmod "$3" "$tmp" && mv -f "$tmp" "$dest/$2"; }; then
    rm -f "$tmp"
    fail "could not install $dest/$2"
  fi
}
put "$bin" dorobot-nav-bridge 755
put "$config" tracer.json 644

echo "installed $dest/dorobot-nav-bridge (from $bin) and $dest/tracer.json"
echo "Put these lines in ~/.config/dora-nav/env, then restart dora-nav (systemctl --user restart dora-nav):"
echo "NAV_VIZ_BRIDGE=$dest/dorobot-nav-bridge"
echo "NAV_VIZ_BRIDGE_CONFIG=$dest/tracer.json"
