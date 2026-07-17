# Skylode - Systems

Technical systems that support the game: the save system, the tech stack, and the
architecture. For player-facing rules, see [MECHANICS.md](MECHANICS.md). For the
concept and gameplay loop, see [DESIGN.md](DESIGN.md).

## Save system

Fully preventing cheating in a single-player offline game is impossible: the save
and any key live on the player's machine. The goal is to make accidental
corruption unlikely and casual tampering harder. This is deterrence, not DRM. DRM
(technical measures that restrict how software is used or copied) is out of place
for a free solo game.

### Format

A single JSON file via `serde_json`. The state is one cohesive blob, so SQLite
would be over-engineering: there are no relational queries, no large datasets, and
no partial updates. JSON is simple and human-debuggable.

### Saved state

The `data` blob (see [Integrity](#integrity-hmac) below) serializes one cohesive
game-state struct. The fields, derived from the mechanics:

- `version`: schema version, for migrations.
- `prng`: the seeded PRNG state — a *position in a sequence*, not just the seed, so a
  reloaded run continues its dice rather than rerolling them.
- `last_seen`: wall-clock time of the last write, for offline accrual.
- `pickaxe`: tier, Efficiency, Fortune, and each enchant's level.
- `inventory`: a map from ore (raw and Compressed) to count.
- `level`: mining XP and current level.
- `worlds`: which worlds are unlocked.
- `mines`: per mine, its current size and its remaining-blocks grid state.
- `selected_mine`: the world and mine currently targeted.
- `prestige`: prestige rank and the derived permanent multiplier.
- `boosts`: active temporary boosts and their remaining timers.
- `config`: the player's *preferences* — colour palette (256 or the 16-colour
  fallback), ASCII-only glyphs, mining input mode, number format. **Not** game
  state, but it lives here anyway: see below.

The exact field names and shapes are settled during implementation; this is the
information the save must carry.

### Config in the save

There is deliberately **no separate config file**. One file, one path, no XDG
handling. Prestige does not touch the file — only the player deleting it does — so
preferences survive a run. Two costs are accepted knowingly: deleting the save
loses the preferences, and adding a config field bumps `version` and needs a
migration like any other schema change.

The consequence that matters is about the [HMAC](#integrity-hmac), which covers the
whole file, config included. **No hand-editing is tolerated, and Settings is the
only path to change a preference.** That is only tenable under one rule:

> The Settings screen exposes **every config field, and no game-state field**.

Both halves are load-bearing. Exposing every config field means nobody ever needs
to open the file to change a colour, so the tamper warning never fires on a
cosmetic edit — the HMAC's false positive goes to zero. Exposing no game-state
field means a player editing their Amethyst count still has to touch the file, and
still trips it — the HMAC's true positive is untouched.

It also forces a bootstrap rule. Config is inside a save that may be **missing**
(fresh install) or **untrusted** (HMAC mismatch), so the screens that run before
the save is validated — main menu, "terminal too small", and the recovery screen
below — **render with hardcoded defaults**. Reading preferences out of a save you
have just decided not to trust is a contradiction, and the recovery screen is the
first thing some players ever see.

### Save cadence

- Autosave every 10 seconds, only if state changed (a `dirty` flag).
- On important transactions (upgrade, prestige).
- On graceful exit.
- Update `last_seen` on every write, so offline accrual stays correct (see
  [MECHANICS.md](MECHANICS.md#offline-accrual)).

### Integrity (HMAC)

An HMAC is a keyed hash. On save: serialize the state to text, compute
`mac = HMAC-SHA256(key, text)`, and write:

```json
{ "version": 1, "data": "<serialized state>", "mac": "<hmac hex>" }
```

The key is embedded in the binary. On load: recompute the HMAC over `data` and
compare to the stored `mac`. A match means intact; a mismatch means modified or
corrupted. This is tamper detection, not prevention: the embedded key is
extractable. It catches hand-editing and corruption, not determined cheating.

### Robustness and recovery

- **Atomic writes:** write to a temp file, then `rename` (atomic on the same
  filesystem), so a crash mid-write cannot corrupt the save.
- **Backup:** keep the last known-good save as `.bak` (free thanks to atomic
  writes).
- **Schema versioning:** the `version` field enables safe migrations.
- **On integrity failure:** do not crash or punish. Inform the player the save
  looks modified or corrupted, and offer to restore the `.bak`, start a new game,
  or continue anyway at their own risk. Treat it first as corruption recovery, not
  anti-cheat enforcement.

## Tech stack

- Language: Rust.
- TUI: `ratatui` and `crossterm` (event loop and rendering).
- Serialization: `serde` and `serde_json` (save file).
- Time: `std::time::SystemTime`.
- RNG: `rand` plus `rand_chacha`, a seeded PRNG whose state lives in the save, for
  deterministic ticks. Specifically `ChaCha8Rng`, **not** `StdRng`: `rand` does not
  guarantee `StdRng`'s algorithm across releases, and an algorithm that changes
  under a save that stores a position in its sequence turns every existing run into
  a different one. `rand_chacha` guarantees reproducibility; that guarantee is the
  whole reason it is here. Both crates are taken with `default-features = false`,
  which strips OS entropy out of the core entirely.
- Atomic file write: `tempfile` (temp plus persist/rename).
- Integrity: `sha2` and `hmac`.
- Distribution: a single static binary (`cargo build --release`), cross-platform
  in the terminal.

## Architecture

Game rules live in a `core` crate (`skylode-core`), decoupled from the TUI
(`skylode-tui`). This keeps the rules testable (deterministic ticks, `#[test]`)
and leaves the door open for other front-ends later. The core owns the game state,
including the mine grid; the TUI only renders it and forwards input.

### Tick loop

The core advances on a fixed timestep of 20 ticks per second (see
[MECHANICS.md](MECHANICS.md#ticks)). One `tick(input)` call applies the held-Space
mining, the auto-miner, timers (boosts, cube regeneration), XP accrual, and enchant
procs, all from the seeded PRNG so a run is reproducible. Rendering is decoupled:
the TUI redraws on change at roughly 30 fps, reading the core state without
driving it.

Offline time is **not** replayed tick by tick. The MVP auto-miner is a flat passive
rate (see [MECHANICS.md](MECHANICS.md#auto-miner)), so what it produces over an
absence is a multiplication, not a simulation — and stepping 432 000 ticks to apply
one is work no player waits for and no test needs. Credit it in closed form on
launch, from the capped elapsed time (see
[offline accrual](MECHANICS.md#offline-accrual)). The tick loop drives the
*interactive* session only.

### Keyboard input

`tick(input)` takes a `space_held: bool`. Producing that bool is the TUI's job, and
it is harder than it looks, because **a terminal sends nothing when a key is
released**. The legacy encoding is "one key = its character", inherited from
teletypes where a key *was* a character and a character has no duration. The
release is not lost in transit: it is never encoded. A tty only knows "data stream
in, data stream out". So *hold Space* — the interaction
[DECISIONS.md](DECISIONS.md) settles on — is not expressible by default.

Two layers, and the second is the one that runs on most machines:

**Layer 1 — exact.** Call `crossterm::terminal::supports_keyboard_enhancement()`
at startup (note: in `terminal`, not `event`; it round-trips a query to the
terminal, so it must run before the event loop). If supported, push the kitty
keyboard protocol flags and read real `Press` / `Release`; pop them on exit. The
flags must be **both**:

```rust
KeyboardEnhancementFlags::REPORT_EVENT_TYPES
    | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
```

`REPORT_EVENT_TYPES` alone is silently useless here. The protocol sends
text-producing keys as raw UTF-8, and Space produces text — so it arrives as `0x20`
with no event-type field at all. The second flag is what forces Space through the
`CSI 32 ; 1 : 3 u` path where a release can exist. Windows needs neither: crossterm
reads the Console API there, which carries a key-down flag natively.

**Layer 2 — the window.** Everywhere else, including every VTE terminal (Ptyxis,
gnome-terminal, Console, Tilix), only OS auto-repeat is observable, and an
auto-repeated `0x20` is byte-identical to a fresh press:

```text
space_held = (now - last_space_event) < HOLD_WINDOW    // HOLD_WINDOW = 1100 ms
```

That is the whole mechanism: one subtraction, one comparison. No measurement, no
calibration, no persisted state — see [DECISIONS.md](DECISIONS.md) for why each of
those was tried and rejected. The 1100 is not a preference: the window must exceed
the largest initial auto-repeat delay a user setting can produce (Windows caps at
1000 ms), or mining cuts out during the gap and resumes, hitching on every hold.
Since the initial delay and the repeat interval differ, no single timeout avoids
both false positives and false negatives, so the design picks: up to 1.1 s of
over-mining after release, which is invisible against a 7-day offline cap.

The accessibility toggle is the same mechanism with two constants — a 15 000 ms
window extended by any key, plus Space cutting it explicitly.

**This does not weaken the core's determinism.** The contract is `tick(input)`: the
core is *given* `space_held`, it never infers it, so "same inputs, same outputs"
holds. What is not reproducible is the *session* — the same physical gesture can
produce different tick sequences on two machines. That is already true of any human
input, and is called out here only because determinism is load-bearing elsewhere in
this document.

### Core modules

The core is split by concern, each unit testable in isolation:

- `worlds`, `materials`: the static data (which ores, their world, hardness, and
  minimum pickaxe tier).
- `pickaxes`: tiers, Efficiency, Fortune, enchant levels, and `mining_power`.
- `mines`: the grid model, mixed content, break progress, batch reset, and size.
- `progression`: mining XP and level, world unlocks, and the two-axis gating.
- `enchants`: the five enchants, their per-dimension caps, and their effects.
- `economy`: costs (composite compressed plus raw), the compression denomination,
  and boosts.
- `prestige`: the reset and the permanent multiplier.
- `save`: serialization, HMAC, atomic write, and migration.

The TUI (`skylode-tui`) holds the screens (Mine, Mines, Inventory, Upgrades,
Stats), reads core state to render, and forwards keyboard input.
