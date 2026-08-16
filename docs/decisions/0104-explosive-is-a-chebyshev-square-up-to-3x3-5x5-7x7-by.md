# 0104 — Explosive is a Chebyshev square (up to 3x3 / 5x5 / 7x7 by level band)

**Status:** accepted
**Tags:** enchants
**Supersedes:** —
**Superseded by:** —

## Decision

Explosive is a **Chebyshev square** (up to 3x3 / 5x5 / 7x7 by level band); Jackhammer is
a **single full-width row** (`k = 1`), not a multi-row band

## Why

Squares and single rows read cleanly on the grid and keep the three spatials distinct:
Explosive a compact 2-D area scaled by level, Jackhammer a 1-D stripe scaled by mine
width, Nuke the whole grid. A multi-row Jackhammer would blur into Explosive, and a
rounded blob costs test complexity for no legibility gain. The radius bands line up with
the world caps (3 / 6 / 10), so a 7x7 can exist only in the End. Supersedes MECHANICS'
earlier "blob or square" and "band of `k` rows".
