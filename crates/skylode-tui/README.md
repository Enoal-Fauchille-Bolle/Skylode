# skylode-tui

[Skylode](https://github.com/Enoal-Fauchille-Bolle/Skylode) is a solo,
terminal-based idle/incremental mining game written in Rust, inspired by
PikaNetwork's SkyMines gamemode.

Hold a key, the grid empties, the enchants fire, the numbers grow. Spend the ore
on a better pickaxe and bigger mines, level up to open new worlds, then prestige
and do it faster.

## Install

```sh
cargo install skylode-tui
skylode
```

The package is `skylode-tui` and the binary is `skylode`, which is not an
oversight: `skylode` is the *whole game* — these rules and this front-end
together — and this package holds only the second of the two. The name a player
types is the binary's; the name the source tree carries is the directory's.

If you would rather not compile it, every release ships prebuilt archives for
Linux (x86_64), Windows (x86_64) and macOS (Apple Silicon), each with a
`SHA256SUMS` and a build provenance attestation:
[releases](https://github.com/Enoal-Fauchille-Bolle/Skylode/releases).

## Playing

`Space` mines. `Tab` cycles the six screens and `1`–`6` jump straight to one.
`?` lists every key, `s` opens the settings, and `q` backs out to the title
screen, where `q` again quits. `skylode --version` tells you which build you are
holding.

Your save lives in the platform's own config directory, is written every ten
seconds, atomically, with an HMAC and a `.bak` to fall back on. Every format
change so far ships with a migration, so a save written today is meant to keep
opening.

## What this crate is

The front-end only: rendering with [`ratatui`](https://crates.io/crates/ratatui)
and input with [`crossterm`](https://crates.io/crates/crossterm). Every rule
lives in [`skylode-core`](https://crates.io/crates/skylode-core), which has no
terminal, no I/O and no clock in it — the boundary is what keeps the rules
testable without a tty, and other front-ends possible.

It is published as a crate so that `cargo install` works, not because it is meant
to be depended on. If you want the game's logic, take the core.

## Versioning

Pre-1.0, so **the minor number is the breaking axis**, and "breaking" here is
about what a player experiences — a save that no longer opens, a binding that
moved, progress that vanished — not about what a signature did. Details in
[CONTRIBUTING.md](https://github.com/Enoal-Fauchille-Bolle/Skylode/blob/main/CONTRIBUTING.md#versioning-and-releases).

## License

MIT. See
[LICENSE](https://github.com/Enoal-Fauchille-Bolle/Skylode/blob/main/LICENSE).
