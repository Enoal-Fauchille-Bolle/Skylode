# 0106 — The palette is 24 entries

**Status:** accepted
**Tags:** ui
**Supersedes:** —
**Superseded by:** —

## Decision

**The palette is 24 entries, one per `Block` variant, and the contrast gate is
*pairwise*, not global** — `ΔE >= 40` **and** `ΔL* >= 20` in CIELAB within each
`(common, value)` pair

## Why

The 24 variants partition exactly into `MineKind`'s twelve pairs, and two mines are
never on screen together: the requirement is therefore not "24 mutually distinguishable
colours" — which 256 indices permit but no eye retains — but twelve pairs with strong
internal contrast. Hue follows Minecraft, so a material is recognised rather than
learned; lightness is the free variable, because it is the only channel that survives
both a poor terminal and colour blindness. Two near-collisions *between* mines are
accepted knowingly: both are faithful, and neither pair is ever co-visible.
