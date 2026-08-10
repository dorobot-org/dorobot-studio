#!/usr/bin/env bash
# Capture one DoRobot UX screen from the mockup app.
#
#   tools/vqa/shoot.sh library
#
# Launches ui_mock on that screen, sizes the window to the design render's
# 1536x1024, captures the window rect, and writes a design-vs-render sheet.
set -uo pipefail

SCREEN="${1:-library}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT="${VQA_OUT:-$ROOT/tools/vqa/out}"
DESIGN="$ROOT/docs/ux"
BIN="$ROOT/target/release/ui_mock"

case "$SCREEN" in
  library)  REF="$DESIGN/ux-01-library.png"  ;;
  hardware) REF="$DESIGN/ux-02-hardware.png" ;;
  record)   REF="$DESIGN/ux-03-record.png"   ;;
  play)     REF="$DESIGN/ux-04-player.png"   ;;
  eval)     REF="$DESIGN/ux-05-eval.png"     ;;
  *) echo "unknown screen: $SCREEN" >&2; exit 2 ;;
esac

mkdir -p "$OUT"
[ -x "$BIN" ] || { echo "build first: cargo build --release -p dorobot-flex --bin ui_mock" >&2; exit 1; }

SHOT="$OUT/app-$SCREEN.png"
SHEET="$OUT/compare-$SCREEN.png"
LOG="$OUT/$SCREEN.log"

# Make sure any previous instance is really gone before launching.
pkill -f "target/release/ui_mock" 2>/dev/null
for _ in $(seq 1 20); do
  pgrep -f "target/release/ui_mock" >/dev/null || break
  sleep 0.25
done

"$BIN" --screen "$SCREEN" >"$LOG" 2>&1 &
APP_PID=$!
cleanup() { kill "$APP_PID" 2>/dev/null; }
trap cleanup EXIT

# Wait for the window to exist rather than sleeping a fixed amount.
for _ in $(seq 1 40); do
  if osascript -e 'tell application "System Events" to exists (first process whose name contains "ui_mock")' 2>/dev/null | grep -q true; then
    break
  fi
  sleep 0.25
done
sleep 1.5

osascript >/dev/null 2>&1 <<'APPLESCRIPT'
tell application "System Events"
  set p to first process whose name contains "ui_mock"
  set frontmost of p to true
  set w to first window of p
  -- Sized to clear the Dock while keeping the full screen content. At the
  -- design 1536x1024 the frame runs to y=1092, and when
  -- the Dock is showing the usable area ends above that, so the window gets
  -- clamped a pixel shorter — which is what made captures land on 2046 in some
  -- sessions and 2048 in others.
  set position of w to {40, 40}
  set size of w to {1536, 990}
end tell
APPLESCRIPT
# Let hover/focus animators finish: the pointer sits wherever the user left it,
# so a mid-animation capture differs from the settled one.
sleep 3.0

# Capture the window itself, not the screen rectangle it occupies. A rect
# capture picks up whatever is in front of the app - a terminal overlapping the
# window silently lands in the golden, which has happened. Asking for the
# window id gets that window's own surface regardless of stacking, and the
# lookup runs per capture, after the window is known to exist, so it cannot
# race window creation.
window_id() {
  python3 "$ROOT/tools/vqa/window_id.py" 2>/dev/null
}

capture_once() {
  local wid
  wid=$(window_id)
  if [ -n "$wid" ]; then
    screencapture -x -o -l"$wid" "$SHOT"
  else
    echo "  no window id for ui_mock; falling back to rect capture" >&2
    screencapture -x -R40,40,1536,1024 "$SHOT"
  fi
}

# The window must come out the same size every session. Setting the AX size
# sets the frame including the title bar, and the resulting content height has
# been seen to land on 2044, 2046 or 2048 native. Four pixels is enough to
# rescale the whole canonical image and shift every row, which reads as a
# content change against a golden shot at another size.
native_h() { python3 -c "import sys;from PIL import Image;print(Image.open(sys.argv[1]).size[1])" "$1" 2>/dev/null; }
EXPECT_H=1980
# Retry rather than nudge. The window is still settling for a moment after it
# appears, and a capture taken then comes out 2046 instead of 2048 — two pixels
# is enough to rescale the canonical image and shift every row, which reads as
# a content change. Resizing to correct it only adds drift; waiting fixes it,
# and consecutive settled captures are byte-identical.
fit_height() {
  local have
  have=$(native_h "$SHOT"); [ -z "$have" ] && return 0
  [ "$have" = "$EXPECT_H" ] && return 0
  echo "  capture was ${have}px tall, expected ${EXPECT_H}px; window still settling" >&2
  sleep 1.0
  return 1
}

# A correct render of this dark UI is dark; anything bright means we grabbed
# the wrong surface (a race we have actually hit).
looks_right() {
  [ -f "$SHOT" ] || return 1
  python3 - "$SHOT" <<'PY'
import sys
from PIL import Image
im = Image.open(sys.argv[1]).convert("L").resize((64, 42))
sys.exit(0 if sum(im.tobytes()) / (64 * 42) < 110 else 1)
PY
}

for attempt in 1 2 3 4; do
  capture_once
  if ! looks_right; then
    echo "  capture attempt $attempt looked wrong, retrying" >&2
    sleep 1.5
    continue
  fi
  fit_height && break
  echo "  capture attempt $attempt was $(native_h "$SHOT")px tall, nudging to $EXPECT_H" >&2
done

kill "$APP_PID" 2>/dev/null

# Stack design over render for human review.
python3 - "$REF" "$SHOT" "$SHEET" <<'PY'
import os, subprocess, sys, tempfile
from PIL import Image

ref, shot, sheet = sys.argv[1:4]
W = 1100
tmp = tempfile.mkdtemp()
scaled = []
for i, src in enumerate((ref, shot)):
    dst = os.path.join(tmp, f"p{i}.png")
    subprocess.run(["sips", "--resampleWidth", str(W), src, "--out", dst],
                   check=True, capture_output=True)
    scaled.append(dst)
imgs = [Image.open(p) for p in scaled]
out = Image.new("RGB", (W, sum(i.height for i in imgs) + 12), (20, 22, 28))
y = 0
for im in imgs:
    out.paste(im, (0, y))
    y += im.height + 12
out.save(sheet)
PY

echo "shot:  $SHOT"
echo "sheet: $SHEET"
