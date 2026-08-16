# 0124 — Press to start is a latch flipped on the rising edge of the hold…

**Status:** accepted
**Tags:** ui
**Supersedes:** —
**Superseded by:** —

## Decision

**`Press to start` is a latch flipped on the rising edge of the hold predicate, not on a
key event — so it is universal by construction rather than by detection**

## Why

TUI phase 9, implementing the accessibility toggle. A terminal with no release protocol
reports a hold as a stream of presses, at a rate the OS lets the player set anywhere
from 30 ms to half a second, so a latch toggled *per event* would strobe the pickaxe.
`HOLD_WINDOW` is already sized to outlast the longest **initial** repeat delay any
setting can produce, so the predicate it answers stays true for the whole hold and rises
exactly once. One rising edge, one toggle, whatever the repeat rate — and **no
capability detection and no branch per terminal**, which is the load-bearing half: this
is an accessibility option, and a mode offered only where the kitty protocol exists
would be absent from exactly the machines most likely to need it. Where a release *is*
reported it simply cuts the window early, so the second tap is immediate instead of
needing the ~1.1 s the window takes to lapse. It costs one `bool` and reuses a constant
that was already there.
