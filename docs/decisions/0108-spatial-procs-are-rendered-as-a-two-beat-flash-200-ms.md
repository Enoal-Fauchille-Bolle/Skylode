# 0108 — Spatial procs are rendered as a two-beat flash (~200 ms)

**Status:** accepted
**Tags:** ui
**Supersedes:** —
**Superseded by:** —

## Decision

**Spatial procs are rendered as a two-beat flash (~200 ms), non-blocking and entirely
front-end**; the core's `Event` carries the **cell list**

## Why

The shape *is* the reward — painting the cells before clearing them is what makes a 7x7
read as a square rather than be inferred from an absence. And an animation is nothing
but an ambient clock, so it belongs on the side of the boundary where the wall clock is
already legal: the core gains no timer, no animation state and no test change, and
`tick()` stays a pure function of `(state, input)`. The corollary is a signature
requirement — a front-end handed `Nuke { blocks: 200 }` cannot draw the shape.
