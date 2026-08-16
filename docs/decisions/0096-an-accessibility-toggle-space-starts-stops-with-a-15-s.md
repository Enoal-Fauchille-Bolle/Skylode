# 0096 — An accessibility toggle

**Status:** accepted
**Amended:** once (TUI phase 9)
**Tags:** ui
**Supersedes:** —
**Superseded by:** —

## Decision

An accessibility toggle: Space starts/stops, with a 15 s inactivity cutoff

## Why

Holding a key for hours excludes anyone with an RSI or a motor impairment, and it buys
**nothing** against a cheater — a strip of tape defeats it. The toggle-with-timeout is
*exactly as* exploitable as the hold (both fall to the same tape), so it costs nothing
in integrity, while its 15 s of AFK is noise against a 7-day offline cap. It is not a
second system: hold and toggle are one mechanism with two constants — window 1100 ms
extended by Space, or 15 000 ms extended by any key. 

## Amendments

### TUI phase 9 — 15 s became 15 min, and a cutoff became a dead-man's switch

Replaced: a 15-second inactivity cutoff.

The 15 s was chosen against the exploit and never against play, and at that scale it
made the toggle into tapping every fifteen seconds. The record's own argument is what
settled it: if a strip of tape defeats the bound, the bound is not what protects
integrity, and it should be sized against the thing it does protect. See
[0126](0126-the-press-to-start-latch-puts-itself-down-after-15.md).
