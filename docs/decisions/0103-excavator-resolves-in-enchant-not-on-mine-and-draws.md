# 0103 — Excavator resolves in enchant

**Status:** accepted
**Tags:** enchants
**Supersedes:** —
**Superseded by:** —

## Decision

Excavator resolves in `enchant`, not on `Mine`, and draws **after** the three spatials —
`SPATIAL_PROC_ORDER` is a *prefix* of `PROC_ORDER`

## Why

It substitutes a drop and reshapes no cell, so putting it in
`Mine::resolve_spatial_procs` would give the grid a say in the inventory it has no
business having. The cost is that the draw order stops being guaranteed by one loop and
becomes a promise between two functions; the prefix relation is what makes that promise
testable in one assertion — the spatials come first, in order, none skipped. Appending
rather than inserting is also what made the enchant shippable against existing saves: a
level-0 enchant is skipped *before* it draws, so a player who never bought it replays on
exactly the dice their save was written with.
