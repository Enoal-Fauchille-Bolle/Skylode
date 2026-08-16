# 0152 — Auto-detecting a player whose auto-repeat is disabled

**Status:** rejected
**Tags:** ui
**Supersedes:** —
**Superseded by:** —

## Decision

Auto-detecting a player whose auto-repeat is disabled

## Why

It reads a difference that does not exist. "Held with auto-repeat off" produces one
event then silence; "tapped once" produces one event then silence — the same byte,
`0x20`, by construction. The toggle lives in Settings and is found there, without magic.
