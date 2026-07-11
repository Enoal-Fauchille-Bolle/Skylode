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

- The subject is one line of 72 characters at most, imperative mood, lowercase,
  with no trailing period (for example `feat(core): add fortune multiplier`).
- A body is optional; separate it from the subject with a blank line.
- The scope is optional; use it when the affected area is obvious (`core`, `tui`,
  `docs`).
- Types: `feat`, `fix`, `docs`, `refactor`, `chore`, `style`, `test`, `build`,
  `ci`. The list is closed: the `commit-msg` hook rejects anything else.

## Git hooks

The hooks in [.githooks/](.githooks/) check the rules above so a broken commit
never reaches the history. Git does not version the hooks path, so install them
once per clone:

```sh
.githooks/setup-hooks.sh     # macOS, Linux, WSL
.githooks/setup-hooks.ps1    # Windows, PowerShell
```

Both scripts do the same thing: `git config core.hooksPath .githooks`, which
points git at the versioned hooks instead of the local `.git/hooks/`. Undo it
with `git config --unset core.hooksPath`.

- **`pre-commit`** runs `cargo fmt --check`, `cargo check`, and `cargo clippy -D
  warnings` over the workspace, and refuses the commit on the first failure. It
  is the slow one, since it compiles.
- **`commit-msg`** validates the commit message against the rules above: type
  from the closed list, optional lowercase scope, subject in the imperative mood
  and 72 characters at most, no trailing period, body separated by a blank line.

Both hooks step aside where enforcing them would only get in the way:

- **On a `dev/*` branch**, both bypass every check. Work in progress stays cheap
  there; rewrite the history before merging into `main`.
- **On the messages git writes itself** (merges, reverts, `fixup!`/`squash!`),
  `commit-msg` bypasses validation. They cannot follow the convention and are not
  typed by hand. This is also why there is no `merge` type: a merge is already
  identified by its two parents, not by its message.

Skipping a hook for a single commit is `git commit --no-verify`. Reach for it
rarely, and never on `main`.

## Design context

Before proposing gameplay or systems changes, read the design documents in
[docs/](docs/), especially [DECISIONS.md](docs/DECISIONS.md), which records what
has already been settled or rejected and why.
