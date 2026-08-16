# 0060 — Every block drops something

**Status:** accepted
**Tags:** mines
**Supersedes:** —
**Superseded by:** —

## Decision

Every block drops something; each world's filler drops its own material (Stone,
Netherrack, End Stone)

## Why

The filler is the block the player breaks most often, so one that paid nothing would
make the bulk of their swings a tax. The three worlds now agree on the rule, and
`Block::material` is total — no `None` branch for a case that cannot happen.
