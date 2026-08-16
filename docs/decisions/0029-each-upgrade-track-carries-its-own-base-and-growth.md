# 0029 — Each upgrade track carries its own base and growth

**Status:** accepted
**Amended:** once (phase 10)
**Tags:** economy
**Supersedes:** —
**Superseded by:** —

## Decision

**Each upgrade track carries its own base and growth**: size 1.55, richness 1.35, tier
jumps and Efficiency 1.45, the Netherite enhancement 1.10, enchants 1.25 on a base of
ten times the others

## Why

Amends [0028](0028-cost-curve-shape-geometric-growth-constants-tuned-at.md), which read
as one curve for the whole game. One slope cannot serve a 9-step track and a 15-step
one. The slope has to be chosen against *that track's* production: size multiplies
output by 22 across its nine steps (9 to 200 cells), so a slope under 1.45 makes the
track fund itself more easily the further it climbs — the sink leaks precisely where it
should bite. But that same slope on Efficiency's fifteen steps puts the last Netherite
level at 643 full Obsidian grids, over five hours of mining for one level.

The corollary is a rule worth stating on its own: **the base governs the early game and
the slope governs the late one**, since `base * growth^0` is the base whatever the
slope. Raising the slope to make the game harder leaves the opening untouched and
inflates only the end.

## Amendments

### phase 10 — the Netherite enhancement gets a slope of its own

Replaced: the enhancement `6..=15` shared the `1.45` slope with tier jumps and
Efficiency.

It now carries its own `1.10`. Even after the curve reset
([0016](0016-netherite-efficiency-0-15-the-enhancement-6-15-on-its.md)) it is a ten-step
track only the completionist buys, where Efficiency `1..=5` and the tier jumps are
≤5-step climbs every run makes; one slope could not price both without coupling them, so
cutting the completionist's enhancement grind would have cut the speedrunner's tier
jumps too. Its own slope is what let the phase-10 pacing pass pull the completionist's
ceiling from ~5.4 h to ~2.3 h without moving the ~1 h speedrun. It shares `COST_BASE`
with the ordinary curve — the two meet at step zero — and is gentler because it
compounds over twice the steps.
