# 0049 — The dial changes a mine's mining speed

**Status:** accepted
**Amended:** once (phase 10)
**Tags:** mines
**Supersedes:** —
**Superseded by:** —

## Decision

**The dial changes a mine's mining *speed*, and not always in the player's favour**

## Why

Kept, not fixed, and written down because it was previously true of the code and stated
nowhere. The two cells of a mine can differ in hardness, so shifting weight between them
shifts the time a grid takes: enriching the Quartz mine makes it **2.4x slower**
(Netherrack 0.4 against Quartz Ore 3.0), enriching the End mine makes it **1.5x slower**
(End Stone 10 against Amethyst 15), and the Obsidian mine is unaffected (both cells 50).
For Quartz this is a consequence of keeping Minecraft's hardness table 1:1; for the End
it is *designed* (phase 10 hardened both cells to give Netherite's Efficiency a reason
to exist — see [0040](0040-permanent-upgrades-alone-never-instamine-ancient.md)). Either
way it reads as design: both dials are a real trade — rarer ore, slower grid.

## Amendments

### phase 10 — the End became a cost like the others

Replaced: the End was the exception, *faster* when enriched on its soft `1.5` Amethyst.

Hardening Amethyst to `15` above End Stone's `10` flips it to a genuine cost and puts
the prestige currency behind the mine's slowest cells, which is what makes Efficiency
worth buying to farm it.
