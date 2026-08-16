# 0102 — Excavator rolls once per swing, on the impact block only

**Status:** accepted
**Tags:** enchants
**Supersedes:** —
**Superseded by:** —

## Decision

Excavator rolls **once per swing, on the impact block only** — never on the cells a
blast took

## Why

Keeps the number of PRNG draws per swing fixed. Rolling each broken cell would make the
draw count depend on a blast's geometry, so the sequence a save resumes would vary with
the grid — and a sequence no golden vector can pin is one no bug report can reproduce.
It also stops a maxed Nuke from being the game's best Compressed source, which is
Excavator's job. Follows [MECHANICS.md](../MECHANICS.md#enchants)' "a swing that lands a
block rolls once per enchant".
