# 0123 — Settings does not pause the game, exactly as Help does not

**Status:** accepted
**Tags:** ui
**Supersedes:** —
**Superseded by:** —

## Decision

**Settings does not pause the game**, exactly as Help does not

## Why

TUI phase 9, and it closes a note phase 7 left open ([UI.md](../UI.md#7-spatial-procs)
§7): *the first session state that pauses the tick must clear the proc flash on the way
in*. It is answered by the case not arising — the run keeps ticking behind an open
Settings screen, so a flash goes on resolving normally and there is nothing to clear. A
screen that stopped the world would also be a place to **park** a run in, and the
auto-miner would go on emptying the mine regardless — so the pause would buy nothing and
cost the one invariant the note is about. The offline summary remains the only state
that pauses a running tick, and the `App` under it was built from the file a moment
earlier and holds no flash either.
