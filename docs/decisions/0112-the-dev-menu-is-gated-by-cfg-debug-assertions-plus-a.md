# 0112 — The dev menu is gated by #[cfg(debug_assertions)] plus a SKYLODE_DEV…

**Status:** accepted
**Tags:** project
**Supersedes:** —
**Superseded by:** —

## Decision

**The dev menu is gated by `#[cfg(debug_assertions)]` plus a `SKYLODE_DEV` environment
variable, and a cheated save is not marked**

## Why

Two layers because they answer two different questions: the `cfg` keeps the cheat doors
out of a release binary *entirely* — including the ones it needed in `skylode-core`,
where `Player::inventory_mut` is `pub(crate)` precisely so nothing outside can mint ore
— and the variable keeps an ordinary `cargo run` an ordinary game. `debug_assertions`
over a Cargo feature because every check the project runs — the hook's `clippy
--all-targets` and `doc`, plus `test` and the hand-run `tarpaulin` — builds the **dev**
profile, so this way the dev code is linted, documented and covered like the rules it
bypasses; a feature left off is none of those, which is the objection already recorded
against putting `serde` behind one. The known cost is that `cargo build --release` is
the one build the hook never runs, and it bit immediately — two imports reachable only
from dev code were `unused_imports` in release alone. **A cheated save carries no
mark**: a `cfg`-gated field would make the save format differ between profiles, an
always-compiled one spends a permanent line of the document on a debug-only feature, and
the underlying question is already settled by the free richness re-roll of
[0057](0057-the-free-geometric-re-roll-is-knowingly-left-open-at.md) — single-player,
offline, no leaderboard. See [DEV-MENU.md](../DEV-MENU.md).
