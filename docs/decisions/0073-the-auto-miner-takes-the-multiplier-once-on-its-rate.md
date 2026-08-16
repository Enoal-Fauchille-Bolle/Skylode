# 0073 — The auto-miner takes the multiplier once

**Status:** accepted
**Amended:** once (phase 10, second pass)
**Tags:** auto-miner
**Supersedes:** —
**Superseded by:** —

## Decision

The auto-miner takes the multiplier once, on its **rate**, before the microblock split

## Why

Exact and free — the carries already there absorb the fraction — and it stops the
multiplier compounding as speed *and* yield into `×1.44` at rank I, which would make an
absence the best use of a rank the player bought with a run. Dividing last is what keeps
"one call of N ticks pays what N calls of one tick pay", the identity the closed-form
offline path rests on. 

## Amendments

### phase 10, second pass — the rule stands, its symmetry does not

Replaced: the rule was justified by a symmetry with the active path.

With mining speed out of the multiplier
([0068](0068-the-prestige-multiplier-no-longer-applies-to-mining.md)), the active path
has only one term left and can no longer compound at all — so this is now the *only*
place a double application is possible, and "speed" here means the auto-miner's own rate
rather than a mirror of the player's. The figure is `×1.21` at rank I, not `×1.44`,
since the per-rank multiplier moved from `200` to `100`.
