# 0019 — Efficiency's level² + 1 is earned from level 1, not level 0

**Status:** accepted
**Tags:** pickaxe
**Supersedes:** —
**Superseded by:** —

## Decision

Efficiency's `level² + 1` is earned from level 1, not level 0

## Why

Minecraft guards the bonus behind `if (i > 0)`. The `+ 1` is what makes the first level
a discrete `+2` jump, not a flat bonus every pickaxe collects; paid at level 0 it hands
a fresh Wooden pickaxe 50% more speed than it earned and breaks the 1:1 times of
[0018](0018-break-time-is-ceil-30-hardness-mining-power-minecraft.md).
