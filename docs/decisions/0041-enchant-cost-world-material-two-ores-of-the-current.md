# 0041 — Enchant cost = world material + two ores of the current progression tier

**Status:** accepted
**Amended:** once
**Tags:** enchants
**Supersedes:** —
**Superseded by:** —

## Decision

Enchant cost = world material + **two ores of the current progression tier**, keyed by
the enchant level and sharing one total 50 / 35 / 15

## Why

**Reverses "a mix of *earlier* mines' ores".** Buying a Nether enchant with Overworld
iron made the Nether's own mines spectators at their own tier; the fuel is now the pair
the player is mining *now* — Netherrack and Ancient Debris at level 4, Ancient Debris
and Obsidian at 5, Obsidian and Crying at 6. Keyed by the **level**, not the world: the
level is the progression scale, and the world cap already forbids reaching level 7
anywhere but the End, so the table is total without a bounds check. The three lines
**share** the step's total rather than adding to it, which puts enchants on the same
footing as the game's other composite prices. The End is the exception the shape forces:
one mine, and its rare material *is* the enchant material, so it quotes two lines (End
Stone plus Amethyst) on the sliding ramp of
[0055](0055-mine-upgrades-size-and-richness-are-paid-in-that-mine.md).
