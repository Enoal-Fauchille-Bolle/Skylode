# 0007 — Level-up rewards: a bundle of ore

**Status:** accepted
**Amended:** once
**Tags:** progression
**Supersedes:** —
**Superseded by:** —

## Decision

Level-up rewards: a bundle of ore, plus a boost charge every fifth level and Emerald
every third

## Why

Keeps early levels satisfying without gating content. The bundle's budget is `10 ×
level` raw — **linear, not on the cost curve**, which is indexed by a track's step
(0–15) and would run its exponent to 50 if read at a mining level, paying out more in
one level-up than the dearest purchase in the game costs. Over a run the rewards come to
~3 % of everything the player must buy: an opening hand, not an income.

## Amendments

### the boost charge became periodic

Replaced: "ores / Compressed ore + a temporary boost".

That boost fired on *every* level — a 30-second window arriving that often is not an
event, it is a permanent uplift to base speed that phase 10 would have had to compensate
everywhere else. A charge every fifth level makes it an event again.
