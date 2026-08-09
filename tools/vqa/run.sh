#!/usr/bin/env bash
# Capture every implemented screen and score it against the design renders.
#
#   tools/vqa/run.sh              # all implemented screens
#   tools/vqa/run.sh library      # one screen
#
# Exits non-zero if any screen fails its gate, so this can guard a PR.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

# Screens with a makepad implementation. Add here as each one lands.
IMPLEMENTED=(library hardware record play eval)
if [ $# -gt 0 ]; then SCREENS=("$@"); else SCREENS=("${IMPLEMENTED[@]}"); fi

echo "building ui_mock…"
cargo build --release -p dorobot-flex --bin ui_mock 2>&1 | grep -E "^error" && exit 1

for s in "${SCREENS[@]}"; do
  echo "capturing ${s}…"
  "$ROOT/tools/vqa/shoot.sh" "$s" >/dev/null 2>&1 || echo "  capture failed for $s"
done

echo
python3 "$ROOT/tools/vqa/score.py" "${SCREENS[@]}"
