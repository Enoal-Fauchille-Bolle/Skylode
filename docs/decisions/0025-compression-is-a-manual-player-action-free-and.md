# 0025 — Compression is a manual player action

**Status:** accepted
**Tags:** economy
**Supersedes:** [0132](0132-compression-as-inventory-management.md)
**Superseded by:** —

## Decision

Compression is a manual player action, free and lossless both ways (100 raw <-> 1
Compressed)

## Why

Revises [0132](0132-compression-as-inventory-management.md), the earlier
"denomination, not inventory management" call: a Compressed unit is
real inventory state, not a display format. Free and reversible so it can never
soft-lock a run. See [MECHANICS.md](../MECHANICS.md#compression).
