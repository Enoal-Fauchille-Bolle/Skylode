# 0092 — The HMAC covers the whole save

**Status:** accepted
**Tags:** save
**Supersedes:** —
**Superseded by:** —

## Decision

The HMAC covers the whole save, config included. **No hand-editing is tolerated;
Settings is the only path**

## Why

Follows from [0091](0091-config-lives-in-the-save-there-is-no-separate-config.md) rather
than fighting it. Settings must therefore expose **every config field and no game-state
field**: a player who wants a different palette never needs to touch the file, so the
tamper warning never fires on someone changing a colour — while a player editing their
Amethyst count trips it, which is precisely its job. The rule is what keeps the HMAC's
false positive at zero and its true positive intact.
