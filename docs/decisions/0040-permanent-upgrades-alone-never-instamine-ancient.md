# 0040 — Permanent upgrades alone never instamine Ancient Debris or Obsidian

**Status:** accepted
**Tags:** enchants
**Supersedes:** —
**Superseded by:** —

## Decision

Permanent upgrades alone never instamine Ancient Debris or Obsidian; `HASTE_PER_LEVEL`
is bounded **above**

## Why

Netherite + Efficiency 15 + Haste at the End's cap is 705, short of Ancient Debris' 900.
The temporary Redstone boost is what closes the gap, and a ceiling the player cannot buy
past is its only reason to exist. Raising the factor to 0.3 reaches 940 and deletes that
role — the rejected option, not an untried one.
