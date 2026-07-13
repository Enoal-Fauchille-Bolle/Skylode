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
- `prng`: the seeded PRNG state (so ticks and offline replay are reproducible).
- `last_seen`: wall-clock time of the last write, for offline accrual.
- `pickaxe`: tier, Efficiency, Fortune, and each enchant's level.
- `inventory`: a map from ore (raw and Compressed) to count.
- `level`: mining XP and current level.
- `worlds`: which worlds are unlocked.
- `mines`: per mine, its current size and its remaining-blocks grid state.
- `selected_mine`: the world and mine currently targeted.
- `prestige`: prestige rank and the derived permanent multiplier.
- `boosts`: active temporary boosts and their remaining timers.

The exact field names and shapes are settled during implementation; this is the
information the save must carry.

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
driving it. On launch, offline time is credited by replaying elapsed ticks
(capped) before the interactive loop starts.

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
