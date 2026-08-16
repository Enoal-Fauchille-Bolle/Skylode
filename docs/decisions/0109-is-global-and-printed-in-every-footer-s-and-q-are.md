# 0109 — ? is global and printed in every footer

**Status:** accepted
**Tags:** ui
**Supersedes:** —
**Superseded by:** —

## Decision

**`?` is global and printed in every footer; `s` and `q` are global and printed in
none** — hence a full-screen, context-sensitive Help screen

## Why

The footer budget is real: Upgrades' footer already runs ~70 columns and `?  help` costs
9. So the choice was never "shown everywhere or shown on one screen" but "an exception
or a rule", and a rule reading "the footer shows whatever fits" makes its contents
depend on string lengths. `?` everywhere is what puts the hidden globals one key away,
and makes Help their only discoverability surface: full screen because ~20 bindings do
not fit a centred modal, context-sensitive because the question that opens Help is
almost always about the screen already in view.
