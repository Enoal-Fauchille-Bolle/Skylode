# Contributing to Skylode

Thanks for your interest in Skylode. The project is in a pre-MVP design phase:
the design is settled (see [docs/](docs/)) but the game is not yet playable. Code
contributions are welcome, but please open an issue to discuss before starting a
large change, so effort stays aligned with the roadmap.

## Prerequisites

- A stable Rust toolchain (the crates use edition 2024).
- `cargo` (bundled with Rust via [rustup](https://rustup.rs)).

## Build, run, and test

```sh
cargo build --release   # build the workspace
cargo test              # run the test suite
cargo run -p skylode-tui  # run the front-end (currently a stub)
```

## Code style

- Format with `cargo fmt` before committing.
- Keep the workspace lint-clean with `cargo clippy`. The workspace denies the
  `correctness` and `suspicious` lint groups and warns on `unwrap`, `expect`, and
  `panic` (see `[workspace.lints.clippy]` in `Cargo.toml`). Prefer returning
  errors over panicking.
- Game rules belong in `skylode-core` (kept deterministic and testable); the TUI
  in `skylode-tui` only renders and forwards input. See
  [docs/SYSTEMS.md](docs/SYSTEMS.md).

## Commit messages

This project follows [Conventional Commits](https://www.conventionalcommits.org):

```text
type(scope): subject
```

- One line, imperative mood, lowercase, no trailing period (for example
  `feat(core): add fortune multiplier to drops`).
- The scope is optional; use it when the affected area is obvious (`core`, `tui`,
  `docs`).
- Common types: `feat`, `fix`, `docs`, `refactor`, `chore`, `style`, `test`,
  `build`, `ci`.

## Design context

Before proposing gameplay or systems changes, read the design documents in
[docs/](docs/), especially [DECISIONS.md](docs/DECISIONS.md), which records what
has already been settled or rejected and why.
