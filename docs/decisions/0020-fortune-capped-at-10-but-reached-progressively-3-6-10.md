# 0020 — Fortune capped at 10, but reached progressively: 3 / 6 / 10 by world

**Status:** accepted
**Amended:** once
**Tags:** enchants
**Supersedes:** —
**Superseded by:** —

## Decision

Fortune capped at 10, but **reached progressively**: 3 / 6 / 10 by world

## Why

The ceiling of 10 stands, and so does its reason — past 10, ore is abundant enough that
more Fortune is pointless (matches Pika).

## Amendments

### the pace, not the ceiling

Replaced: Fortune was buyable to 10 from level 1.

That made it the one lever in the game no progression slowed. It now shares
[`World::enchant_cap`](../MECHANICS.md#enchants) with the five specials, so Fortune X
exists only in the End. This is an amendment of rhythm, not of value, but it costs
[0039](0039-efficiency-stays-capped-by-the-tier-every-other.md) — the cap-group
decision, which had to become two groups rather than three.
