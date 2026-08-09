# Visual validation for the DoRobot UX

Renders each implemented screen from `ui_mock` (driven by `api::mock::MockBackend`,
whose fixtures are transcribed from `docs/ux/*.png`) and checks it three ways.

```bash
tools/vqa/run.sh                 # capture + score every implemented screen
tools/vqa/run.sh library         # one screen
python3 tools/vqa/score.py --accept   # promote current renders to golden
python3 tools/vqa/score.py --json     # machine-readable
```

Exit code is non-zero on any failure, so this can gate a PR.

## What gates

| Signal | Why it exists |
|---|---|
| **dsl** | Any `[E]` from the script VM means the screen did not render as authored. This caught every real bug so far: unmerged `+:` overrides, immutable shader `let`, one-arg `atan`. A screen can look plausible while logging 270 errors. |
| **tokens** | Semantic-colour coverage. An unstyled widget — the classic dev-system failure where `draw_bg:` replaced instead of merged — shows up as a token collapsing toward zero. |
| **golden** | Grid correlation + mean pixel delta against the last accepted render of the same screen. Renders are deterministic (identical input → `1.000/1.000`, mad `0.0000`), so any drift is a real change. |

## What does not gate

`design L/E` compares against the generated design renders in `docs/ux/`. Those
are **intent, not a pixel spec** — they contain photographic thumbnails and a
rendered robot arm that the app draws as placeholders until real previews and
`RobotView` are wired up. Expect ~0.2–0.4; it is reported for direction only.

## Colour matching note

The renderer colour-manages output: authored `#58BE8A` lands as `#73BC8E`
on screen (R +27). Tokens are therefore matched by hue/saturation, not RGB
distance. Classification is **value-first** — near-black chrome like `#12151C`
has high HSV saturation (small channel values, large relative spread), so a
saturation-first test misfiles the entire background as accent.

## Adding a screen

1. Implement it in `dorobot-flex/src/ui/`, mount it in `src/bin/ui_mock.rs`.
2. Add the name to `IMPLEMENTED` in `run.sh` and to `REQUIRED` in `score.py`.
3. `tools/vqa/run.sh <screen>`, fix until `dsl` is 0, then `--accept`.
