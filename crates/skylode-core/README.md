# skylode-core

The rules of [Skylode](https://github.com/Enoal-Fauchille-Bolle/Skylode), a
terminal idle/incremental mining game: worlds, blocks, materials, the pickaxe and
its enchants, the mine grid, progression, prestige, and the run itself.

This crate is the game *minus* the game — no terminal, no files, no clock. The
front-end that turns it into something playable is
[`skylode-tui`](https://crates.io/crates/skylode-tui).

## Determinism is the contract

Everything here is a pure function of state plus input. That is the property the
crate is built around, and it is enforced by the compiler rather than by
discipline:

- **No ambient randomness.** `rand` and `rand_chacha` are pulled in with
  `default-features = false`, which drops `thread_rng` and `os_rng` — OS entropy
  is not merely unused, it is not compiled in. Every draw comes from a seeded
  `ChaCha8Rng` whose position lives in the save, so a reloaded run continues its
  sequence instead of rerolling it.
- **No wall clock.** The caller passes `now` in. The core never asks what time it
  is, which is what lets a test replay seven days of absence in a microsecond.
- **No I/O.** [`save`](https://docs.rs/skylode-core/latest/skylode_core/save/)
  turns a run into text and back; writing that text somewhere is the front-end's
  job.

Two consequences worth knowing before depending on this: the RNG **draw order is
part of the format**, pinned by a golden-vector test, and the save document's
shape is pinned byte for byte by another. A change to either is a question about
every save on disk, not a refactor.

## Using it

```rust
use skylode_core::game::{GameState, Input};
use std::time::SystemTime;

// The seed and the clock come from the caller. Same seed, same run.
let mut state = GameState::new(0xB0BA_CAFE, SystemTime::now());

// The simulation is a fixed 20 ticks per second. One second of holding the key:
for _ in 0..20 {
    for event in state.tick(Input { space_held: true }) {
        println!("{event:?}");
    }
}
```

`tick` returns only what the player is owed an announcement about — a level-up, an
enchant proc — never a log of everything. An ordinary block breaking is already
visible in the inventory.

## What is in it

`world`, `block`, `material` and `inventory` are the static data; `mine` and
`mine_kind` are the grid the player digs and the twelve canonical mines it can
be; `pickaxe`, `enchant` and `upgrade` are the tool and its roadmap; `player`,
`reward` and `prestige` are progression; `economy` is what things cost; `game`
holds the run and is the only place the rules are composed; `rng`, `save`,
`tunables` and `error` are the machinery around them.

The rustdoc is written to explain *why* a formula or a visibility is shaped the
way it is, so it is worth reading rather than skimming:
[docs.rs/skylode-core](https://docs.rs/skylode-core). It is built with
`--document-private-items`, deliberately — the interesting links point at
`pub(crate)` items, because the argument for why one thing is public usually
names the thing that is not.

## Versioning

Pre-1.0, so **the minor number is the breaking axis** (`0.2.3` resolves as
`>=0.2.3, <0.3.0`). The version is shared with `skylode-tui`: the two crates ship
together, inside one binary, to one player. `SAVE_VERSION` is a separate number
and moves for its own reasons — see
[CONTRIBUTING.md](https://github.com/Enoal-Fauchille-Bolle/Skylode/blob/main/CONTRIBUTING.md#versioning-and-releases).

## License

MIT. See
[LICENSE](https://github.com/Enoal-Fauchille-Bolle/Skylode/blob/main/LICENSE).
