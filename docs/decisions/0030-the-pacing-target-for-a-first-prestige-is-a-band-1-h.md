# 0030 — The pacing target for a first prestige is a band

**Status:** accepted
**Amended:** twice (both downward)
**Tags:** prestige
**Supersedes:** —
**Superseded by:** —

## Decision

**The pacing target for a first prestige is a *band*, ~1 h to ~2.3 h, and it is
deliberately short**

## Why

The criterion every balance change is judged against, so it belongs here rather than in
a harness comment.

The band is measured by two reference players in the balance harness — a
**speedrunner** that rushes the prestige gates (the floor, ~1 h) and a
**completionist** that maxes the pickaxe, every enchant and every mine first (the
ceiling, ~2.3 h).

Two consequences worth naming. The floor is **XP-gated, not cost-gated** — the
speedrunner finishes its pickaxe before level 50 and then waits — so raising prices
barely moves it and only the level curve does. And the ceiling's last stretch is
enchant-fuel farming across every world, which is the completionist's *signature
activity*: it was left uncut on purpose when trimming it further would have hollowed out
the thing that makes a completionist run different.

Guarded at **both ends**, by `the_first_prestige_lands_inside_the_pacing_window` and
`the_completionist_ceiling_stays_inside_its_window` — one test per edge, because a band
held at one end is not held. The ceiling needs its own guard precisely *because* the
enhancement was given its own slope
([0029](0029-each-upgrade-track-carries-its-own-base-and-growth.md)) so that pricing it
could not touch the floor: put that slope back to the shared `1.45` and the speedrunner
still prestiges at 1.0 h to the tick, while the completionist silently returns to 5.4 h.

## Amendments

### 15–25 h became ~1 h to ~2.3 h

Replaced: a target of 15–25 h of active play, revised down twice by Enoal.

The reason is about *this* game rather than about idle games generally: Skylode is
played by holding one key. There is no movement, no combat, no other players to fill the
time, so an hour of Skylode is not an hour of the game it borrows its numbers from, and
length that would read as depth there reads as tedium here.

### the spread was tightened, not merely capped

Replaced: a ceiling of 2.7 h against a 1 h floor.

Rejected as *too far apart*. Enoal's call on the spread matters as much as on the ends,
so the band is kept tight rather than only bounded above.
