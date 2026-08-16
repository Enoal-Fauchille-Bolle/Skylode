# 0071 — The prestige reset also takes the boost reserve

**Status:** accepted
**Tags:** prestige
**Supersedes:** —
**Superseded by:** —

## Decision

The prestige reset also takes the boost reserve, the auto-miner's carries and the mines
left behind — but **never the seeded generator**

## Why

The first three would survive by omission rather than by decision: converted ore, the
charges the erased levels granted, and grids a level-1 player can no longer enter. The
generator is the opposite case — its *position* is run state, and rewinding it would
deal the player back an identical second run, same grids and same procs, which is the
one thing a reset for the sake of re-walking must not do.
