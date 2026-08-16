# 0072 — The prestige multiplier is an integer in permille

**Status:** accepted
**Tags:** prestige
**Supersedes:** —
**Superseded by:** —

## Decision

The prestige multiplier is an integer in permille, applied **once per swing** over the
swing's total, with the fraction **carried** to the next swing

## Why

Every yield in the game is a whole number. Applied per block, a `×1.2` on a one-ore drop
truncates to `×1.0` — so rank I would be worth literally nothing through the
post-prestige early game, the exact stretch it exists to shorten. Totalling the swing
rescues a 200-cell blast; the carry rescues the single cell. It is the same device, for
the same reason, as the auto-miner's microblock carries.
