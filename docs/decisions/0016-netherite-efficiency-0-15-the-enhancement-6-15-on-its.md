# 0016 — Netherite Efficiency 0..=15

**Status:** accepted
**Amended:** once (phase 10)
**Tags:** pickaxe
**Supersedes:** —
**Superseded by:** —

## Decision

Netherite Efficiency 0..=15, the enhancement `6..=15` on its **own reset of the curve**

## Why

15 is Pika's instamine point; the top tier keeps climbing past 5. See
[MECHANICS.md](../MECHANICS.md#pickaxe-progression).

## Amendments

### phase 10 — the enhancement restarts the curve, and rides a slope of its own

Replaced: the enhancement continued the Ancient-Debris climb at steps 5→14, on the
shared `1.45` slope.

It now restarts the curve at zero (steps 0→9 in Obsidian). Without the reset the
fifteenth level was an Obsidian wall ~6x its neighbours — a reference speedrun spent
~37 h of a 39 h run on it — and it was the single longest grind in the game. The dip in
price at Eff 5→6 is the same kind a tier jump already makes.

It also rides its **own gentler slope** (`1.10`, not the shared `1.45`) since the pacing
pass — see [0029](0029-each-upgrade-track-carries-its-own-base-and-growth.md) — which is
what makes it a bounded completionist option rather than the run's dominant cost.
