# 0067 — Prestige is a deep reset (including XP)

**Status:** accepted
**Amended:** once (phase 10, second pass)
**Tags:** prestige
**Supersedes:** —
**Superseded by:** —

## Decision

Prestige is a deep reset (including XP), keeping only prestige rank and its global
multiplier

## Why

Re-walking the progression is the point; the multiplier makes it fast. **Measured (phase
10):** the ladder is a **U** — successive runs take 1.00, 0.68, 0.52, 0.41, 0.29,
**0.22** h, then turn back up through 0.39, 0.85, 1.76 to 3.48 h at rank 10. So the loop
has a floor around rank 6 and a wall after it, which is the genre's intended shape: an
acceleration the player feels, then a price that outgrows it. This also **sharpens a
claim that was too strong**: the reason given for pricing prestige above the size track
was that a gentler slope makes "each prestige cheaper than the last in real time". Each
prestige *is* cheaper than the last, for six ranks — what the doubling prevents is that
continuing **forever**. The distinction matters because the compile-time assertion
(`PRESTIGE_COST_GROWTH > SIZE_COST_GROWTH`) compares two *numbers*, while the property
the design wants is about *time*, with the yield multiplier, the XP multiplier and the
whole re-walk sitting in between. Only a simulation can close that gap, which is why
`the_prestige_loop_accelerates_then_turns_back_up` asserts the shape rather than the
constants. 

## Amendments

### phase 10, second pass — the U is rejected and the wall's reason inverted

Replaced: the ladder was a **U** — successive runs taking 1.00, 0.68, 0.52, 0.41, 0.29,
**0.22** h, then turning back up through 0.39, 0.85, 1.76 to 3.48 h at rank 10 — and
that shape was taken as the genre's intended one: an acceleration the player feels, then
a price that outgrows it.

The measurement stands as a record of what the geometric price did; what changed is the
verdict on whether that was the shape to want. The three records that replace it are
[0068](0068-the-prestige-multiplier-no-longer-applies-to-mining.md),
[0069](0069-the-prestige-price-is-a-sum-one-climb-s-amethyst.md) and
[0070](0070-the-prestige-loop-is-endless-by-design-and-the-price.md).

It also sharpened a claim that was too strong. The reason once given for pricing
prestige above the size track was that a gentler slope makes "each prestige cheaper than
the last in real time". Each prestige *is* cheaper than the last, for six ranks — what
the doubling prevented was that continuing **forever**. The distinction matters because
the compile-time assertion (`PRESTIGE_COST_GROWTH > SIZE_COST_GROWTH`) compared two
*numbers*, while the property the design wants is about *time*, with the yield
multiplier, the XP multiplier and the whole re-walk sitting in between. Only a
simulation can close that gap.
