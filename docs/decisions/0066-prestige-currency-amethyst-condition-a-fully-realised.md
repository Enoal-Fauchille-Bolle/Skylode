# 0066 — Prestige currency: Amethyst

**Status:** accepted
**Amended:** twice
**Tags:** prestige
**Supersedes:** —
**Superseded by:** —

## Decision

Prestige currency: Amethyst; condition: **a fully realised run** (level cap, Netherite
pickaxe) plus enough Amethyst to pay

## Why

The condition is a *fully developed* run — mining level at its cap and a Netherite
pickaxe — checked before the price, since Amethyst only drops past the level gate.
Paired with [0036](0036-the-end-s-ore-gates-behind-netherite-the-nether-s.md), which
makes Amethyst need Netherite, this puts the two-axis progression back on the prestige
itself. `Player::prestige_lock` reports which gates are still shut, the shape `MineLock`
takes. Amethyst stays dual-use (enchants or prestige).

## Amendments

### the condition became a fully developed run

Replaced: "reach the End (level 30) and accumulate Amethyst".

That left the shortest path to prestige an XP race that never climbed a tier: the End's
Amethyst mine was Wooden-gated, so a level-30 player prestiged with the pickaxe they
started on, and the balance harness measured that floor at ~2.6 h.

### phase 10 — Efficiency 15 dropped as a gate

Replaced: "a Netherite pickaxe *with Efficiency maxed*".

That third condition was redundant with the Amethyst price (which already forces
reaching and working the End) while being the sole source of the mono-mine Obsidian
grind phase 10 exists to flatten — a reference speedrun spent ~37 h of a 39 h run on
Efficiency 6→15. Dropping it makes that enhancement pure optimisation and lets the two
reference players diverge: the speedrunner skips it and farms Amethyst, the
completionist maxes it. The lock is now two gates, not three.
