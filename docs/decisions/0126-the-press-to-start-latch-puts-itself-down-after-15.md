# 0126 — The Press to start latch puts itself down after 15 minutes with no key…

**Status:** accepted
**Tags:** ui
**Supersedes:** —
**Superseded by:** —

## Decision

**The `Press to start` latch puts itself down after 15 minutes with no key at all, and
announces it: a dead-man's switch, not a cutoff**

## Why

**Amends the 15 s figure** in
[0096](0096-an-accessibility-toggle-space-starts-stops-with-a-15-s.md) and **restricts**
the rejected [0153](0153-an-unbounded-accessibility-toggle-no-inactivity-cutoff.md) to
what it actually protects. Two findings, both
from building it. First, the bound is **not** an anti-cheat measure, and the toggle row
already said why: a strip of tape over the mine key defeats any bound this could have,
and this project answers that class of question the same way every time — single-player,
offline, no leaderboard. What the bound really protects is the **balance** distinction
between active play and idle accrual, which the ~1 h–2.3 h prestige band was measured
against: a session left running overnight must not pay eight hours at the manual rate,
and fifteen minutes cuts that to ~3 % of itself. Second, and this is Enoal's call on the
shape: *cutoff* and *toggle* are **opposites** — a toggle says the state holds until the
player changes it, and a timer expiring under it makes the mode one the game silently
revokes. The two are reconciled by **scale and voice**: long enough that a present
player never meets it, and it **says so in a toast** when it fires. At 15 *seconds*
neither held — "I am watching my mine" and "I have left" are not distinguishable at that
scale, so the toggle degenerated into tapping every fifteen seconds, which is the very
thing the mode exists to avoid. The delay is stated in the Settings pane and read from
the constant, so the sentence and the behaviour cannot drift.
