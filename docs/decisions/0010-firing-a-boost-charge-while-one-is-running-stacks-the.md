# 0010 — Firing a boost charge while one is running stacks the duration

**Status:** accepted
**Tags:** boost
**Supersedes:** —
**Superseded by:** —

## Decision

**Firing a boost charge while one is running stacks the duration**, and the core does
not refuse it

## Why

Every boost is identical, so a second charge can buy nothing but time; refreshing the
timer instead would hand a player who fires at 25 of 30 seconds left only 5 seconds for
a full charge — a purchase that takes something away. Leaving the *rule* permissive is
what lets the confirmation live in the interface, which can see the running boost
through `active_boost`; a refusal in the core would put that question beyond every
front-end rather than in front of the player. Same split as the proc flash, where the
core supplies the data and the wall-clock experience lives outside.
