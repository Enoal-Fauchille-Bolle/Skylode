# 0115 — q in a game returns to the title

**Status:** accepted
**Tags:** ui
**Supersedes:** —
**Superseded by:** —

## Decision

**`q` in a game returns to the title; only `Ctrl-C` ends the process. The title keeps no
run in memory, so `Continue` re-reads the file**

## Why

Enoal's call, TUI phase 8. It gives `Continue` **one meaning** on every path — after a
launch, after walking out of a game — instead of two that would have to be kept in
agreement. The cost is one read of a few kilobytes; what it buys beyond the single
meaning is that the save's whole round trip is exercised in *real play*, every time the
player puts a run down, rather than only in tests. It also makes a file that broke while
the game was up route to recovery on the way out, because leaving re-runs the same boot
routing a relaunch does. The ordering is load-bearing: the run is **written first**,
then the title is rebuilt from the file, or `Continue` would offer the state the player
had ten seconds ago.
