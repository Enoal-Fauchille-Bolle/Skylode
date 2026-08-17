# 0121 — Firing onto a running boost asks no confirmation

**Status:** accepted
**Tags:** ui
**Supersedes:** —
**Superseded by:** —

## Decision

**Firing onto a running boost asks no confirmation**, and **`M` on the Boost sub-tab has
no cap to stop at**

## Why

Two consequences of the same fact, both Enoal's call. The confirmation
[0010](0010-firing-a-boost-charge-while-one-is-running-stacks-the.md) leaves to the
interface was reasoned about when a second charge *replaced* the first, where firing at
25 of 30 seconds left cost the player 25 seconds; charges stack by
**addition**, so an early fire spends only the choice of when, and a confirm whose only
sensible answer is yes is a keypress. The same addition is what makes a held `b`
harmless — auto-repeat is byte-for-byte a fresh press under the legacy encoding and
cannot be filtered, so the reserve is spent early and none of it is destroyed. `M`
follows from the boost being **the only uncapped sink in the economy** — every other
track ends at a ceiling the game defines, and this one ends at the purse: "as far as
possible" can empty a Redstone reserve the enchant tracks are also paid from.
Taken with that stated: one meaning for `M` on all four sub-tabs beats an exception on
one.
