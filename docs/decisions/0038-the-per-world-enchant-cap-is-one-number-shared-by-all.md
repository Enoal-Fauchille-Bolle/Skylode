# 0038 — The per-world enchant cap is one number shared by all five specials (3…

**Status:** accepted
**Tags:** enchants
**Supersedes:** —
**Superseded by:** —

## Decision

The per-world enchant cap is **one number shared by all five specials** (3 / 6 / 10),
not a cap per `(enchant, world)` pair

## Why

The cap is the *gate* — how much may be invested — and the enchant's effect scaling is
what the investment *buys*. Two dials, one job each. An effect that grows too fast at
high levels is a fault in its own curve (Explosive's square radius, or an enchant's
proc-chance curve) and is fixed there; capping that one enchant lower would fix a curve
with the wrong tool and cost the player a visible asymmetry for nothing. Also the
literal reading of "caps the enchant level available in each dimension" — one cap per
dimension. In code it is `World::enchant_cap`, a rule of the world.
