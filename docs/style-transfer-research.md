# Transferring the nexus look onto dorobot-flex: research

Status: research only, nothing implemented. Both experiments below were run and
then reverted; the tree is unchanged.

Written 2026-08-13 against `dorobot-studio` @ branch `lineage-split`,
`makepad-flex-layout` @ `78425b1`, makepad `dev` @ `0cd882f`.

## The question

Give `dorobot-flex` the visual language `nexus-studio` draws with, **without
changing any makepad widget**, and without disturbing the slideable/expandable
panel behaviour that `makepad-app-shell` provides.

## Short answer

There is a mechanism that works, it is already in use in this repo for one
element, and it is the same mechanism the Splash-Makepad project independently
documents as "the fork-free finding". The obstacle is not technique. It is that
`makepad-app-shell` hardcodes its palette inside shader bodies and composes its
own widget tree before any app can intervene, so the fix is a refactor of
app-shell rather than a stylesheet in the app.

## What works — proven

**Path-based override at the instantiation site, with the `+:` merge operator.**

`dorobot-flex/src/app.rs:62` already does this for the logo:

```rust
ShellLayout{
    main_container +: { header +: { logo_container +: { draw_bg +: { … } } } }
}
```

*Experiment:* the same path override was extended to the header's own surface —
`header +: { draw_bg +: { pixel: fn() { return #x141312 } } }`. The header
rendered nexus deep instead of app-shell's white, first build, no errors, no
widget modified. Reverted afterwards.

*Caveat found in the same experiment:* the title and icons stayed dark-on-dark
and became unreadable. Surfaces and foregrounds are independent children with
independently hardcoded colours, so every surface flipped needs its foregrounds
flipped in the same pass. Restyling this way is not one edit per panel; it is
one edit per drawn thing.

## What does not work — also proven

**Re-declaring the prototype in the app's own `script_mod!`.** This is the
obvious approach, and the registration order looks like it should work:
`makepad_app_shell::script_mod(vm)` runs at `dorobot-flex/src/app.rs:605`, and
the app's own modules at 608-613, after it.

*Experiment:* `mod.widgets.ShellHeader = set_type_default() do ShellHeaderBase{…}`
with a different `draw_bg`, placed in `dorobot-flex/src/shared/styles.rs`. It
compiled, ran without a single error, and **changed nothing at all**. Reverted.

*Why:* `makepad-flex-layout/src/shell/layout.rs:273` instantiates
`header := ShellHeader{}` **inside app-shell's own DSL tree**, which is
evaluated when app-shell registers. That instance captured the prototype as it
stood then; redefining the name afterwards cannot reach it.

Worth writing down because the failure is silent — no error, no warning, just no
effect — and it is a day lost to anyone who tries it.

## Why this is app-shell's problem

`makepad-app-shell` writes its colours as literals **inside shader bodies**:

```rust
draw_bg +: {
    dark_mode: instance(0.0)
    pixel: fn() {
        let light = vec4(1.0, 1.0, 1.0, 1.0)
        let dark  = vec4(0.059, 0.090, 0.165, 1.0)
        return mix(light, dark, self.dark_mode)
    }
}
```

Counted across `shell/`, `panel/` and `grid/`: **76 `draw_bg` blocks**, with the
literals concentrated in `layout.rs` (29), `panel.rs` (26), `header.rs` (19),
`sidebar.rs` and `sidebar_menu.rs` (12 each), `footer_grid.rs` (10),
`footer.rs` (6), `panel_grid.rs` (3). Not all are visible chrome — the realistic
target is 15-25 sites.

**The abstraction is already half-built.** `src/theme/colors.rs` defines a full
semantic palette — `BG_APP`, `BG_HEADER`, `BG_SIDEBAR`, `TEXT_PRIMARY`,
`ACCENT`, `BORDER` — and is *almost entirely unreferenced*; only `panel_colors`
is used anywhere in the crate. Someone started this and stopped. Finishing it is
most of the work.

## What is already done

The accent transfer already landed: **Load Dataset** and **Play** render nexus
orange in the current build, through `dorobot-ux` tokens applied to flex's *own*
widgets (`script_apply_eval!` in `playback_controls.rs`, `episode_info_panel.rs`
and others). What that work could not reach is exactly the app-shell chrome, and
the sections above are why.

## Splash-Makepad

`https://github.com/ymote/Splash-Makepad` — actively maintained (commits through
2026-08-10). Its `crates/splash-widgets/src/lib.rs:1-25` documents the same
mechanism, reached independently and verified on-device:

> * A **runtime** `script_eval!` override of `mod.prelude.widgets.*` **drops the
>   shader** — the widget renders blank. ❌
> * A **compiled** `script_mod!` that extends the base widget … and then
>   **references** the result into the prelude **keeps the shader**. ✅
>
> The difference: the `script_mod!` macro compiles the MPSL at build time; a
> runtime string is never compiled.

Their `widgets_mod()` does three things **in order**: register upstream's
widgets, define the themed variants, then **re-point the prelude** at them —
all before anything composes a tree. That third step is what the failed
experiment above was missing, and it is the shape app-shell needs.

### It is a pattern, not a dependency

`splash-widgets` path-depends on a sibling checkout
(`path = "../../../makepad-splash/widgets"`), as does `splash-render`
(`../../../makepad-splash/platform/script`). Its own manifest says it is kept
out of the default build because it "needs the full makepad build + a
makepad-script rev that matches splash-render's; wire it into your app's
makepad workspace to use it."

So it cannot be added as a git dependency. Either adopt the pattern, or vendor
the crate — the second is a road this project has already travelled:
`crates/makepad-plot` was vendored from this same repo and is **still current**,
identical in shape to upstream's (same 13 chart modules, same v0.2.0).

### The revision constraint

| | makepad rev | date |
|---|---|---|
| this workspace | `0cd882f` | **2026-08-10** — commit message: *"splash optimisation work"* |
| Splash-Makepad pins | `e1c2164b` | **2026-07-28** |

Splash-Makepad is pinned **13 days behind this workspace, on the very subsystem
in question**. Adopting its pin would regress makepad across `nexus-app`,
`nexus-studio`, `dorobot-ux`, `dorobot-flex` and `makepad-urdf-player`.

**Not verified:** whether Splash-Makepad builds against `0cd882f`. It
path-depends on a `makepad-splash` checkout not present here, so this is an open
question, not a known failure. GitHub reports 1,836 commits between the two
revs, but `dev` appears rebase-heavy; trust the dates over that number.

## What the refactor is, concretely

1. **Split widget definition from layout composition** in `makepad-flex-layout`.
   Today `layout.rs:273` composes `header := ShellHeader{}` inside app-shell's
   own DSL, which is what makes it unthemeable from outside. It needs a
   `widgets_mod()`-shaped hook the app calls *between* "register base widgets"
   and "compose the tree".
2. **Move the shader literals out** of inline `pixel: fn()` bodies into
   overridable variants.
3. **Finish `theme/colors.rs`** — wire the semantic palette that already exists
   into those variants, so a theme is one token block rather than 20 path
   overrides.

Then `dorobot-ux`'s `nx.*` tokens become the theme flex passes in, and the two
token systems stop competing. They already agree on mechanism — both drive a
shader uniform, app-shell's `dark_mode: instance(0.0)` against dorobot-ux's
`light: instance(0.0)` — and disagree only on name and direction.

## Not at risk

Slide, expand, drag, dock and persist are `PanelGrid` drag-and-drop,
`LayoutState` and `persistence.rs` — event and layout logic with no `draw_bg`
involvement. Styling cannot break them, and they were intact in every
experimental build.

## Open questions

- **Is `mofa-org/makepad-flex-layout` ours to change?** The whole recommendation
  turns on this. It is pinned at `78425b1`, so the timing is ours either way,
  but the refactor is upstream work.
- **Does Splash-Makepad build on `0cd882f`?** Needs a `makepad-splash` checkout
  to answer.
- If the answer to the first is no, the fallback is path overrides at the
  instantiation site: verbose, 15-25 sites, needs no cooperation, and works
  today — proven above.

## Build note

`dorobot-flex` builds without the video feature —
`cargo build -p dorobot-flex --no-default-features` — which is how it was run
here, on a machine with no ffmpeg or pkg-config. Useful for anyone doing UI work
who does not want the media stack.
