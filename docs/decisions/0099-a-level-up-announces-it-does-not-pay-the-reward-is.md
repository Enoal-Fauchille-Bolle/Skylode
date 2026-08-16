# 0099 — A level-up announces

**Status:** accepted
**Tags:** progression
**Supersedes:** —
**Superseded by:** —

## Decision

**A level-up announces; it does not pay.** The reward is filed against the level and the
player collects it on the Levels screen (`Enter` for one, `A` for all). Uncollected
rewards are lost to a prestige

## Why

Enoal's call, TUI phase 7, on playing the built game. The bundle used to land in the
inventory in the same instant the level was crossed, which made the reward a number
moving somewhere the player was not looking — and left the Levels roadmap with nothing
to *be*, since every figure on it described something already done. A reward you go and
take is an event; one that lands silently is a rounding on the inventory. The split
costs one field in the save (`unclaimed`, a `BTreeSet<u32>` with `serde(default)`, so no
`SAVE_VERSION` bump — a file written before it existed has nothing waiting, which is the
truth about that file and not a default standing in for one). **What it is not:** a gate
on progression. Levels 15 and 30 still open their dimension the instant they are reached
— the unlocked set is derived from the level, so *reaching* it is the unlock — and only
the boost charge is collectable there. **Losing them to a prestige** is the deep reset
applied honestly: carrying them across would make them the one thing that survives
without being the rank or its multiplier, and would pay into an inventory the reset has
just emptied. What is left is a legible tension: collect before you trade the run in.
The pacing harnesses now claim in their decision block, because a modelled player who
threw every bundle away is not the run the bands describe.
