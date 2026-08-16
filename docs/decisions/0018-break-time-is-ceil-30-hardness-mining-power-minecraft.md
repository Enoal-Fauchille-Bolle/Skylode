# 0018 — Break time is ceil(30 * hardness / mining_power)

**Status:** accepted
**Tags:** pickaxe
**Supersedes:** —
**Superseded by:** —

## Decision

Break time is `ceil(30 * hardness / mining_power)`, Minecraft's formula unabridged

## Why

The 30 is the *conversion* between dig speed and hardness (`getDestroyProgress` divides
by it), not a knob. It is what makes the 1:1 hardness table above mean 1:1 break *times*
— without it every block in the game instamines from the first swing. A balance pass
that wants faster mining moves `base_tier`, which is already ours. Minecraft's other
divisor (100, wrong tool) has no counterpart: the tier gate refuses instead of slowing.
See [MECHANICS.md](../MECHANICS.md#breaking-a-block).
