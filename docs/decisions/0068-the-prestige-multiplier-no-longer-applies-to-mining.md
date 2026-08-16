# 0068 — The prestige multiplier no longer applies to mining speed

**Status:** accepted
**Tags:** prestige
**Supersedes:** —
**Superseded by:** —

## Decision

**The prestige multiplier no longer applies to mining speed** — ore yield and XP only

## Why

It was the wrong term in both halves of a run. Past instamine a block falls in one tick
and no further power buys anything, so the speed multiplier paid **nothing** through the
endgame it was sold as rewarding — which is also why the Amethyst banking rate scales
with the multiplier and not its square. What it did do was compound with yield and XP
over the **climb**, the stretch a reset player spends walking six pickaxe tiers back up
to Amethyst, shrinking it with the *cube* of the multiplier instead of its square. That
inverts what the reset is for: the climb is the content, and at `200` per rank a rank-10
climb ran eleven times quicker than a rank-1 one — 38 minutes down to 3½, with all
twelve mines and both progression axes traversed inside it. Removing the term leaves the
climb on `mult^-2.35`, and dropping the per-rank multiplier from `200` to `100`
alongside it brings the whole ladder's acceleration to ×2.1, which is an acceleration
the player feels without it eating the run. Cost accepted knowingly: the `docs/UI.md`
§6.8 preview and the tab-bar mocks re-quote at `×1.20 → ×1.30`.
