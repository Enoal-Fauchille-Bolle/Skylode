# 0051 — An Overworld ore mine's common cell is the ore itself

**Status:** accepted
**Tags:** mines
**Supersedes:** —
**Superseded by:** —

## Decision

An Overworld ore mine's common cell is the ore itself, never worthless filler

## Why

Rejects the Minecraft-style "iron mine is mostly Stone with veins". The rule is about
*pace*: unlocking a mine must not drop the player into breaking mostly-valueless Stone,
which would stall progression at the moment it should accelerate — and would reopen
"pickaxe tier opens mines" (a wooden pickaxe could break 85% of the iron mine's cells)
and make "a mine funds its own growth" ambiguous (paid in iron, or in the stone it
mostly produces?). It binds where pace is fragile, the Overworld tier ladder, so its
eight mines are pure same-material (ore + dense form). It does **not** forbid a themed
common cell elsewhere: the Nether's Quartz mine is Netherrack + Quartz Ore — Netherrack,
otherwise the one material with no function, becomes that mine's own growth currency —
and the two-material Obsidian and End mines are common + rare by design.
