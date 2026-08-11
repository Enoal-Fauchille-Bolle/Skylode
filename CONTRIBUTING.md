# Contributing to Skylode

Thanks for your interest in Skylode. The game is **playable and pre-1.0**: the
mining loop, the six screens, the enchants, prestige and the save system all work.
What stands between here and `1.0.0` is the tail of the balance work — see
[docs/ROADMAP.md](docs/ROADMAP.md) for the scope, and the § *Versioning and
releases* below for what that version number promises. Code contributions are
welcome, but please open an issue to discuss before starting a large change, so
effort stays aligned with the roadmap.

## Prerequisites

- A stable Rust toolchain (the crates use edition 2024).
- `cargo` (bundled with Rust via [rustup](https://rustup.rs)).

## Build, run, and test

```sh
cargo build --release   # build the workspace
cargo test              # run the test suite
cargo run -p skylode-tui  # play it: Space mines, q quits
```

A debug build also carries a **dev menu** — ore out of nothing, free upgrades, a time
skip — behind two gates: `SKYLODE_DEV=1 cargo run -p skylode-tui`, then `` ` ``. It is
compiled out of `--release` entirely. See [docs/DEV-MENU.md](docs/DEV-MENU.md).

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
type(scope)!: subject
```

- The subject is one line of 72 characters at most, imperative mood, lowercase,
  with no trailing period (for example `feat(core): add fortune multiplier`). It
  may end with a closing parenthesis when it names code that way
  (`refactor(core): shut the free upgrade paths behind pub(crate)`), as long as
  the parentheses it opens are the ones it closes.
- A body is optional; separate it from the subject with a blank line.
- The scope is optional; use it when the affected area is obvious (`core`, `tui`,
  `docs`).
- The `!` is optional and marks a breaking change; it goes after the scope, just
  before the colon (`feat(core)!: make every block drop its own material`).
- Types: `feat`, `fix`, `docs`, `refactor`, `chore`, `style`, `test`, `build`,
  `ci`. The list is closed: the `commit-msg` hook rejects anything else.

## Versioning and releases

One version covers the whole workspace. It lives in `[workspace.package]` at the
repository root, and both crates inherit it with `version.workspace = true`. That
is a claim about the product rather than about the code: the two crates always ship
together, inside one binary, to one player. If `skylode-core` is ever consumed by
something other than this front-end, its version becomes an API contract and has to
move on its own.

Releases are **annotated** tags named `vX.Y.Z` (`git tag -a v0.2.0 -m "…"`). The
tag is the only thing that starts a release; nothing else does. Tags that are not
versions (backup markers, for example) go without the `v` prefix, which is what
keeps the two kinds apart.

### What the three numbers promise

Skylode is a game, not a library, so its public contract is not a set of `pub fn`
signatures. It is what a player relies on: **their save file opens**, **the keys do
what they did**, and **the run they are in the middle of still makes sense**. So the
question to ask of any release is the one a player would ask — *if I update without
doing anything, do I lose something?*

- **Breaking** — a save from the previous version cannot be loaded, a binding
  changed under the player's fingers, or progress they had banked is gone.
- **Added** — new content or a new capability, with everything that worked still
  working.
- **Fixed** — corrections only; nothing moved.

Note that "breaking" is about what the player experiences, not about what the code
changed. A save-format change shipped **with a migration** is not breaking: the file
still opens. A change that leaves every signature intact but desynchronises existing
saves — a different RNG draw order, say — *is* breaking, and no compiler will say so.

### Which number moves

While the version is below `1.0.0`, **Cargo treats the minor as the breaking axis**
(`0.2.3` resolves as `>=0.2.3, <0.3.0`), which differs from upstream SemVer's "no
guarantees before 1.0". So, pre-1.0:

| Change | Bump |
| --- | --- |
| Breaking, or added | `0.MINOR.0` |
| Fixed | `0.x.PATCH` |

Against the commit types above: `feat` and any `!` take the minor; `fix`,
`refactor`, `style` and `perf` take the patch; `docs`, `chore`, `test`, `build` and
`ci` do not justify a release on their own. Lower components reset to zero — after
`0.4.7`, a feature gives `0.5.0`, not `0.5.7`.

`1.0.0` is reserved for one condition, and it is checkable rather than a matter of
taste: **the MVP list in [docs/ROADMAP.md](docs/ROADMAP.md) is complete**. Tagging it
is a promise that breaking anything afterwards costs a `2.0.0`.

### `SAVE_VERSION` is a separate number

`skylode_core::save::SAVE_VERSION` is a migration selector, not a version of the
game. It advances only when the on-disk document changes shape, for reasons that
have nothing to do with the product's maturity — it reached `3` across two releases
nobody shipped. Tying the two together would force a format bump for nothing, or
make a bugfix release lie about the format. The relationship belongs in the release
notes as prose (*"`SAVE_VERSION` 3 → 4, migrated on load"*), never in the numbers.

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
  from the closed list, optional lowercase scope, optional breaking-change `!`,
  subject in the imperative mood and 72 characters at most, no trailing period,
  balanced parentheses, body separated by a blank line.

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

To reach a state past the first hour of a run without playing to it, use the dev menu
([docs/DEV-MENU.md](docs/DEV-MENU.md)).
