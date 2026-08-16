# 0045 — Releasing the mine key forfeits the block in progress

**Status:** accepted
**Tags:** runtime
**Supersedes:** —
**Superseded by:** —

## Decision

**Releasing the mine key forfeits the block in progress** — the counter to 0, the aim
untouched

## Why

What makes the interaction *continuous* rather than merely active: if a release only
paused `break_progress`, a run of taps would break a block in exactly as many held ticks
as a hold, and holding the key — the one input the game has — would be a comfort rather
than the mechanic. The cost scales with hardness, up to ~75 s on an Obsidian, which is
where it should bite. **Keeping the aim is the load-bearing half**: a mine's two cells
are not worth the same, so a release that re-rolled the target would turn tapping into a
way to fish for the valuable one — a strategy nobody chose, and one that would reward
the opposite of holding. It also keeps a released tick **inert in the generator** (no
draw), so a run's dice still cannot depend on how long its player sat in a menu. Two
consequences accepted: the punishment is only as prompt as the terminal (exact under
kitty, up to `HOLD_WINDOW` = 1100 ms late elsewhere — a floor on the cost, never a false
one), and the planned accessibility toggle's 15 s inactivity cutoff will forfeit a block
on firing, which is the same event and needs no second rule.
