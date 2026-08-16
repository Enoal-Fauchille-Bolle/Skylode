# 0008 — The auto-miner pays ore and never XP

**Status:** accepted
**Tags:** auto-miner
**Supersedes:** —
**Superseded by:** —

## Decision

**The auto-miner pays ore and never XP**, and it runs at all times rather than only
while the player is idle

## Why

Levels open worlds, ore opens pickaxes, and *"neither axis alone carries progression"*.
An auto-miner that granted experience would open the Nether and the End over a week's
absence, turning the level axis into a clock — the same collapse Fortune was kept off
the XP to prevent, applied to elapsed time instead of to an upgrade. It also sits beside
the settled rule that the triggered enchants fire on active mining only: playing pays
ore, XP and procs; being away pays ore. Running it during active play is the other half
— "idle accrual comes only from the auto-miner" says where passive income *originates*,
and a helper that stopped when the player started would tax playing. Cheap consequence:
nothing on the offline path can cross a level, so the whole level-up cascade keeps
exactly one caller. **Sharpened (TUI phase 7): "at all times" includes the very first
tick.** Playing the built game raised the question of whether the helper should be gated
behind a level, a purchase or the first prestige — it is visibly running before the
player has swung at anything, at 0.22 blocks a second. Enoal's call: **no gate**. A gate
would have to sit above `credit_auto_mining` in both the tick and the offline accrual,
which is two places to keep one rule; it would make an early absence pay nothing, which
is the opposite of what an idle game's opening should teach; and the phase 10 pacing
window was measured with the helper running from zero, so gating it moves a balance gate
to buy back an unrequested difficulty. See [MECHANICS.md](../MECHANICS.md#auto-miner).
