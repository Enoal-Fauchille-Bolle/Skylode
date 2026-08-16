# 0150 — Measuring the auto-repeat delay to calibrate HOLD_WINDOW

**Status:** rejected
**Tags:** ui
**Supersedes:** —
**Superseded by:** —

## Decision

Measuring the auto-repeat delay to calibrate `HOLD_WINDOW`

## Why

Two independent kills. It is **poisoned trivially** — tapping Space twice quickly at the
first hold measures the tap interval (~80 ms) instead of the initial delay, and every
later hold gets a 500 ms hole, forever, from a player doing nothing wrong. And it is
**unnecessary**, since a fixed 1100 ms window is strictly more restrictive than the 15 s
toggle already accepted. Filtering taps from auto-repeat is the original problem
restated.
