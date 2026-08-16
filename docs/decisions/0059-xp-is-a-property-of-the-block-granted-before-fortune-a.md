# 0059 — XP is a property of the block, granted before Fortune

**Status:** accepted
**Amended:** once
**Tags:** progression
**Supersedes:** —
**Superseded by:** —

## Decision

**XP is a property of the block, granted before Fortune** — a per-block value climbing
with the progression, the cell of value worth **three times** its mine's common cell

## Why

Keeps the two progression axes independent. Fortune multiplies loot, not experience; if
it multiplied both, one investment would advance both axes and "neither axis alone
carries progression" would stop being true. Richness still speeds levelling, and
Excavator substitutes the loot and likewise grants no extra XP. 

## Amendments

### XP left the loot count

Replaced: "1 per ore cell, 9 per dense cell".

That tied XP to the *loot count* and had two consequences nobody chose. The Iron mine
out-levelled the End (4 968 XP a grid against 3 820), because a dense cell drops nine
while Amethyst drops one — so the three endgame mines, the only ones with no dense form,
were the worst XP in the game. And the whole level curve rode on a number picked for
crafting fidelity.

XP is now its own table, with the ratio dropped from 9 to **3** so the dial still
matters without the loot count dominating. `RAW_PER_DENSE_BLOCK` (9) stays the loot;
they are two ratios for two things. The ordering is a *property*, not a balance pass: a
grid is worth `base * (1 + 2w)` for dial weight `w`, so it is proportional to the base
at **every** dial setting, and rising bases order the twelve mines everywhere at once.
