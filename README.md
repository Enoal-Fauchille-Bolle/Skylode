# Skylode

A solo, terminal-based (TUI) idle/incremental mining game written in Rust,
inspired by PikaNetwork's SkyMines gamemode.

## Status

Pre-MVP, design phase. The game is not yet playable: the design is settled and
the workspace compiles, but the TUI front-end is still a stub. See the
[roadmap](docs/ROADMAP.md) for scope.

## Concept

Start with a Wooden pickaxe, mine ore cubes, and spend the ores to upgrade the
pickaxe (tier plus Efficiency plus Fortune). Unlock new worlds (Overworld, Nether,
End) whose materials open new functions, then prestige for permanent multipliers.
There is no PvP, multiplayer, or money: the economy runs on ores directly.

The core loop:

1. Mine the selected ore cube (hold Space).
2. Ores accumulate in the inventory (Fortune multiplies yield).
3. Spend ores to upgrade the pickaxe.
4. A tier jump unlocks harder ores but temporarily lowers mining speed.
5. New worlds unlock new materials and functions.
6. At the soft cap, prestige for a permanent multiplier.

Full detail is in [docs/DESIGN.md](docs/DESIGN.md).

## Planned features

All of the following are planned, not yet implemented:

- Core mining loop with progressive block breaking and an instamine endgame.
- Pickaxe upgrades: tiers, Efficiency, Fortune, with composite (compressed plus
  raw) costs.
- Three worlds with materials that open distinct functions.
- Amethyst enchants: Explosive, Jackhammer, Nuke, Excavator, Haste.
- A basic auto-miner with offline accrual.
- Prestige.
- A robust save system (JSON, autosave, atomic writes, HMAC integrity, `.bak`
  recovery).

## Tech stack

- Language: Rust.
- TUI: `ratatui` and `crossterm`.
- Serialization: `serde` and `serde_json`.
- RNG: `rand` (seeded, state in the save, for deterministic ticks).
- Integrity: `sha2` and `hmac`.

More detail in [docs/SYSTEMS.md](docs/SYSTEMS.md).

## Project structure

```text
crates/
  skylode-core   game rules and state (deterministic, testable)
  skylode-tui    terminal front-end (rendering and input)
docs/            design documentation (see below)
```

The game rules live in `skylode-core`, decoupled from the TUI, so they stay
testable and open to other front-ends later.

## Build

```sh
cargo build --release
```

## Run

Not yet available: the TUI is still a stub. Usage instructions will be added once
the game is runnable.

## Documentation

- [Design](docs/DESIGN.md): concept, scope, gameplay loop, screens.
- [Mechanics](docs/MECHANICS.md): mining, worlds, pickaxe, enchants, auto-miner,
  offline, prestige.
- [Systems](docs/SYSTEMS.md): save system, tech stack, architecture.
- [Roadmap](docs/ROADMAP.md): MVP scope, post-MVP, open questions.
- [Decisions](docs/DECISIONS.md): settled decisions and rejected ideas.

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for build,
style, and commit-message guidelines. The project is pre-MVP, so please open an
issue before starting a large change.

## License

Released under the MIT License. See [LICENSE](LICENSE).
