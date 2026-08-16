# 0056 — Mines persist; no free action may ever put a broken block back

**Status:** accepted
**Tags:** mines
**Supersedes:** —
**Superseded by:** —

## Decision

Mines persist; **no free action may ever put a broken block back**

## Why

One rule covering two doors. Regenerating a mine on entry, or on a dial move, would be a
free batch reset — break the 4 Amethyst out of 200, leave, return to a full grid,
repeat. Depleting the mine *is* the price of the refill.
