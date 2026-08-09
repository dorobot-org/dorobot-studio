#!/usr/bin/env python3
"""Score a rendered screen. Local, deterministic, no API key — safe for CI.

The design renders in `docs/ux/` are *intent*, not a pixel spec: they contain
photographic thumbnails and a rendered robot the app will never reproduce
exactly. So similarity-to-design is reported but never gates. What gates:

  dsl      Any `[E]` line from the script VM means the screen did not render
           as authored. Hard fail — this is the signal that caught every real
           bug so far (unmerged overrides, immutable shader `let`, bad atan).
  tokens   Design-token coverage in the render. A styled widget paints its
           token; the classic dev-system failure (a `draw_bg:` that replaced
           instead of merging) shows up as a token dropping to ~zero.
  golden   Correlation against the last accepted render of this same screen.
           This is the true regression gate: same renderer, same content, so
           any drift is a real change. `--accept` promotes current to golden.

Usage:
    score.py [screen ...] [--json] [--accept]
"""
from __future__ import annotations

import json
import os
import shutil
import sys

from PIL import Image, ImageFilter

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
OUT = os.environ.get("VQA_OUT", os.path.join(ROOT, "tools", "vqa", "out"))
GOLDEN = os.path.join(ROOT, "tools", "vqa", "golden")
DESIGN = os.path.join(ROOT, "docs", "ux")

REFS = {
    "library": "ux-01-library.png",
    "hardware": "ux-02-hardware.png",
    "record": "ux-03-record.png",
    "play": "ux-04-player.png",
    "eval": "ux-05-eval.png",
}

# The renderer colour-manages output: authored #58BE8A lands as #73BC8E on
# screen. So chromatic tokens are matched by hue/saturation, not RGB distance —
# robust to that shift, and still unambiguous (blue 217°, green 145°, amber 38°,
# red 358° are far apart). Neutrals are matched by lightness band instead,
# because #12151C / #1A1F29 / #222836 are perceptually one dark family.
HUES = {           # name: (hue_deg, tolerance_deg)
    "accent": (217, 16),
    "ok": (145, 20),
    "warn": (38, 16),
    "stop": (358, 16),
}
MIN_SAT = 0.28
DARK_V, INK_V = 0.30, 0.78

# Tokens each screen must paint, as a fraction of all pixels. Floors detect
# "this widget lost its styling", not "this pixel moved".
REQUIRED = {
    "library": {"dark": 0.50, "ink": 2e-3, "accent": 5e-4, "ok": 2e-4,
                "stop": 1e-4, "warn": 1e-5},
    "hardware": {"dark": 0.50, "ink": 1e-3, "accent": 5e-4, "ok": 1e-3,
                 "warn": 1e-5},
    "record": {"dark": 0.50, "ink": 1e-3, "stop": 2e-4},
    "play": {"dark": 0.50, "ink": 1e-3, "accent": 2e-4},
    "eval": {"dark": 0.50, "ink": 1e-3, "stop": 2e-4},
}

TITLEBAR_FRAC = 28.0 / 1024.0   # macOS window capture includes the title bar
GRID_W, GRID_H = 32, 22
GOLDEN_GATE = 0.990
GOLDEN_MAD_GATE = 0.0003   # mean abs channel delta vs golden


def load(path: str, crop_titlebar: bool) -> Image.Image:
    im = Image.open(path).convert("RGB")
    if crop_titlebar:
        im = im.crop((0, round(im.height * TITLEBAR_FRAC), im.width, im.height))
    return im


def cells(im: Image.Image, edges: bool) -> list[float]:
    g = im.convert("L")
    if edges:
        g = g.filter(ImageFilter.FIND_EDGES)
    small = g.resize((GRID_W, GRID_H), Image.BOX)
    return [b / 255.0 for b in small.tobytes()]


def pearson(a: list[float], b: list[float]) -> float:
    n = len(a)
    ma, mb = sum(a) / n, sum(b) / n
    va = sum((x - ma) ** 2 for x in a)
    vb = sum((y - mb) ** 2 for y in b)
    if va == 0 or vb == 0:
        return 0.0
    return sum((x - ma) * (y - mb) for x, y in zip(a, b)) / (va * vb) ** 0.5


def token_coverage(im: Image.Image) -> dict[str, float]:
    """Fraction of pixels belonging to each semantic colour family.

    Full resolution so 1px borders and 8px bars are not blurred away.
    """
    raw = im.tobytes()
    total = len(raw) // 3
    counts = {k: 0 for k in HUES}
    counts["dark"] = 0
    counts["ink"] = 0
    for i in range(0, len(raw), 3):
        r, g, b = raw[i] / 255.0, raw[i + 1] / 255.0, raw[i + 2] / 255.0
        mx, mn = max(r, g, b), min(r, g, b)
        v = mx
        sat = 0.0 if mx == 0 else (mx - mn) / mx
        # Value first: near-black chrome (#12151C..#2A3140) has *high* HSV
        # saturation because the channel spread is large relative to the max,
        # so a saturation-first test misfiles the whole background as accent.
        if v < DARK_V:
            counts["dark"] += 1
            continue
        if v > INK_V and sat < 0.18:
            counts["ink"] += 1
            continue
        if sat < MIN_SAT:
            continue
        d = mx - mn
        if mx == r:
            h = 60.0 * (((g - b) / d) % 6.0)
        elif mx == g:
            h = 60.0 * (((b - r) / d) + 2.0)
        else:
            h = 60.0 * (((r - g) / d) + 4.0)
        for name, (h0, tol) in HUES.items():
            if min(abs(h - h0), 360.0 - abs(h - h0)) <= tol:
                counts[name] += 1
                break
    return {k: v / total for k, v in counts.items()}


def mad(a: Image.Image, b: Image.Image) -> float:
    """Mean absolute channel difference, 0..1. Sensitive to colour-only
    changes that a grid correlation smooths over."""
    size = (384, 256)
    ra = a.resize(size, Image.BOX).tobytes()
    rb = b.resize(size, Image.BOX).tobytes()
    return sum(abs(x - y) for x, y in zip(ra, rb)) / (len(ra) * 255.0)


def dsl_errors(screen: str) -> int:
    log = os.path.join(OUT, f"{screen}.log")
    if not os.path.exists(log):
        return -1
    with open(log, errors="ignore") as f:
        return sum(1 for line in f if line.startswith("[E]"))


def score(screen: str, accept: bool) -> dict:
    app_path = os.path.join(OUT, f"app-{screen}.png")
    if not os.path.exists(app_path):
        return {"screen": screen, "error": f"no render at {app_path}", "verdict": "FAIL"}

    app = load(app_path, crop_titlebar=True)
    res: dict = {"screen": screen, "dsl_errors": dsl_errors(screen)}
    fails: list[str] = []

    if res["dsl_errors"] != 0:
        fails.append(f"{res['dsl_errors']} script-VM errors")

    # ---- token coverage ----------------------------------------------------
    cov = {n: round(v, 5) for n, v in token_coverage(app).items()}
    res["tokens"] = cov
    missing = [
        f"{n}<{floor:g}"
        for n, floor in REQUIRED.get(screen, {}).items()
        if cov.get(n, 0.0) < floor
    ]
    res["tokens_missing"] = missing
    if missing:
        fails.append("unstyled: " + ", ".join(missing))

    # ---- regression vs last accepted render --------------------------------
    gpath = os.path.join(GOLDEN, f"app-{screen}.png")
    if os.path.exists(gpath):
        gold = load(gpath, crop_titlebar=True)
        res["golden_layout"] = round(pearson(cells(gold, False), cells(app, False)), 4)
        res["golden_edges"] = round(pearson(cells(gold, True), cells(app, True)), 4)
        res["golden_mad"] = round(mad(gold, app), 5)
        if min(res["golden_layout"], res["golden_edges"]) < GOLDEN_GATE:
            fails.append(f"structure drift vs golden ({res['golden_layout']:.3f}/{res['golden_edges']:.3f})")
        if res["golden_mad"] > GOLDEN_MAD_GATE:
            fails.append(f"pixel drift vs golden (mad {res['golden_mad']:.5f})")
    else:
        res["golden_layout"] = None
        res["golden_edges"] = None
        res["golden_mad"] = None

    # ---- similarity to design intent (informational) -----------------------
    ref_path = os.path.join(DESIGN, REFS[screen])
    if os.path.exists(ref_path):
        ref = load(ref_path, crop_titlebar=False)
        res["design_layout"] = round(max(0.0, pearson(cells(ref, False), cells(app, False))), 3)
        res["design_edges"] = round(max(0.0, pearson(cells(ref, True), cells(app, True))), 3)

    res["verdict"] = "PASS" if not fails else "FAIL"
    res["fails"] = fails

    if accept:
        os.makedirs(GOLDEN, exist_ok=True)
        shutil.copy2(app_path, gpath)
        res["accepted"] = True

    return res


def main() -> int:
    accept = "--accept" in sys.argv
    as_json = "--json" in sys.argv
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    screens = args or [s for s in REFS if os.path.exists(os.path.join(OUT, f"app-{s}.png"))]
    results = [score(s, accept) for s in screens]

    if as_json:
        print(json.dumps(results, indent=2))
        return 0 if all(r["verdict"] == "PASS" for r in results) else 1

    print(f"{'screen':<10} {'dsl':>4} {'golden L/E + mad':>22} {'design L/E':>12}  verdict")
    print("-" * 68)
    for r in results:
        if "error" in r:
            print(f"{r['screen']:<10} {r['error']}")
            continue
        g = ("  —" if r["golden_layout"] is None
             else f"{r['golden_layout']:.3f}/{r['golden_edges']:.3f} m{r['golden_mad']:.4f}")
        d = f"{r.get('design_layout', 0):.2f}/{r.get('design_edges', 0):.2f}"
        print(f"{r['screen']:<10} {r['dsl_errors']:>4} {g:>22} {d:>12}  {r['verdict']}")
        for f in r.get("fails", []):
            print(f"           ! {f}")
    if accept:
        print("\ngolden updated for:", ", ".join(r["screen"] for r in results))
    print("\ndesign L/E is informational — design renders contain photographic")
    print("content the app renders as placeholders. Gates are dsl + tokens + golden.")
    return 0 if all(r["verdict"] == "PASS" for r in results) else 1


if __name__ == "__main__":
    sys.exit(main())
