# 0156 — The opening is a Wooden pickaxe in the Stone mine

**Status:** accepted
**Tags:** progression
**Supersedes:** —
**Superseded by:** —

## Decision

A Wooden pickaxe in the Stone mine is the opening, confirmed as written.

## Why

It was never in doubt so much as never signed off, and it is what `Player::new` and
`GameState::new` have built all along.

Recording it matters for one concrete reason rather than for tidiness: the two reference
players in the phase-10 balance harness start there, so the measured pacing band of
[0030](0030-the-pacing-target-for-a-first-prestige-is-a-band-1-h.md) is a band about
*this* opening and no other. Moving the starting state would invalidate the measurement
without failing any test that names it.

This closes the gap between a default nobody chose on purpose and one that has now been
chosen.
