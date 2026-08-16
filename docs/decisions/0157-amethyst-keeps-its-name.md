# 0157 — Amethyst keeps its name

**Status:** accepted
**Tags:** mines
**Supersedes:** —
**Superseded by:** —

## Decision

The End's signature ore keeps the name Amethyst, rather than taking an invented one.

## Why

The question was whether the End's rich ore should stop borrowing a material Minecraft
puts in the Overworld. The answer follows the rule the palette already obeys: a material
is meant to be **recognised, not learned**, which is why hue follows Minecraft rather
than being invented.

An invented name would add one mapping for the player to memorise and close no
ambiguity, since nothing else in the game is called Amethyst. And the lore argument is
one the game declines everywhere else — Ancient Debris, Obsidian and Quartz are all
taken as they are. That Amethyst also serves as the prestige currency is a reason to
keep it legible, not to rename it.

Renaming would have been cheap — `Material::name` is a display name, deliberately kept
apart from the save key — so this is a choice and not a constraint.
