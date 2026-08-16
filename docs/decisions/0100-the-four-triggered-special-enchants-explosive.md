# 0100 — The four triggered special enchants (Explosive, Jackhammer, Nuke, Excavator)

**Status:** accepted
**Amended:** once
**Tags:** enchants
**Supersedes:** —
**Superseded by:** —

## Decision

**The four triggered special enchants (Explosive, Jackhammer, Nuke, Excavator) fire on a
random proc; frequency scales with level**

## Why

A proc is the *rare, legible burst* the seeded PRNG already exists for (see the
Fortune/Excavator split in [MECHANICS.md](../MECHANICS.md#fortune)), so this lets level
scale *frequency* cheaply while a separate curve scales Explosive's square. Nuke needs
no cooldown: emptying the mine is its own limiter, since a re-proc finds nothing until
the batch reset. Haste stays passive — a permanent multiplier, not a trigger. Procs fire
on active mining only; the closed-form auto-miner cannot draw. See
[MECHANICS.md](../MECHANICS.md#enchants).

## Amendments

### the spatials became probabilistic

Replaced: the spatials fired deterministically — Explosive and Jackhammer on every
break, Nuke on a level-shortened cooldown.

A deterministic blast on every break is not an event; it is the base swing with a larger
radius, and it left level with nothing to scale but area.
