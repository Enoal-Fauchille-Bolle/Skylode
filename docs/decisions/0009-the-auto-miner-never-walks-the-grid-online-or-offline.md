# 0009 — The auto-miner never walks the grid, online or offline

**Status:** accepted
**Tags:** auto-miner
**Supersedes:** —
**Superseded by:** —

## Decision

**The auto-miner never walks the grid**, online or offline: it weights the expected
composition by the richness dial and multiplies

## Why

One model instead of two. The offline half has to be closed form (seven days is over
twelve million ticks of a flat rate), so a grid-walking online path would mean two code
paths, two test suites, two balance passes — and a player who watched for an hour being
paid differently from one who was away for an hour. It also keeps the auto-miner out of
the PRNG entirely, which is what makes a run's dice a function of the player's swings
alone, and is *why* the procs of
[0100](0100-the-four-triggered-special-enchants-explosive.md) are unreachable from it.
The cost, knowingly accepted: an idle mine does not visibly empty.
