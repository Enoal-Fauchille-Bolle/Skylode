# 0054 — Richness has no weight cap

**Status:** accepted
**Tags:** mines
**Supersedes:** —
**Superseded by:** —

## Decision

Richness has no weight *cap*: the value-cell weight per level is an ordinary tunable

## Why

Reversed. This once read "the weight stays strictly below 100%, an invariant doing
double duty (anti-brick and anti-runaway)". Both jobs are done elsewhere: the free,
reversible **dial** is the anti-brick (an over-enriched mine is always one free
dial-move from harvesting its common cell again), and the geometric cost curve is the
anti-runaway (any finite production gain loses to an unbounded cost). So the weight is a
provisional formula (`value_weight`) phase 10 tunes; the only structural rule the core
enforces is the weaker "the two weights are never both zero", so the composition is
always a valid distribution. See [MECHANICS.md](../MECHANICS.md#mine-richness).
