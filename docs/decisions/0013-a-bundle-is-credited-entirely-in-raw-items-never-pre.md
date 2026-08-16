# 0013 — A bundle is credited entirely in raw items

**Status:** accepted
**Tags:** economy
**Supersedes:** —
**Superseded by:** —

## Decision

A bundle is credited **entirely in raw items**, never pre-split into Compressed units

## Why

The two denominations are not fungible at the till — a player holding 115 raw still
cannot pay a line quoted as `1 Compressed` — so this decides what a reward can actually
buy, not merely how it prints. Crediting raw keeps compression *"a deliberate step in
the upgrade path rather than a cosmetic button"*: handing over ready-made Compressed
units would take that step on the player's behalf, and since
[`Inventory::compress`](../MECHANICS.md#compression) is free and lossless both ways,
pre-splitting removes no friction the player cannot remove in one keypress — only the
choice. It also makes the toast honest, since `+115 Quartz` is then exactly what lands.
