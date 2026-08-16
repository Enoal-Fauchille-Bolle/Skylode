# 0125 — Mining happens on the Mine screen and nowhere else, in both input modes

**Status:** accepted
**Tags:** ui
**Supersedes:** —
**Superseded by:** —

## Decision

**Mining happens on the Mine screen and nowhere else, in both input modes — written as
one condition rather than inherited from three**

## Why

TUI phase 9. `Hold` already behaved this way, but by accident: `Space` is decoded on one
screen only, so leaving stops refreshing the window and it lapses on its own up to 1.1 s
later. Stating it explicitly makes the stop instant, and makes the *latch* survive a tab
change — so leaving pauses and coming back **resumes**, which is what the player expects
of a mode they never pressed anything to stop. It also turns three implicit causes into
one line a test can aim at.
