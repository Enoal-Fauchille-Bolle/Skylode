# 0111 — No "next affordable upgrade" hint at MVP

**Status:** accepted
**Tags:** ui
**Supersedes:** —
**Superseded by:** —

## Decision

**No "next affordable upgrade" hint at MVP**; the role goes to Stats' `This run` panel

## Why

Such a hint must **rank** upgrades, and nothing in the design ranks them:
`affordability` answers *can I*, `max_affordable` answers *how many*, neither answers
*which is worth it*. Building it means inventing a weighting — that is, balance — inside
the crate that holds no rules, calibrated against cost constants explicitly deferred to
phase 10. `This run` is already an ordered objective list with a cursor on the current
one: an answer that is written rather than computed.
