# 0015 — A tier jump is paid in the tier being left

**Status:** accepted
**Amended:** once (phase 10)
**Tags:** pickaxe
**Supersedes:** —
**Superseded by:** —

## Decision

**A tier jump is paid in the tier being *left*, at that tier's step on the curve**

## Why

Leaving Gold costs Gold, not Diamond. The jump is the last thing a tier is *for*, so it
is priced as the tier's own final purchase rather than as a down payment on the next one
— the player spends what they have been mining, not a material the mine they are about
to unlock has not given them yet. It also settles a question the old shape could not
answer cleanly, since it took the material from one tier and the curve index from
another: one purchase, one tier, one concept. Two things fall out. Stone regains an
economic role it had lost past the opening (it now buys the first jump), and Ancient
Debris stops paying for the Netherite tier — it pays for Efficiency 1→5 *on* Netherite,
which is the reading [MECHANICS.md](../MECHANICS.md#worlds-and-materials)' "Ancient
Debris = Netherite tier upgrades" supports either way.

## Amendments

### phase 10 — the jump is keyed past the Efficiency cap, not at the tier index

Replaced: the jump read the curve at the bare tier index.

The jump now reads the shared curve one step *past* that tier's Efficiency cap
(`curve(5 + rank)`). This reunites the two pickaxe curves — within a tier the player
climbs Efficiency 1→5 (steps 0→4) then the jump (step 5 + rank) — so the jump is always
the tier's *dearest* step and never cheaper than the Efficiency it follows. The old
keying made leaving Wooden cost `curve(0)` against that same tier's `curve(4)`
Efficiency, so a tier's final act was its cheapest. The `+ rank` keeps the jump climbing
tier to tier; the material keying is unchanged.
