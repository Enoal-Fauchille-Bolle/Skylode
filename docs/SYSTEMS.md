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
no partial updates. JSON is simple and human-debuggable. RON is a Rust-native
alternative if preferred.

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
- RNG: `rand`, a seeded PRNG whose state lives in the save, for deterministic
  ticks.
- Atomic file write: `tempfile` (temp plus persist/rename).
- Integrity: `sha2` and `hmac`.
- Distribution: a single static binary (`cargo build --release`), cross-platform
  in the terminal.

## Architecture

Game rules live in a `core` crate (`skylode-core`), decoupled from the TUI
(`skylode-tui`). This keeps the rules testable (deterministic ticks, `#[test]`)
and leaves the door open for other front-ends later. The core owns the game state,
including the mine grid; the TUI only renders it and forwards input.
