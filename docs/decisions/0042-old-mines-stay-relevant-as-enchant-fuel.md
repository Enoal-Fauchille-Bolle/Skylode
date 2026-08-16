# 0042 — Old mines stay relevant as enchant fuel

**Status:** withdrawn
**Tags:** enchants
**Supersedes:** —
**Superseded by:** —

## Decision

~~Old mines stay relevant as enchant fuel~~

## Why

**Withdrawn, and it never worked.** It claimed to solve "mines become useless once their
tier is behind you". Costing the fix out shows it could not: a geometric curve
concentrates its money at the top, and an Overworld ore could only ever fuel the
Overworld's three enchant levels — **2.3 % of an enchant's lifetime budget** at the
original slope, and *less* after any slope increase, since raising the slope inflates
only the End. Flattening the enchant curve (base ten times higher, slope 1.25) raises it
to 11.5 %, which is real money and the reason for that shape, but it is a mitigation and
not a solution. **The problem is left open rather than declared solved**: once a mine's
two tracks are maxed, an Overworld ore has no unbounded sink, and inventing one is
post-MVP work. See [ROADMAP.md](../ROADMAP.md).
