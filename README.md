# Skylode

[![CI](https://github.com/Enoal-Fauchille-Bolle/Skylode/actions/workflows/ci.yml/badge.svg)](https://github.com/Enoal-Fauchille-Bolle/Skylode/actions/workflows/ci.yml)
[![codecov](https://codecov.io/gh/Enoal-Fauchille-Bolle/Skylode/graph/badge.svg?token=V5S115AW3C)](https://codecov.io/gh/Enoal-Fauchille-Bolle/Skylode)
[![Release](https://img.shields.io/github/v/release/Enoal-Fauchille-Bolle/Skylode)](https://github.com/Enoal-Fauchille-Bolle/Skylode/releases)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A solo, terminal-based (TUI) idle/incremental mining game written in Rust,
inspired by PikaNetwork's SkyMines gamemode.

Hold a key, the grid empties, the enchants fire, the numbers grow. Spend the ore
on a better pickaxe and bigger mines, level up to open new worlds, then prestige
and do it faster.

## Status

**Playable, and pre-1.0.** The mining loop, the six screens, the five special
enchants, the auto-miner with offline accrual, prestige and the save system all
work. What is left before `1.0.0` is the tail of the balance work — see the
[roadmap](docs/ROADMAP.md).

Being below `1.0.0` is a statement, not a placeholder: **the minor number is the
breaking axis** until then, and `1.0.0` is reserved for the point where the MVP
list in the roadmap is complete. Save files carry their own version and migrate
forward on load, so a save written today is meant to keep opening.
[CONTRIBUTING.md](CONTRIBUTING.md#versioning-and-releases) has the details.

## Play

Download the archive for your platform from the
[releases page](https://github.com/Enoal-Fauchille-Bolle/Skylode/releases),
unpack it, and run `skylode`. Builds are published for Linux (x86_64), Windows
(x86_64) and macOS (Apple Silicon); `SHA256SUMS` ships alongside them.

`skylode --version` tells you which build you are holding once the archive name
is gone.

If you have a Rust toolchain, the registry is the shorter route:

```sh
cargo install skylode-tui
skylode
```

The package is `skylode-tui` and the binary is `skylode`. That is deliberate:
`skylode` is the whole game, rules and front-end together, while the package
holds only the second — so the name a player types belongs to the binary, and
the name the source tree carries belongs to the directory.

Every archive also carries a build provenance attestation, which answers a
question `SHA256SUMS` cannot: that file detects a corrupted download, but it is
published beside the binaries by the same account, so it says nothing about a
substituted one. The attestation is signed against a public transparency log and
names the workflow, the repository and the commit that produced the file:

```sh
gh attestation verify skylode-v0.2.0-x86_64-unknown-linux-gnu.tar.gz \
  --repo Enoal-Fauchille-Bolle/Skylode
```

Or build it yourself:

```sh
cargo run --release -p skylode-tui
```

`Space` mines. `Tab` cycles the six screens and `1`–`6` jump straight to one.
`?` lists every key, `s` opens the settings, and `q` backs out to the title
screen, where `q` again quits.

## Concept

Start with a Wooden pickaxe, mine ore blocks, and spend the ore to upgrade the
pickaxe (tier, Efficiency, Fortune) and grow your mines. Level up your mining to
unlock new worlds — Overworld, Nether, End — whose materials open new functions,
enchant the pickaxe with each world's enchant material, then prestige for a
permanent multiplier. There is no PvP, no multiplayer and no money: the economy
runs on ore directly.

The core loop:

1. Mine the selected mine (hold `Space`).
2. Ore accumulates in the inventory, and Fortune multiplies the yield.
3. Spend ore to upgrade the pickaxe and the mine.
4. A tier jump unlocks harder blocks but briefly lowers mining speed.
5. New worlds unlock new materials and functions.
6. At the soft cap, prestige for a permanent multiplier.

Full detail is in [docs/DESIGN.md](docs/DESIGN.md).

## Features

- **Two-axis progression**: mining level (XP) opens worlds, pickaxe tier opens
  mines. Neither alone advances you.
- **Three worlds and twelve mines**, each with its own block pool and signature
  ore, plus the per-world enchant materials (Lapis, Quartz, Amethyst).
- **Pickaxe upgrades**: tiers, Efficiency, Fortune and the Netherite
  enhancement, paid in composite (raw plus compressed) costs.
- **Five special enchants** — Explosive, Jackhammer, Nuke, Excavator, Haste —
  levelled per world, each with its own blast shape.
- **Per-mine size and richness.** Size grows from 3x3 to 20x10. Richness is a
  ceiling you buy permanently and a dial you then slide freely below it.
- **Temporary Haste boosts**, bought as charges and fired when you want them.
- **An auto-miner with offline accrual**, credited in closed form rather than by
  replaying ticks.
- **Prestige**: a deep reset for a permanent global multiplier.
- **A save system that takes itself seriously**: one JSON file, autosaved every
  ten seconds, written atomically, checked with an HMAC, with a `.bak` to fall
  back on and a migration for every format change so far.

## Tech stack

- Language: Rust, edition 2024, stable toolchain (pinned in
  `rust-toolchain.toml`).
- TUI: `ratatui` and `crossterm`.
- Serialization: `serde` and `serde_json`.
- RNG: `rand` and `rand_chacha` — a seeded `ChaCha8Rng` whose state lives in the
  save, so ticks are reproducible.
- Integrity: `sha2` and `hmac`.

More detail in [docs/SYSTEMS.md](docs/SYSTEMS.md).

## Project structure

```text
crates/
  skylode-core   game rules and state (deterministic, testable, no I/O)
  skylode-tui    terminal front-end (rendering and input), builds `skylode`
docs/            design documentation (see below)
```

The rules live in `skylode-core`, decoupled from the TUI so they stay testable
without a terminal and open to other front-ends later. The core compiles without
any source of ambient randomness, which is what makes its determinism a property
the compiler enforces rather than a convention.

## Development

```sh
cargo test                                       # the whole workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo doc-all -p skylode-core --open             # not `cargo doc`; see CONTRIBUTING
cargo tarpaulin --fail-under 94 --out Stdout     # coverage gate
```

[The CI](.github/workflows/ci.yml) runs all of these on every push — the checks
and the coverage gate on Linux, the tests on Linux and Windows — and a release
tag re-runs the lot before it publishes anything. A debug build also carries a
**dev menu**
(`SKYLODE_DEV=1`, then `` ` ``), compiled out of `--release` entirely — see
[docs/DEV-MENU.md](docs/DEV-MENU.md).

Install the git hooks once per clone with `.githooks/setup-hooks.sh`.
[CONTRIBUTING.md](CONTRIBUTING.md) has the rest: code style, commit-message
convention, and how versions and releases work.

## Documentation

**[docs/README.md](docs/README.md) routes by what you came to do** — it is the one to
open first. In short:

- [Design](docs/DESIGN.md): concept, scope, gameplay loop, screens.
- [Mechanics](docs/MECHANICS.md): mining, worlds, pickaxe, enchants, auto-miner,
  offline, prestige.
- [Balance](docs/BALANCE.md): every price, generated from the code.
- [Systems](docs/SYSTEMS.md): save system, tech stack, architecture.
- [UI](docs/UI.md): the front-end's screens, states and render loop.
- [Decisions](docs/decisions/): 157 numbered records — every settled decision and
  every rejected idea, one file each.
- [Guides](docs/guides/): how to add a mine, retune a curve, change the save format,
  add a screen.
- [Roadmap](docs/ROADMAP.md): MVP scope and what is left before `1.0.0`.
- [Phases](docs/PHASES.md): the dependency-ordered build plan, all of it shipped.
- [Dev menu](docs/DEV-MENU.md): reaching a state a test cannot play to.

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for build,
style, commit-message and versioning guidelines. The project is pre-1.0, so
please open an issue before starting a large change.

## License

Released under the MIT License. See [LICENSE](LICENSE).
