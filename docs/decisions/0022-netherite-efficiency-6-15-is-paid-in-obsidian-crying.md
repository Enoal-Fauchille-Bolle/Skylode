# 0022 — Netherite Efficiency 6..=15 is paid in Obsidian + Crying Obsidian

**Status:** accepted
**Amended:** once
**Tags:** pickaxe
**Supersedes:** —
**Superseded by:** —

## Decision

Netherite Efficiency `6..=15` is *paid* in Obsidian + Crying Obsidian, folded into the
Efficiency climb

## Why

Resolves an earlier contradiction — the pickaxe list said the post-Netherite enhancement
was "progression-gated, not paid" while the worlds table said it "consumes both".
Settled toward *paid*: Efficiency `1..=5` costs Ancient Debris (the ordinary tier
upgrade), `6..=15` costs a two-material mix, Obsidian-heavy at the bottom and
Crying-heavy at the top. The enhancement is thus the upper half of the Efficiency climb,
not a separate track, and the Obsidian mine's richness dial has an *optimum* ratio to
farm toward. See [MECHANICS.md](../MECHANICS.md#pickaxe-progression).

## Amendments

### the ratio climbs with the step

Replaced: "a minority Crying share (provisional 3:1)".

A *fixed* share pins that optimum at dial 1.7 of 9 and leaves seven rungs of the mine's
own richness track worth nothing, since buying them can only overshoot the recipe. The
share now climbs with the step, 25 % to 91 % — the top being `value_weight` at its own
ceiling, so the dial's last rung is exactly what the last Efficiency level wants. The
optimum becomes a *moving* target the player re-aims all the way up, which is what a
dial is for.
