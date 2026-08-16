# 0119 — A boost charge is bought on a fourth Upgrades sub-tab

**Status:** accepted
**Tags:** ui
**Supersedes:** —
**Superseded by:** —

## Decision

**A boost charge is bought on a fourth Upgrades sub-tab**, not with a key on the Mine
screen

## Why

Enoal's call, on wiring the boost. The core had shipped both doors — `buy_boost_charge`
and `fire_boost` — and the front-end reached neither, so *where* the shop lives was
still open. Upgrades is the screen where ore is spent, and it is the only one with a
detail pane large enough to state the multiplier, the duration, the stacking rule and
the reserve — none of which fits in a table cell. A buy-and-fire key on the Mine screen
was the cheap alternative and it fails on its own terms: it fuses the two acts the core
deliberately separates (a charge held versus a boost running), and leaves nowhere to
show a 3 Compressed Redstone price *before* it is paid. The sub-tab holds one row, which
is a departure from the screen's own reason for having sub-tabs — 96 rows do not fit in
21 — and is accepted knowingly: the pane is the product, the list is the handle.
[UI.md](../UI.md#544-boost) §5.4.4.
