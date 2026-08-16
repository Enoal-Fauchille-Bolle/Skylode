# 0039 — Efficiency stays capped by the tier

**Status:** accepted
**Amended:** once
**Tags:** pickaxe
**Supersedes:** —
**Superseded by:** —

## Decision

Efficiency stays capped by the **tier**; every other enchant, **Fortune included**, is
capped by the **world**

## Why

The argument for capping Efficiency by the tier load-bears and still does: a world that
raised Efficiency's ceiling would let one investment advance both axes and collapse the
two-axis gate into one, and it would make Netherite's cap of 15 unreachable outside the
End, deleting the final tier's whole reward.

## Amendments

### two cap groups, not three

Replaced: "Fortune by *nothing*; neither is world-capped".

The original argument was that three groups — keyed by tier, by world, by nothing — keep
the two progression axes independent. None of that applies to Fortune, which is keyed by
neither axis. What "capped by nothing" actually bought was a lever the player could max
at level 1 — the one upgrade in the game no progression paced. Fortune joins
`World::enchant_cap`; its ceiling of 10 is unchanged, it is simply reached in three
steps rather than one. This is the cap-group cost that
[0020](0020-fortune-capped-at-10-but-reached-progressively-3-6-10.md) names.
