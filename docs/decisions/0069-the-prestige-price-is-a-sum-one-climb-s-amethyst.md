# 0069 — The prestige price is a sum

**Status:** accepted
**Tags:** prestige
**Supersedes:** —
**Superseded by:** —

## Decision

**The prestige price is a sum — one climb's Amethyst income plus a growing surcharge —
not a step on a geometric curve**

## Why

Splitting the price names the thing the design actually controls. A run banks ~5 000
Amethyst *by itself* mining experience between the End and the level cap, invariant in
the rank (the multiplier scales XP and yield together, so ore per level does not move)
and near-invariant in strategy (5 167 speedrun, 4 916 completionist). So the part of any
price that costs time is `price − 5 000`, and that difference moves far faster than the
price does — a price rising ×2.8 across the ladder moves it ×10. Tuning a total
therefore means tuning a small number through a large one. Naming the surcharge puts the
dial on the quantity with an opinion attached, and because the income rate is known (~2
700 × multiplier per hour) it is tuned in **minutes**: ~20 at rank 1, ~34 at rank 10.
The old curve's two failures were one bug — a price with no fixed relationship to the
income paying it — and it failed in *both* directions: invisible for six ranks (under
the free 5 000, so literally zero time), then the whole run for the next four. The new
invariant is a comparison of two slopes, `PRESTIGE_SURCHARGE_PER_RANK_PERMILLE` against
`PRESTIGE_MULT_PER_RANK_PERMILLE`, which replaces the compile-time assertion against the
size track — two numbers with no player-facing relationship, and satisfied throughout by
the curve that cost nothing. **The 5 000 is measured, not chosen**, so it can go stale
silently and in the dangerous direction (upward makes prestige *free*);
`one_climb_still_banks_about_what_the_price_is_aimed_at` guards it.
