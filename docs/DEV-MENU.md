# Skylode - The dev menu

A nine-row overlay that reaches doors the rules deliberately keep shut: it credits
ore out of nothing, moves the mining level and the prestige rank, makes every
upgrade free, and skips time. It exists so that a state past the first hour of a run
— a Netherite dip, a capped enchant, an offline summary, a prestige preview — can be
*looked at* without playing to it.

It is not part of the game's interface, and it is not in [UI.md](UI.md). This file is
its specification.

## Running it

```sh
cargo run -p skylode-tui               # an ordinary game: ` does nothing
SKYLODE_DEV=1 cargo run -p skylode-tui # ` opens the menu
cargo run --release -p skylode-tui     # the menu is not in the binary at all
```

Then `` ` `` (backquote) from any screen. `↑↓` picks a row, `←→` turns the value on
it, `Enter` applies it, `Esc` or `` ` `` closes. **Both wrap** — the row list and every
value ladder on it, so `←` on `Mining level 1` goes to the cap and `→` past `1 000 000`
comes back to `1`. That is the game's own rule (`UI.md` §11) and not a dev-only
convenience; nothing in this menu is a quantity being spent, so nothing in it stops at
an end. A `DEV` marker sits at the right end of the tab row for as long as the session
has the menu, and turns red while free upgrades are on.

## The two-layer gate

| Layer | Mechanism | What it guarantees |
| --- | --- | --- |
| Compilation | `#[cfg(debug_assertions)]` | A `--release` binary contains **no** dev code — not the doors in `skylode-core`, not the overlay, not the key, not the strings |
| Activation | `SKYLODE_DEV` present in the environment | A debug build without it is an ordinary game |

`debug_assertions` rather than a Cargo feature, and the reason is the checks the
project runs. The pre-commit hook runs `clippy --all-targets -D warnings` and
`cargo doc -D warnings`; `cargo test` and a hand-run `cargo tarpaulin` complete the
set. All of them build the **dev** profile — so gating this way keeps the dev code
linted, documented and covered exactly like the rules it bypasses. Code behind a
feature left off is none of those things, which is the objection
`skylode-core/Cargo.toml` already records against putting `serde` behind one.

The price is stated where it is paid: **`cargo build --release` is the only build the
hook never runs**, so a `cfg`-gated door that stops compiling there would be found
late. Two imports in `app.rs` already had to be gated for exactly this reason — they
are reached from dev code alone and were `unused_imports` warnings in release only.
Run `cargo check --release --all-targets` after touching anything under a `cfg`.

The environment variable is read in `main` and nowhere else, which is the same rule
the seed and the clock follow: `main` is the outside. It is checked for *presence*,
so `SKYLODE_DEV=0` enables it too — a variable whose only job is to be set has no
business having a grammar of truthy strings.

## The rows

| Row | `←→` | `Enter` |
| --- | --- | --- |
| Free upgrades | flips it | reports which way it now reads |
| Amount | `1` → `1 000 000`, by tens | reports the figure |
| Give material | the fifteen materials | credits `Amount` **raw** of it |
| Give everything | — | credits `Amount` raw of all fifteen |
| Give experience | — | grants `Amount` xp and files the levels it crosses |
| Mining level | `1`..`50` | puts the player there |
| Prestige rank | `0`..`20` | sets the rank, leaving the run standing |
| Boost charges | `1`..`99` | adds that many to the reserve |
| Skip time | `1 m` .. `7 d` | rewinds the offline mark and resumes |

Three of them are worth a sentence each:

- **Give experience** is the only way to put something on the Levels screen to
  collect. *Mining level* deliberately does not queue rewards — a jump to 50 would
  bury the roadmap under thirty-odd bundles nobody earned — so use the experience row
  when the claim flow is what you are testing.
- **Mining level** is the one row that can move *down*, and it prunes any reward
  waiting above the new level. It has to: a reward for a level the player has not
  reached is a state `GameState::validate` refuses, so leaving it would build a run
  that cannot be saved.
- **Skip time** is two shipped calls and no new arithmetic — `dev_rewind` moves the
  mark, and the ordinary `resume` credits the absence, applies `OFFLINE_CAP` and
  builds the report. The toast prints that report's own figures.

## Free upgrades

The toggle does **not** stuff the wallet. It routes the Upgrades screen's `Enter` and
`M` through `skylode_core::game::dev`'s doors, which never consult an inventory at
all. Two consequences follow, and both are the point:

- **The caps still refuse.** Netherite Efficiency 15, a world's enchant cap, the end
  of a mine's ladder, a mine this run has never entered — none of those was ever
  enforced by a price, so none of them is bypassed. Only the till is.
- **The prices on the screen stay real.** A purse holding billions would make every
  figure on the Upgrades screen unreadable, which is the screen the toggle exists to
  let you look at.

One known oddity follows from that second point: the `✓ ~ ✗` marks in the ladder are
computed from what the player can *afford*, so a row marked `✗` still buys while the
toggle is on. That is honest — the mark answers "could you pay for this", and the
answer has not changed; what changed is that nobody is asking.

The dip modal is skipped in free mode. Its question is *"this costs you power — spend
the ore anyway?"*, and with nothing spent the question has no second half.

## What it deliberately does not do

- **No pickaxe, enchant or mine rows.** The free toggle makes the Upgrades screen
  itself free, with its real ladders and its real cursor. A second, worse copy of
  those three ladders inside a box would let a dev tool route around the interface it
  is meant to exercise.
- **No editing of the balance constants.** `tunables.rs` is `pub const`, inlined at
  compile time; making it editable at runtime means threading a struct through the
  core and into the save. Edit the file and rebuild.
- **No mark on a cheated save.** It was considered and rejected: a `cfg`-gated field
  would make the save format differ between profiles, and an always-compiled one puts
  a permanent line in the document to serve a debug-only feature. The project has
  already settled the underlying question — see
  [0057](../docs/decisions/0057-the-free-geometric-re-roll-is-knowingly-left-open-at.md)
  on the free richness re-roll: *single-player, offline, no leaderboard*.

## Where the code is

| What | Where |
| --- | --- |
| The doors into the rules | `game::dev`, an inline `#[cfg(debug_assertions)] pub mod` at the bottom of [crates/skylode-core/src/game.rs](../crates/skylode-core/src/game.rs) |
| The two field setters it needs | `Player::dev_set_level` / `dev_set_prestige` |
| The overlay, its rows and its state | [crates/skylode-tui/src/overlay/dev.rs](../crates/skylode-tui/src/overlay/dev.rs) |
| The key | `keymap::resolve`, step 3 |
| Applying a row, and the free routing | `App::apply_dev_row` and `App::buy_free_at_cursor` |
| The activation | `main::dev_requested` |

`game::dev` is a **child** module of `game`, which is why the whole thing needed no
new accessors: Rust privacy is *visible in the defining module and its descendants*,
so it reaches `GameState`'s private fields directly. A top-level `dev` module would
have had to open a `pub(crate)` door onto every field it wanted, and those doors would
still be there in release.

Almost every door composes a `pub(crate)` mutator the economy already calls after
debiting — `Pickaxe::upgrade`, `Enchants::upgrade`, `Mine::upgrade_size_level`,
`Player::add_experience`. That is what makes a dev purchase a real purchase with the
till skipped, rather than a second set of rules that could disagree with the first.

## Verifying a change to it

```sh
cargo test                                        # the dev code is in the debug profile
cargo clippy --workspace --all-targets -- -D warnings
cargo tarpaulin --fail-under 94 --out Stdout
cargo check --release --all-targets               # the build the hook never runs
SKYLODE_DEV=1 cargo run -p skylode-tui
```

And the claim the gate makes, checked directly:

```sh
cargo build --release
strings target/release/skylode | grep -ciE 'dev menu|SKYLODE_DEV|free upgrades'   # 0
```
