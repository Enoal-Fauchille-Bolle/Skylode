# 0091 — Config lives in the save; there is no separate config file

**Status:** accepted
**Tags:** save
**Supersedes:** —
**Superseded by:** —

## Decision

**Config lives in the save**; there is no separate config file

## Why

One file, one path, no XDG handling. The save is not reset by prestige — only by the
player's own choice — so settings survive a run. The cost is accepted knowingly:
deleting the save loses the palette and glyph settings, and a config change bumps the
save `version` and needs a migration.
