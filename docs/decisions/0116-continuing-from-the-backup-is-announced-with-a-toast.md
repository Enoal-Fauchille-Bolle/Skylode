# 0116 — Continuing from the backup is announced with a toast, not a frame

**Status:** accepted
**Tags:** save
**Supersedes:** —
**Superseded by:** —

## Decision

**Continuing from the backup is announced with a toast, not a frame**; and a save from a
**newer build** is offered `Quit` alone

## Why

Two questions [UI.md](../UI.md#83-the-session-state-machine) §8.3 left open, closed in
TUI phase 8. The toast: a frame exists to *ask* something, and here the player answers
"yes, go on" every time — what they lose is the few seconds the recovery frame itself
calls acceptable, so a modal would be a full stop in front of a footnote. The
`Quit`-only frame: every other refusal is a *broken* file, but a save from the future is
a **good** one this build is too old to read, so offering "start a new game" would let
the older build write over a run the player made with a newer one. It is the one refusal
in the table where starting again destroys something that was never damaged.
