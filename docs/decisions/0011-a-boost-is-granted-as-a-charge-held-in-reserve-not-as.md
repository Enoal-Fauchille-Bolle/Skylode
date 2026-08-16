# 0011 — A boost is granted as a charge held in reserve, not as a running boost

**Status:** accepted
**Tags:** boost
**Supersedes:** —
**Superseded by:** —

## Decision

A boost is granted as a **charge held in reserve**, not as a running boost

## Why

Every boost in the game is identical (`BOOST_MULTIPLIER`, `BOOST_DURATION_TICKS`), so a
stored one carries no information beyond *how many* — the reserve is a count, and
`Boost` stays the type of a boost that is **running**. The deciding case is offline
accrual: a lump of experience can cross several levels at once, and boosts that started
themselves would burn down in a window nobody is watching. It also removes a stacking
question rather than answering it, since two charges never overlap unless the player
chooses to overlap them. `economy::buy_boost` sells a charge for the same reason.
